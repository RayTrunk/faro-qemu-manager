use crate::config::{
    Accelerator, Arch, DriveConfig, DriveFormat, DriveInterface, Firmware, VmConfig,
};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// Resolve the accelerator string passed to QEMU.
/// If `Accelerator::Auto`, picks WHPX on Windows, KVM on Linux, and TCG as fallback.
pub fn resolve_accelerator(accel: Accelerator) -> &'static str {
    match accel {
        Accelerator::Auto => {
            #[cfg(target_os = "windows")]
            {
                "whpx,kernel-irqchip=off"
            }
            #[cfg(target_os = "linux")]
            {
                "kvm"
            }
            #[cfg(not(any(target_os = "windows", target_os = "linux")))]
            {
                "tcg"
            }
        }
        Accelerator::WHPX => "whpx,kernel-irqchip=off",
        Accelerator::HAXM => {
            #[cfg(target_os = "windows")]
            {
                // HAXM is omitted in modern Windows 11 QEMU binaries; fallback to WHPX or TCG
                "whpx,kernel-irqchip=off"
            }
            #[cfg(not(target_os = "windows"))]
            {
                "hax"
            }
        }
        Accelerator::KVM => {
            #[cfg(target_os = "windows")]
            {
                "whpx,kernel-irqchip=off"
            }
            #[cfg(not(target_os = "windows"))]
            {
                "kvm"
            }
        }
        Accelerator::TCG => "tcg",
    }
}

