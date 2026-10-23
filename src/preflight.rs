// FARO QEMU MANAGER (C) R.Trunk 2026
// What can be checked before a start, so the interface can say it instead of
// leaving the user to read the console afterwards.

use std::path::Path;

use crate::config::{Accelerator, DriveInterface, Firmware, VmConfig};
use crate::i18n::I18n;
use crate::qemu::{resolve_accelerator, QemuCapabilities, UefiFirmware};
use crate::ui::vm_detail::offered_netdev_backends;

/// How much a finding matters.
///
/// The line is from the design document, section 5.6: only something missing
/// blocks, oversizing warns. A machine with more memory than the host has
/// will run badly, and that is the user's call; a machine whose disk is not
/// there cannot run at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Blocking,
    Warning,
}

/// One thing worth saying before the start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub text: String,
}

/// What the host this is running on has to offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostFacts {
    pub cores: u32,
    pub memory_mb: u64,
}

impl HostFacts {
    /// Ask the host. Core count is in the standard library; total memory is
    /// not, so that one goes through a platform call.
    pub fn probe() -> Self {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1);
        Self {
            cores,
            // A host that will not say its memory must never produce a
            // warning, so an unknown amount counts as plenty.
            memory_mb: host_memory_mb().unwrap_or(u64::MAX / (1024 * 1024)),
        }
    }
}

/// Total physical memory in MB, or None when the host will not say.
#[cfg(target_os = "windows")]
fn host_memory_mb() -> Option<u64> {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }

    let mut status = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    // Safety: the struct matches MEMORYSTATUSEX field for field, its length
    // field is set as the call requires, and the pointer is to a live local.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
    (ok && status.total_phys > 0).then_some(status.total_phys / (1024 * 1024))
}

#[cfg(not(target_os = "windows"))]
fn host_memory_mb() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

/// The AHCI controller SATA disks hang off has six ports.
const AHCI_PORTS: usize = 6;

/// Whether any finding holds the start.
pub fn blocks(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.severity == Severity::Blocking)
}

/// Everything worth saying about this machine before it starts.
///
/// `caps` is what the installed QEMU reported, or None when none was found.
/// With None, free-text values are **not** held against the user: section
/// 11.6 says such options are shown and marked unverified, and blocking a
/// start that would have worked is worse than letting it try.
pub fn check_vm(
    vm: &VmConfig,
    host: &HostFacts,
    caps: Option<&QemuCapabilities>,
    uefi: Option<&UefiFirmware>,
    i18n: &I18n,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    // --- missing files: these block ---------------------------------------
    for drive in &vm.drives {
        if !path_is_present(&drive.path) {
            findings.push(blocking(
                i18n,
                "pf_missing_disk",
                &[("path", &drive.path.display().to_string())],
            ));
        }
    }
    for iso in &vm.cdroms {
        if !path_is_present(iso) {
            findings.push(blocking(
                i18n,
                "pf_missing_iso",
                &[("path", &iso.display().to_string())],
            ));
        }
    }
    match &vm.firmware {
        Firmware::Uefi { nvram } => {
            if uefi.is_none() {
                findings.push(blocking(i18n, "pf_missing_firmware", &[("path", "edk2")]));
            }
            if !path_is_present(nvram) {
                findings.push(blocking(
                    i18n,
                    "pf_missing_nvram",
                    &[("path", &nvram.display().to_string())],
                ));
            }
        }
        Firmware::Bios { rom: Some(rom) } => {
            if !path_is_present(rom) {
                findings.push(blocking(
                    i18n,
                    "pf_missing_firmware",
                    &[("path", &rom.display().to_string())],
                ));
            }
        }
        Firmware::Bios { rom: None } => {}
    }

    // --- stored values this build would run as something else: these block -
    if let Some(replacement) = substituted_accelerator(vm.accel) {
        findings.push(blocking(
            i18n,
            "pf_accel_substituted",
            &[
                ("stored", &vm.accel.as_str().to_uppercase()),
                ("replacement", &replacement.to_uppercase()),
            ],
        ));
    }
    let backend = vm.network.backend.trim().to_string();
    if !backend.is_empty() && !offered_netdev_backends(caps).contains(&backend.as_str()) {
        findings.push(blocking(
            i18n,
            "pf_backend_unavailable",
            &[("backend", &backend)],
        ));
    }

    // --- values only a probed QEMU can judge ------------------------------
    if let Some(caps) = caps.filter(|c| c.was_probed()) {
        let machine = vm.machine_type.trim();
        if !machine.is_empty() && !caps.has_machine(machine) {
            findings.push(blocking(
                i18n,
                "pf_machine_unknown",
                &[("machine", machine)],
            ));
        }
    }

    // --- oversizing and limits: these warn --------------------------------
    if u64::from(vm.memory_mb) > host.memory_mb {
        findings.push(warning(
            i18n,
            "pf_memory_over_host",
            &[
                ("vm", &vm.memory_mb.to_string()),
                ("host", &host.memory_mb.to_string()),
            ],
        ));
    }
    if vm.cpu_cores > host.cores {
        findings.push(warning(
            i18n,
            "pf_cores_over_host",
            &[
                ("vm", &vm.cpu_cores.to_string()),
                ("host", &host.cores.to_string()),
            ],
        ));
    }
    if !vm.network.port_forwards.is_empty() && !backend.eq_ignore_ascii_case("user") {
        findings.push(warning(i18n, "forwards_backend_only", &[]));
    }
    let sata = vm
        .drives
        .iter()
        .filter(|d| d.interface == DriveInterface::Sata)
        .count();
    if sata > AHCI_PORTS {
        findings.push(warning(
            i18n,
            "pf_too_many_sata",
            &[
                ("count", &sata.to_string()),
                ("ports", &AHCI_PORTS.to_string()),
            ],
        ));
    }

    findings
}

/// Whether a configured path points at something that is there.
///
/// An empty path is not a file and not a complaint: it means the slot was
/// left blank.
fn path_is_present(path: &Path) -> bool {
    path.as_os_str().is_empty() || path.exists()
}

/// What this build would actually run instead of the stored accelerator.
///
/// `Auto` is not a substitution — choosing it means asking for whatever fits.
/// Anything else that resolves to a different accelerator is the silent
/// replacement section 11.6 forbids, and the start is held so the user can
/// decide rather than discover it later.
fn substituted_accelerator(accel: Accelerator) -> Option<&'static str> {
    if accel == Accelerator::Auto {
        return None;
    }
    let resolved = resolve_accelerator(accel);
    let head = resolved.split(',').next().unwrap_or(resolved);
    (!head.eq_ignore_ascii_case(accel.as_str())).then_some(head)
}

fn blocking(i18n: &I18n, key: &str, args: &[(&str, &str)]) -> Finding {
    Finding {
        severity: Severity::Blocking,
        text: i18n.tf(key, args),
    }
}

fn warning(i18n: &I18n, key: &str, args: &[(&str, &str)]) -> Finding {
    Finding {
        severity: Severity::Warning,
        text: i18n.tf(key, args),
    }
}
