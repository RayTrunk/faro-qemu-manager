// FARO QEMU MANAGER (C) R.Trunk 2026
// What the interface says before the start, instead of the console after it.

mod common;

use std::path::{Path, PathBuf};
use tempfile::tempdir;

use faro_qem::config::{
    Accelerator, Arch, DriveConfig, DriveFormat, DriveInterface, Firmware, PortForwardRule,
    VmConfig,
};
use faro_qem::i18n::I18n;
use faro_qem::preflight::{blocks, check_vm, Finding, HostFacts, Severity};
use faro_qem::qemu::{find_qemu_binary, probe_capabilities, QemuCapabilities};

/// A fixed host, so the tests do not depend on the machine running them.
fn host() -> HostFacts {
    HostFacts {
        cores: 8,
        memory_mb: 16_384,
    }
}

/// A machine with nothing wrong with it: a disk that exists, plain BIOS,
/// user-mode networking, and sizes well inside the host.
fn healthy_vm(dir: &Path) -> VmConfig {
    let disk = dir.join("disk.qcow2");
    std::fs::write(&disk, b"not really a qcow2, but it exists").unwrap();
    let mut vm = VmConfig::new_default("healthy", Arch::X86_64);
    vm.cpu_cores = 2;
    vm.memory_mb = 2048;
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: None,
    });
    vm
}

/// Capabilities really probed from the installed QEMU, or None without one.
fn probed_caps() -> Option<QemuCapabilities> {
    let bin = find_qemu_binary(None, Arch::X86_64)?;
    let caps = probe_capabilities(&bin);
    caps.was_probed().then_some(caps)
}

fn blocking(findings: &[Finding]) -> Vec<&Finding> {
    findings
        .iter()
        .filter(|f| f.severity == Severity::Blocking)
        .collect()
}

#[test]
fn test_a_missing_disk_blocks_and_names_the_file() {
    let mut vm = VmConfig::new_default("broken", Arch::X86_64);
    vm.drives.push(DriveConfig {
        path: PathBuf::from(r"C:\nope\disk.qcow2"),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: None,
    });
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    let blocking = blocking(&findings);
    assert_eq!(blocking.len(), 1, "got: {:?}", findings);
    assert!(
        blocking[0].text.contains("disk.qcow2"),
        "got: {}",
        blocking[0].text
    );
}

#[test]
fn test_a_missing_iso_and_a_missing_store_block_too() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.cdroms.push(PathBuf::from(r"C:\nope\install.iso"));
    vm.firmware = Firmware::Uefi {
        nvram: PathBuf::from(r"C:\nope\nvram.fd"),
    };
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    // Three things are missing: the ISO, the variable store, and the firmware
    // the UEFI machine needs, since the caller passed None for it.
    assert!(blocking(&findings).len() >= 2, "got: {:?}", findings);
    let all = findings
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(all.contains("install.iso"), "got: {}", all);
    assert!(all.contains("nvram.fd"), "got: {}", all);
}

#[test]
fn test_an_empty_cdrom_slot_is_not_a_missing_file() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.cdroms.push(PathBuf::new());
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.is_empty(), "got: {:?}", findings);
}

#[test]
fn test_more_memory_than_the_host_has_warns_but_does_not_block() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.memory_mb = 1_000_000;
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(
        findings.iter().any(|f| f.severity == Severity::Warning),
        "got: {:?}",
        findings
    );
    assert!(
        !blocks(&findings),
        "oversizing is the user's call, not a refusal"
    );
}

#[test]
fn test_more_cores_than_the_host_has_warns() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.cpu_cores = host().cores + 1;
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.iter().any(|f| f.severity == Severity::Warning));
    assert!(!blocks(&findings));
}

#[test]
fn test_using_every_core_of_the_host_is_not_a_warning() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.cpu_cores = host().cores;
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.is_empty(), "got: {:?}", findings);
}

#[test]
fn test_a_stored_accelerator_this_build_cannot_run_blocks_and_names_the_replacement() {
    // Spec 11.6: never rewritten in silence, and the start is held. HAXM is
    // absent from modern Windows builds and would be bent to WHPX.
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.accel = Accelerator::HAXM;
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));

    if cfg!(target_os = "windows") {
        let blocking = blocking(&findings);
        assert_eq!(blocking.len(), 1, "got: {:?}", findings);
        let text = blocking[0].text.to_uppercase();
        assert!(
            text.contains("HAXM"),
            "name the stored value, got: {}",
            text
        );
        assert!(
            text.contains("WHPX") || text.contains("TCG"),
            "name the replacement, got: {}",
            text
        );
    } else {
        assert!(!blocks(&findings), "hax exists on other hosts");
    }
}

#[test]
fn test_an_accelerator_that_runs_as_chosen_is_not_flagged() {
    let dir = tempdir().unwrap();
    for accel in [Accelerator::Auto, Accelerator::TCG] {
        let mut vm = healthy_vm(dir.path());
        vm.accel = accel;
        let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
        assert!(findings.is_empty(), "{:?} gave: {:?}", accel, findings);
    }
}

#[test]
fn test_a_network_backend_this_qemu_lacks_blocks() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.network.backend = "tap".to_string();
    let Some(caps) = probed_caps() else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let findings = check_vm(&vm, &host(), Some(&caps), None, &I18n::new("en"));
    assert!(blocks(&findings), "got: {:?}", findings);
}