/// Helper to search for candidate binary names in custom directory, PATH, and standard directories.
fn search_binary_candidates(
    custom_dir: Option<&Path>,
    candidate_names: &[String],
) -> Option<PathBuf> {
    // 1. Check custom directory
    if let Some(dir) = custom_dir {
        if dir.is_file() {
            if candidate_names
                .iter()
                .any(|c| dir.file_name().is_some_and(|f| f == c.as_str()))
            {
                return Some(dir.to_path_buf());
            }
        } else if dir.is_dir() {
            for name in candidate_names {
                let candidate_path = dir.join(name);
                if candidate_path.is_file() {
                    return Some(candidate_path);
                }
            }
        }
    }

    // 2. Search PATH environment variable
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            for name in candidate_names {
                let candidate_path = path.join(name);
                if candidate_path.is_file() {
                    return Some(candidate_path);
                }
            }
        }
    }

    // 3. Search platform-specific default installation directories
    #[cfg(target_os = "windows")]
    {
        let mut dirs = vec![
            PathBuf::from(r"C:\Program Files\qemu"),
            PathBuf::from(r"C:\Program Files (x86)\qemu"),
            PathBuf::from(r"C:\qemu"),
        ];

        if let Some(pf) = std::env::var_os("ProgramFiles") {
            dirs.push(PathBuf::from(pf).join("qemu"));
        }
        if let Some(pf86) = std::env::var_os("ProgramFiles(x86)") {
            dirs.push(PathBuf::from(pf86).join("qemu"));
        }
        if let Some(local_app) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local_app).join("Programs").join("qemu"));
        }

        for dir in dirs {
            for name in candidate_names {
                let candidate_path = dir.join(name);
                if candidate_path.is_file() {
                    return Some(candidate_path);
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let dirs = [
            PathBuf::from("/usr/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/opt/local/bin"),
            PathBuf::from("/usr/pkg/bin"),
            PathBuf::from("/snap/bin"),
        ];

        for dir in &dirs {
            for name in candidate_names {
                let candidate_path = dir.join(name);
                if candidate_path.is_file() {
                    return Some(candidate_path);
                }
            }
        }
    }

    None
}

/// Find the QEMU emulator binary for a specific architecture.
/// Checks `custom_dir` if specified, followed by the system PATH and standard installation paths.
pub fn find_qemu_binary(custom_dir: Option<&Path>, arch: Arch) -> Option<PathBuf> {
    let suffix = arch.qemu_binary_suffix();
    let candidate_names = if cfg!(windows) {
        vec![
            format!("qemu-system-{}.exe", suffix),
            format!("qemu-system-{}", suffix),
            format!("qemu-system-{}w.exe", suffix),
        ]
    } else {
        vec![
            format!("qemu-system-{}", suffix),
            format!("qemu-system-{}.exe", suffix),
        ]
    };

    search_binary_candidates(custom_dir, &candidate_names)
}

/// Find the `qemu-img` binary used for disk image manipulation.
/// Checks `custom_dir` if specified, followed by the system PATH and standard installation paths.
pub fn find_qemu_img(custom_dir: Option<&Path>) -> Option<PathBuf> {
    let candidate_names = if cfg!(windows) {
        vec!["qemu-img.exe".to_string(), "qemu-img".to_string()]
    } else {
        vec!["qemu-img".to_string(), "qemu-img.exe".to_string()]
    };

    search_binary_candidates(custom_dir, &candidate_names)
}

/// What the installed QEMU can do, as reported by its own help output.
///
/// An empty list means **not probed**, not "nothing available" — so the
/// `has_*` queries answer `true` for an empty list. Hiding an option because
/// we failed to ask would tell the user something is unavailable when we
/// simply do not know.
#[derive(Debug, Clone, Default)]
pub struct QemuCapabilities {
    pub machines: Vec<String>,
    pub devices: Vec<String>,
    pub netdev_backends: Vec<String>,
    pub accelerators: Vec<String>,
    /// Display backends. These cannot be checked by launching QEMU, because the
    /// acceptance probe passes `-display none` itself and the last one wins.
    pub displays: Vec<String>,
}

impl QemuCapabilities {
    /// Whether anything at all was learned from QEMU.
    pub fn was_probed(&self) -> bool {
        !self.machines.is_empty()
            || !self.devices.is_empty()
            || !self.netdev_backends.is_empty()
            || !self.accelerators.is_empty()
            || !self.displays.is_empty()
    }

    pub fn has_machine(&self, name: &str) -> bool {
        list_allows(&self.machines, name)
    }

    pub fn has_device(&self, name: &str) -> bool {
        list_allows(&self.devices, name)
    }

    pub fn has_netdev(&self, name: &str) -> bool {
        list_allows(&self.netdev_backends, name)
    }

    pub fn has_accelerator(&self, name: &str) -> bool {
        list_allows(&self.accelerators, name)
    }

    pub fn has_display(&self, name: &str) -> bool {
        list_allows(&self.displays, name)
    }
}

/// An unprobed (empty) list permits everything; a probed one is authoritative.
fn list_allows(list: &[String], name: &str) -> bool {
    list.is_empty() || list.iter().any(|n| n == name)
}

/// Run one `-<what> help` query and collect the names it reports.
fn query_help(binary: &Path, flag: &str) -> Vec<String> {
    let mut cmd = Command::new(binary);
    cmd.arg(flag);
    cmd.arg("help");
    cmd.stdin(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let Ok(output) = cmd.output() else {
        return Vec::new();
    };
    // A binary that rejects the flag writes complaints, not names. Reading
    // those as options would make the list non-empty — and a non-empty list
    // counts as authoritative, which is the exact inversion to avoid.
    if !output.status.success() {
        return Vec::new();
    }

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text.lines().flat_map(parse_help_names).collect()
}

/// Pull every usable identifier out of one line of QEMU help output.
///
/// The forms differ by query. `-device help` writes
/// `name "ich9-ahci", bus PCI, alias "ahci"` — and the alias is what one
/// actually passes to `-device`, so **both** names count; taking only the
/// primary one would report AHCI as unavailable on a QEMU that has it.
/// `-machine help` and `-netdev help` lead with the bare name instead.
/// Heading lines such as "Available netdev backend types:" carry no name.
fn parse_help_names(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.ends_with(':') {
        return Vec::new();
    }

    if trimmed.starts_with("name \"") {
        return quoted_values_after(trimmed, "name \"")
            .into_iter()
            .chain(quoted_values_after(trimmed, "alias \""))
            .collect();
    }

    let Some(first) = trimmed.split_whitespace().next() else {
        return Vec::new();
    };
    // Prose lines ("Some display backends support suboptions, …") start with a
    // capitalised word; QEMU identifiers never do.
    if first.chars().next().is_some_and(|c| c.is_uppercase()) {
        return Vec::new();
    }
    vec![first.trim_matches('"').to_string()]
}

/// Every value introduced by `marker` and closed by the next quote.
fn quoted_values_after(line: &str, marker: &str) -> Vec<String> {
    line.match_indices(marker)
        .filter_map(|(at, _)| {
            line[at + marker.len()..]
                .split('"')
                .next()
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
        })
        .collect()
}

/// Ask `binary` what it supports. Each query runs exactly once.
///
/// Call this when the QEMU directory is chosen or changed, and keep the result
/// — it spawns five processes and must never run as part of drawing a frame.
pub fn probe_capabilities(binary: &Path) -> QemuCapabilities {
    QemuCapabilities {
        machines: query_help(binary, "-machine"),
        devices: query_help(binary, "-device"),
        netdev_backends: query_help(binary, "-netdev"),
        accelerators: query_help(binary, "-accel"),
        displays: query_help(binary, "-display"),
    }
}

/// Escape a value inside a QEMU option list.
///
/// A comma separates options, so a comma that belongs to the value — a path
/// like `C:\My VMs\disk,1.qcow2` — must be doubled or QEMU reads the tail as
/// another option and the drive specification falls apart.
fn escape_opt_value(value: &str) -> String {
    value.replace(',', ",,")
}

/// Prefix for every identifier this builder invents, so they cannot collide
/// with anything the user wrote into `extra_args`.
const ID_PREFIX: &str = "faro";

/// Append an anonymous backing drive for a device-attached disk.
fn push_backing_drive(args: &mut Vec<String>, file: &str, format: &str, id: &str) {
    args.push("-drive".to_string());
    args.push(format!("file={},format={},if=none,id={}", file, format, id));
}

fn append_bootindex(device: &mut String, boot_index: Option<u32>) {
    if let Some(index) = boot_index {
        device.push_str(&format!(",bootindex={}", index));
    }
}

/// How many drives one IDE bus of `machine` carries.
///
/// q35's built-in controller is AHCI: six buses of one unit each. The i440fx
/// machines keep the classic PIIX pair, two buses of two units. Guessing wrong
/// produces `Can't create IDE unit 1, bus supports only 1 units` or
/// `Bus 'ide.2' not found`, so the number is read off the machine name. Both
/// mappings are proven against the installed QEMU by the acceptance tests.
fn ide_units_per_bus(machine: &str) -> u32 {
    if machine.starts_with("q35") {
        1
    } else {
        2
    }
}

/// How a VM's IDE disks and CD-ROMs share the machine's IDE slots.
///
/// The short `if=ide` form lets QEMU allocate slots itself, which works on
/// every machine type and is therefore the default. It cannot carry a
/// `bootindex`, though — with a `format=` beside it the option lands in the
/// block-format namespace and QEMU answers `Block format 'qcow2' does not
/// support the option 'bootindex'`. So one IDE disk asking for a boot index
/// moves the whole IDE group, CD-ROMs included, onto explicit devices, and
/// then the slots must be handed out here: a `-drive media=cdrom` left on the
/// short form would grab a slot an explicit `ide-hd` already holds.
pub struct IdeLayout {
    explicit: bool,
    next_slot: u32,
    units_per_bus: u32,
}

impl IdeLayout {
    /// Slots allocated by QEMU, the form that works on every machine.
    pub fn new(machine: &str) -> Self {
        IdeLayout {
            explicit: false,
            next_slot: 0,
            units_per_bus: ide_units_per_bus(machine),
        }
    }

    /// Whether disks and CD-ROMs are placed by explicit device arguments.
    pub fn is_explicit(&self) -> bool {
        self.explicit
    }

    /// The `bus=`/`unit=` suffix for the next free IDE slot.
    fn take_slot(&mut self) -> String {
        let bus = self.next_slot / self.units_per_bus;
        let unit = self.next_slot % self.units_per_bus;
        self.next_slot += 1;
        format!(",bus=ide.{},unit={}", bus, unit)
    }
}

/// Build the drive arguments for a VM running on `machine`.
///
/// Every interface but `ide` gets an anonymous backing drive plus an explicit
/// device: `sata`, `nvme` and `scsi` because QEMU has no such `if=` bus type,
/// and `virtio` because only the device form accepts a boot index. Controllers
/// are added once however many disks hang off them.
///
/// Returns the arguments and the IDE layout the CD-ROMs must continue.
pub fn build_drive_args(drives: &[DriveConfig], machine: &str) -> (Vec<String>, IdeLayout) {
    let mut args = Vec::new();
    let mut ahci_added = false;
    let mut scsi_added = false;
    let mut ahci_unit = 0u32;

    let mut ide = IdeLayout::new(machine);
    ide.explicit = drives
        .iter()
        .any(|d| d.interface == DriveInterface::Ide && d.boot_index.is_some());

    for (idx, drive) in drives.iter().enumerate() {
        let file = escape_opt_value(&drive.path.display().to_string());
        let format = drive.format.as_str();
        let id = format!("{}-drive-{}", ID_PREFIX, idx);

        match drive.interface {
            DriveInterface::VirtIO => {
                push_backing_drive(&mut args, &file, format, &id);
                let mut device = format!("virtio-blk-pci,drive={}", id);
                append_bootindex(&mut device, drive.boot_index);
                args.push("-device".to_string());
                args.push(device);
            }

            DriveInterface::Ide => {
                if ide.explicit {
                    push_backing_drive(&mut args, &file, format, &id);
                    let mut device = format!("ide-hd,drive={}{}", id, ide.take_slot());
                    append_bootindex(&mut device, drive.boot_index);
                    args.push("-device".to_string());
                    args.push(device);
                } else {
                    args.push("-drive".to_string());
                    args.push(format!("file={},format={},if=ide", file, format));
                }
            }

            DriveInterface::Sata => {
                if !ahci_added {
                    args.push("-device".to_string());
                    args.push(format!("ahci,id={}-ahci", ID_PREFIX));
                    ahci_added = true;
                }
                push_backing_drive(&mut args, &file, format, &id);
                let mut device =
                    format!("ide-hd,drive={},bus={}-ahci.{}", id, ID_PREFIX, ahci_unit);
                append_bootindex(&mut device, drive.boot_index);
                args.push("-device".to_string());
                args.push(device);
                ahci_unit += 1;
            }

            DriveInterface::Nvme => {
                push_backing_drive(&mut args, &file, format, &id);
                // QEMU refuses an nvme device that has no serial.
                let mut device = format!("nvme,drive={},serial={}-{}", id, ID_PREFIX, idx);
                append_bootindex(&mut device, drive.boot_index);
                args.push("-device".to_string());
                args.push(device);
            }

            DriveInterface::Scsi => {
                if !scsi_added {
                    args.push("-device".to_string());
                    args.push(format!("virtio-scsi-pci,id={}-scsi", ID_PREFIX));
                    scsi_added = true;
                }
                push_backing_drive(&mut args, &file, format, &id);
                // virtio-scsi-pci has exactly one bus; disks on it are told
                // apart by the scsi-id and lun QEMU assigns, not by the bus.
                let mut device = format!("scsi-hd,drive={},bus={}-scsi.0", id, ID_PREFIX);
                append_bootindex(&mut device, drive.boot_index);
                args.push("-device".to_string());
                args.push(device);
            }
        }
    }

    (args, ide)
}

/// Build the CD-ROM arguments for a VM, continuing `ide`.
///
/// `-cdrom` is shorthand for a single drive at a fixed slot, so giving it
/// twice makes QEMU refuse to start with "drive with bus=2, unit=0 exists".
/// A fixed `index=` has the same defect one step later: IDE disks occupy the
/// same slots, and three of them plus an ISO brings the collision back. So no
/// index is named at all — QEMU hands out the next free slot — unless the
/// disks are on explicit devices, in which case the CD-ROMs must be too.
pub fn build_cdrom_args(cdroms: &[PathBuf], ide: &mut IdeLayout) -> Vec<String> {
    let mut args = Vec::new();

    for (idx, cdrom) in cdroms.iter().enumerate() {
        if cdrom.as_os_str().is_empty() {
            continue;
        }
        let file = escape_opt_value(&cdrom.display().to_string());

        if ide.explicit {
            let id = format!("{}-cdrom-{}", ID_PREFIX, idx);
            args.push("-drive".to_string());
            args.push(format!("media=cdrom,if=none,id={},file={}", id, file));
            args.push("-device".to_string());
            args.push(format!("ide-cd,drive={}{}", id, ide.take_slot()));
        } else {
            args.push("-drive".to_string());
            args.push(format!("media=cdrom,file={}", file));
        }
    }

    args
}

/// Build the network arguments for a VM.
pub fn build_network_args(net: &crate::config::NetworkConfig) -> Vec<String> {
    let mut args = Vec::new();

    let backend = if net.backend.trim().is_empty() {
        "user"
    } else {
        net.backend.trim()
    };

    if backend.eq_ignore_ascii_case("none") {
        args.push("-nic".to_string());
        args.push("none".to_string());
        return args;
    }

    let mut netdev = format!("{},id=net0", backend);

    // hostfwd belongs to the user-mode backend alone. QEMU answers
    // "Invalid parameter 'hostfwd'" on tap, so the rules stay in the stored
    // configuration but do not reach the command line. The UI says where they
    // do not apply.
    if backend.eq_ignore_ascii_case("user") {
        for rule in &net.port_forwards {
            netdev.push_str(&format!(
                ",hostfwd={}::{}-:{}",
                rule.proto, rule.host_port, rule.guest_port
            ));
        }
    }
    args.push("-netdev".to_string());
    args.push(netdev);

    let nic = if net.nic_model.trim().is_empty() {
        "virtio-net-pci"
    } else {
        net.nic_model.trim()
    };
    args.push("-device".to_string());
    args.push(format!("{},netdev=net0", nic));

    args
}

/// The two files a UEFI machine needs.
pub struct UefiFirmware {
    /// Read-only firmware code image.
    pub code: PathBuf,
    /// Pristine variable store, copied once per VM.
    pub vars_template: PathBuf,
}

/// Locate the UEFI firmware shipped with QEMU.
///
/// Deliberately the plain code image, not `edk2-x86_64-secure-code.fd`: with
/// the stock variable template there are no enrolled keys, so the secure build
/// would be called Secure Boot while enforcing nothing. QEMU ships no
/// `edk2-x86_64-vars.fd`; the i386 variable store is the one to use for x86_64
/// as well.
pub fn find_uefi_firmware(custom_dir: Option<&Path>) -> Option<UefiFirmware> {
    const CODE: &str = "edk2-x86_64-code.fd";
    const VARS: &str = "edk2-i386-vars.fd";

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(dir) = custom_dir {
        let base = if dir.is_file() {
            dir.parent().map(|p| p.to_path_buf())
        } else {
            Some(dir.to_path_buf())
        };
        if let Some(base) = base {
            roots.push(base.join("share"));
            roots.push(base.clone());
        }
    }
    if let Some(bin) = find_qemu_binary(custom_dir, Arch::X86_64) {
        if let Some(base) = bin.parent() {
            roots.push(base.join("share"));
            roots.push(base.to_path_buf());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        roots.push(PathBuf::from("/usr/share/qemu"));
        roots.push(PathBuf::from("/usr/share/edk2/x64"));
        roots.push(PathBuf::from("/usr/share/OVMF"));
    }

    for root in roots {
        let code = root.join(CODE);
        let vars = root.join(VARS);
        if code.is_file() && vars.is_file() {
            return Some(UefiFirmware {
                code,
                vars_template: vars,
            });
        }
    }
    None
}

/// Give this VM its own copy of the UEFI variable store.
///
/// Does nothing when the target already exists: that file holds the guest's
/// boot entries, and replacing it would make an installed system unbootable.
/// Where a machine's UEFI variable store belongs: beside its first disk, so
/// it travels with the machine, named after the machine's own id.
///
/// The id in the name is what keeps two machines apart. A duplicate that
/// keeps the original's path points both at one file, and they overwrite each
/// other's boot entries — so this is the one place that builds the name, and
/// both the firmware switch and the duplicate call it.
pub fn nvram_path_for(vm: &VmConfig) -> PathBuf {
    vm.drives
        .first()
        .and_then(|d| d.path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default()
        .join(format!("{}-nvram.fd", vm.id))
}

pub fn prepare_nvram(template: &Path, target: &Path) -> Result<(), String> {
    if target.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }
    }
    std::fs::copy(template, target).map(|_| ()).map_err(|e| {
        format!(
            "Failed to copy {} to {}: {}",
            template.display(),
            target.display(),
            e
        )
    })
}

/// Build the firmware arguments for a VM.
///
/// UEFI needs the pflash pair, not `-bios`. `-bios` boots the firmware but
/// discards the variable store, and Windows writes only an NVRAM boot entry —
/// no `\EFI\BOOT\BOOTX64.EFI` fallback — so an installation completes and then
/// will not boot.
///
/// A UEFI VM whose firmware could not be found emits nothing at all rather
/// than silently starting as BIOS; the start check reports what is missing.
pub fn build_firmware_args(fw: &Firmware, uefi: Option<&UefiFirmware>) -> Vec<String> {
    match fw {
        Firmware::Bios { rom: None } => Vec::new(),
        Firmware::Bios { rom: Some(path) } => {
            vec!["-bios".to_string(), path.display().to_string()]
        }
        Firmware::Uefi { nvram } => match uefi {
            None => Vec::new(),
            Some(found) => vec![
                "-drive".to_string(),
                format!(
                    "if=pflash,format=raw,unit=0,readonly=on,file={}",
                    escape_opt_value(&found.code.display().to_string())
                ),
                "-drive".to_string(),
                format!(
                    "if=pflash,format=raw,unit=1,file={}",
                    escape_opt_value(&nvram.display().to_string())
                ),
            ],
        },
    }
}

/// Build the graphics arguments for `arch`.
///
/// `-vga` is an x86 convenience. Elsewhere QEMU has no VGA adapter at all and
/// answers `Virtio VGA not available`, so the adapter is a plain PCI device —
/// and an old configuration naming the x86 spelling means the same hardware,
/// so it is translated rather than refused.
fn build_vga_args(arch: Arch, vga_type: &str) -> Vec<String> {
    if vga_type.is_empty() {
        return Vec::new();
    }
    if vga_type.eq_ignore_ascii_case("none") {
        return vec!["-vga".to_string(), "none".to_string()];
    }
    match arch {
        Arch::X86_64 | Arch::I386 => {
            let name = if vga_type.eq_ignore_ascii_case("virtio-vga") {
                "virtio"
            } else {
                vga_type
            };
            vec!["-vga".to_string(), name.to_string()]
        }
        _ => {
            let device = if vga_type.eq_ignore_ascii_case("virtio-vga") {
                "virtio-gpu-pci"
            } else {
                vga_type
            };
            vec!["-device".to_string(), device.to_string()]
        }
    }
}

/// Construct command-line arguments to launch QEMU for the given `VmConfig`.
///
/// `uefi` is the firmware pair the caller resolved, which is also how the
/// configured QEMU directory reaches this function. Searching for it here
/// would hide filesystem work inside an argument builder and — worse — would
/// miss a portable QEMU, leaving a UEFI machine to start as BIOS.
///
/// A UEFI machine without firmware is refused rather than built: a guest
/// installed under BIOS can never boot again once the firmware is found, so
/// the quiet fallback destroys exactly the installation it was meant to save.
pub fn build_qemu_args(vm: &VmConfig, uefi: Option<&UefiFirmware>) -> Result<Vec<String>, String> {
    if matches!(vm.firmware, Firmware::Uefi { .. }) && uefi.is_none() {
        return Err(format!(
            "No UEFI firmware found for {}. Starting this machine as BIOS would              install a guest that can never boot again, so the launch is refused.              Set the QEMU directory in the settings.",
            vm.name
        ));
    }

    let mut args = Vec::new();

    // 1. Machine and accelerator
    let machine_type = if vm.machine_type.trim().is_empty() {
        vm.arch.default_machine()
    } else {
        vm.machine_type.trim()
    };
    let accel_flag = resolve_accelerator(vm.accel);
    args.push("-machine".to_string());
    args.push(format!("{},accel={}", machine_type, accel_flag));

    // 2. CPU model and core count
    let cpu_model = vm.cpu_model.trim();
    if !cpu_model.is_empty() {
        // "host" requires KVM on Linux or WHPX on Windows. If TCG fallback, map "host" to "max".
        let effective_cpu = if cpu_model.eq_ignore_ascii_case("host") && accel_flag == "tcg" {
            "max"
        } else {
            cpu_model
        };
        args.push("-cpu".to_string());
        args.push(effective_cpu.to_string());
    }
    if vm.cpu_cores > 0 {
        args.push("-smp".to_string());
        args.push(vm.cpu_cores.to_string());
    }

    // 3. Memory
    if vm.memory_mb > 0 {
        args.push("-m".to_string());
        args.push(format!("{}M", vm.memory_mb));
    }

    // 4. Drives
    let (drive_args, mut ide) = build_drive_args(&vm.drives, machine_type);
    args.extend(drive_args);

    // 5. CD-ROMs, continuing the IDE slots the disks left free
    args.extend(build_cdrom_args(&vm.cdroms, &mut ide));

    // 6. Firmware
    args.extend(build_firmware_args(&vm.firmware, uefi));

    // 7. Display and VGA
    let vga_type = vm.display.vga_type.trim();
    args.extend(build_vga_args(vm.arch, vga_type));

    let display_backend = vm.display.display_backend.trim();
    if !display_backend.is_empty() {
        let display_arg = if vga_type.eq_ignore_ascii_case("none") {
            display_backend.to_string()
        } else if display_backend.eq_ignore_ascii_case("default") {
            "default,show-cursor=on".to_string()
        } else if display_backend.eq_ignore_ascii_case("gtk")
            || display_backend.eq_ignore_ascii_case("sdl")
        {
            format!("{},show-cursor=on", display_backend)
        } else {
            display_backend.to_string()
        };
        args.push("-display".to_string());
        args.push(display_arg);
    }

    if vm.display.fullscreen {
        args.push("-full-screen".to_string());
    }

    // Absolute pointing device (USB tablet) so the mouse cursor is always
    // visible and synced. `-usb` adds a controller on the x86 machines only;
    // elsewhere QEMU answers "No 'usb-bus' bus found for device 'usb-tablet'",
    // so the controller is named explicitly.
    let has_tablet = vm.extra_args.contains("tablet") || vm.extra_args.contains("mouse");
    if !has_tablet && !vga_type.eq_ignore_ascii_case("none") {
        match vm.arch {
            Arch::X86_64 | Arch::I386 => args.push("-usb".to_string()),
            _ => {
                args.push("-device".to_string());
                args.push("qemu-xhci".to_string());
            }
        }
        args.push("-device".to_string());
        args.push("usb-tablet".to_string());
    }

    // 8. Network configuration
    args.extend(build_network_args(&vm.network));

    // 9. Extra arguments (parsed with shell escaping)
    if !vm.extra_args.trim().is_empty() {
        match shell_words::split(&vm.extra_args) {
            Ok(words) => args.extend(words),
            Err(_) => {
                args.extend(vm.extra_args.split_whitespace().map(String::from));
            }
        }
    }

    Ok(args)
}

/// Create a new virtual disk image using `qemu-img create`.
pub fn create_disk_image(
    qemu_img: &Path,
    path: &Path,
    format: DriveFormat,
    size_gb: u32,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }
    }

    let output = Command::new(qemu_img)
        .arg("create")
        .arg("-f")
        .arg(format.as_str())
        .arg(path)
        .arg(format!("{}G", size_gb))
        .output()
        .map_err(|e| {
            format!(
                "Failed to execute qemu-img at '{}': {}",
                qemu_img.display(),
                e
            )
        })?;

    if output.status.success() {
        Ok(())
    } else {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        let out_msg = String::from_utf8_lossy(&output.stdout);
        let msg = if !err_msg.trim().is_empty() {
            err_msg.trim()
        } else if !out_msg.trim().is_empty() {
            out_msg.trim()
        } else {
            "Unknown error"
        };
        Err(format!(
            "qemu-img failed with exit code {:?}: {}",
            output.status.code(),
            msg
        ))
    }
}

/// Events emitted during the lifecycle of a QEMU process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QemuEvent {
    /// The process has started with the given PID.
    Started(u32),
    /// A line of standard output was produced.
    Stdout(String),
    /// A line of standard error was produced.
    Stderr(String),
    /// The process exited with an optional exit code.
    Exited(Option<i32>),
    /// An error occurred during process execution or log streaming.
    Error(String),
}

/// Terminate a process by operating system process ID
fn kill_pid(pid: u32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let mut cmd = Command::new("taskkill");
        cmd.args(["/F", "/PID", &pid.to_string()]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.status();
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
        Ok(())
    }
}

/// Handle to a running virtual machine process.
pub struct VmProcess {
    /// Operating system process ID of the child process.
    pub pid: u32,
    alive: Arc<AtomicBool>,
}

impl std::fmt::Debug for VmProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VmProcess")
            .field("pid", &self.pid)
            .field("alive", &self.alive.load(Ordering::SeqCst))
            .finish()
    }
}

impl VmProcess {
    /// Get the process ID of the child process.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Check if the virtual machine process is currently running.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Terminate the virtual machine process.
    pub fn kill(&mut self) -> Result<(), String> {
        if !self.is_alive() {
            return Ok(());
        }
        self.alive.store(false, Ordering::SeqCst);
        kill_pid(self.pid)
    }
}