#[test]
fn test_an_unavailable_backend_says_what_would_be_used_instead() {
    // Spec section 11.6: a stored value this build cannot use is kept, and
    // the interface names what to use instead. Being told "no such backend"
    // and nothing else leaves the user to guess.
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.network.backend = "tap".to_string();
    let Some(caps) = probed_caps() else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let findings = check_vm(&vm, &host(), Some(&caps), None, &I18n::new("en"));
    let said = findings
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("user"),
        "the replacement belongs in the text, got: {}",
        said
    );
}

#[test]
fn test_an_unknown_machine_type_says_what_would_be_used_instead() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.machine_type = "pc-i440fx".to_string();
    let Some(caps) = probed_caps() else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let findings = check_vm(&vm, &host(), Some(&caps), None, &I18n::new("en"));
    let said = findings
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("q35"),
        "the replacement belongs in the text, got: {}",
        said
    );
}

#[test]
fn test_a_machine_type_the_running_qemu_does_not_know_blocks() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    // A name QEMU rejects outright: only the versioned pc-i440fx-* exist.
    vm.machine_type = "pc-i440fx".to_string();
    let Some(caps) = probed_caps() else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let findings = check_vm(&vm, &host(), Some(&caps), None, &I18n::new("en"));
    assert!(blocks(&findings), "got: {:?}", findings);
}

#[test]
fn test_a_machine_type_the_running_qemu_knows_is_fine() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.machine_type = "q35".to_string();
    let Some(caps) = probed_caps() else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let findings = check_vm(&vm, &host(), Some(&caps), None, &I18n::new("en"));
    assert!(findings.is_empty(), "got: {:?}", findings);
}

#[test]
fn test_without_a_probe_a_free_text_value_is_not_held_against_the_user() {
    // Spec 11.6: with no QEMU found, options are shown but marked unverified.
    // Blocking here would hold a start that would have worked.
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.machine_type = "something-exotic".to_string();
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(!blocks(&findings), "got: {:?}", findings);
}

#[test]
fn test_rules_that_the_backend_cannot_apply_warn() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.network.backend = "none".to_string();
    vm.network.port_forwards.push(PortForwardRule {
        host_port: 2222,
        guest_port: 22,
        proto: "tcp".to_string(),
    });
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.iter().any(|f| f.severity == Severity::Warning));
    assert!(!blocks(&findings), "got: {:?}", findings);
}

#[test]
fn test_more_than_six_sata_disks_warns_because_ahci_has_six_ports() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    for n in 0..7 {
        let p = dir.path().join(format!("extra-{}.qcow2", n));
        std::fs::write(&p, b"x").unwrap();
        vm.drives.push(DriveConfig {
            path: p,
            format: DriveFormat::Qcow2,
            interface: DriveInterface::Sata,
            boot_index: None,
        });
    }
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(
        findings
            .iter()
            .any(|f| f.text.to_lowercase().contains("ahci")),
        "got: {:?}",
        findings
    );
    assert!(!blocks(&findings), "it is a limit, not a missing file");
}

#[test]
fn test_six_sata_disks_are_still_fine() {
    let dir = tempdir().unwrap();
    let mut vm = VmConfig::new_default("six", Arch::X86_64);
    for n in 0..6 {
        let p = dir.path().join(format!("d-{}.qcow2", n));
        std::fs::write(&p, b"x").unwrap();
        vm.drives.push(DriveConfig {
            path: p,
            format: DriveFormat::Qcow2,
            interface: DriveInterface::Sata,
            boot_index: None,
        });
    }
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.is_empty(), "got: {:?}", findings);
}

#[test]
fn test_a_healthy_vm_has_nothing_to_report() {
    let dir = tempdir().unwrap();
    let findings = check_vm(
        &healthy_vm(dir.path()),
        &host(),
        None,
        None,
        &I18n::new("en"),
    );
    assert!(findings.is_empty(), "got: {:?}", findings);
}

#[test]
fn test_the_host_probe_answers_with_plausible_numbers() {
    let h = HostFacts::probe();
    assert!(h.cores >= 1);
    assert!(
        h.memory_mb >= 256,
        "a host with less than 256 MB cannot be running this, got {}",
        h.memory_mb
    );
}

#[test]
fn test_the_findings_are_in_all_three_catalogues() {
    for lang in ["en", "de", "fr"] {
        let keys = common::catalogue_keys(lang);
        for key in [
            "pf_missing_disk",
            "pf_missing_iso",
            "pf_missing_nvram",
            "pf_missing_firmware",
            "pf_memory_over_host",
            "pf_cores_over_host",
            "pf_accel_substituted",
            "pf_backend_unavailable",
            "pf_machine_unknown",
            "pf_too_many_sata",
        ] {
            assert!(keys.contains(key), "{} missing in {}", key, lang);
        }
    }
}

#[test]
fn test_launching_a_vm_with_a_blocking_finding_does_not_start_qemu() {
    let dir = tempdir().unwrap();
    let mut app = common::app_with_vm_dir(&dir.path().join("FARO-VMs"));
    let mut vm = VmConfig::new_default("broken", Arch::X86_64);
    vm.drives.push(DriveConfig {
        path: PathBuf::from(r"C:\nope\disk.qcow2"),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: None,
    });
    let id = vm.id;
    app.vms.push(vm);

    let err = app
        .launch_vm(id, None)
        .expect_err("a missing disk holds the start");
    assert!(err.contains("disk.qcow2"), "got: {}", err);
    assert!(app.running_processes.is_empty());
}