/// Spawn a QEMU process in the background with real-time stdout and stderr log streaming.
///
/// Sends `QemuEvent::Started(pid)` immediately after spawning, streams `Stdout` and `Stderr`
/// lines asynchronously via dedicated background reader threads, and monitors termination
/// via a supervisor thread that emits `QemuEvent::Exited(exit_code)`.
pub fn spawn_qemu(
    binary: &Path,
    args: &[String],
    event_tx: Sender<QemuEvent>,
) -> Result<VmProcess, String> {
    let mut cmd = Command::new(binary);
    cmd.args(args);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let err = format!("Failed to spawn '{}': {}", binary.display(), e);
            let _ = event_tx.send(QemuEvent::Error(err.clone()));
            return Err(err);
        }
    };

    let pid = child.id();
    let _ = event_tx.send(QemuEvent::Started(pid));

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let tx_stdout = event_tx.clone();
    let stdout_handle = if let Some(pipe) = stdout {
        let handle = std::thread::Builder::new()
            .name("qemu-stdout".to_string())
            .spawn(move || {
                let reader = std::io::BufReader::new(pipe);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            if tx_stdout.send(QemuEvent::Stdout(l)).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx_stdout
                                .send(QemuEvent::Error(format!("Stdout read error: {}", e)));
                            break;
                        }
                    }
                }
            })
            .map_err(|e| {
                let _ = child.kill();
                format!("Failed to spawn stdout reader thread: {}", e)
            })?;
        Some(handle)
    } else {
        None
    };

    let tx_stderr = event_tx.clone();
    let stderr_handle = if let Some(pipe) = stderr {
        let handle = std::thread::Builder::new()
            .name("qemu-stderr".to_string())
            .spawn(move || {
                let reader = std::io::BufReader::new(pipe);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            if tx_stderr.send(QemuEvent::Stderr(l)).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx_stderr
                                .send(QemuEvent::Error(format!("Stderr read error: {}", e)));
                            break;
                        }
                    }
                }
            })
            .map_err(|e| {
                let _ = child.kill();
                format!("Failed to spawn stderr reader thread: {}", e)
            })?;
        Some(handle)
    } else {
        None
    };

    let alive = Arc::new(AtomicBool::new(true));
    let sup_alive = Arc::clone(&alive);
    let sup_tx = event_tx;

    std::thread::Builder::new()
        .name("qemu-supervisor".to_string())
        .spawn(move || {
            let exit_code = match child.wait() {
                Ok(status) => status.code(),
                Err(e) => {
                    let _ = sup_tx.send(QemuEvent::Error(format!("Wait error: {}", e)));
                    None
                }
            };

            // Ensure stdout and stderr reader threads finish streaming before Exited event
            if let Some(h) = stdout_handle {
                let _ = h.join();
            }
            if let Some(h) = stderr_handle {
                let _ = h.join();
            }

            sup_alive.store(false, Ordering::SeqCst);
            let _ = sup_tx.send(QemuEvent::Exited(exit_code));
        })
        .map_err(|e| format!("Failed to spawn supervisor thread: {}", e))?;

    Ok(VmProcess { pid, alive })
}
