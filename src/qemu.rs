use crate::config::{Accelerator, Arch, DriveConfig, DriveFormat, DriveInterface, VmConfig};
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
}

impl QemuCapabilities {
    /// Whether anything at all was learned from QEMU.
    pub fn was_probed(&self) -> bool {
        !self.machines.is_empty()
            || !self.devices.is_empty()
            || !self.netdev_backends.is_empty()
            || !self.accelerators.is_empty()
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
/// — it spawns four processes and must never run as part of drawing a frame.
pub fn probe_capabilities(binary: &Path) -> QemuCapabilities {
    QemuCapabilities {
        machines: query_help(binary, "-machine"),
        devices: query_help(binary, "-device"),
        netdev_backends: query_help(binary, "-netdev"),
        accelerators: query_help(binary, "-accel"),
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

/// Build the drive arguments for a VM.
///
/// `virtio` and `ide` are bus types QEMU really has, and keep the short
/// `if=` form. `sata`, `nvme` and `scsi` are not: QEMU rejects `if=sata` and
/// `if=nvme` as unsupported bus types and refuses `if=scsi` on q35 outright.
/// Those get an anonymous backing drive plus an explicit device, and their
/// controller is added once however many disks hang off it.
pub fn build_drive_args(drives: &[DriveConfig]) -> Vec<String> {
    let mut args = Vec::new();
    let mut ahci_added = false;
    let mut scsi_added = false;
    let mut ahci_unit = 0u32;
    let mut scsi_unit = 0u32;

    for (idx, drive) in drives.iter().enumerate() {
        let file = escape_opt_value(&drive.path.display().to_string());
        let format = drive.format.as_str();
        let id = format!("{}-drive-{}", ID_PREFIX, idx);

        match drive.interface {
            DriveInterface::VirtIO | DriveInterface::Ide => {
                let mut spec = format!(
                    "file={},format={},if={}",
                    file,
                    format,
                    drive.interface.as_str()
                );
                if let Some(index) = drive.boot_index {
                    spec.push_str(&format!(",bootindex={}", index));
                }
                args.push("-drive".to_string());
                args.push(spec);
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
                let mut device =
                    format!("scsi-hd,drive={},bus={}-scsi.{}", id, ID_PREFIX, scsi_unit);
                append_bootindex(&mut device, drive.boot_index);
                args.push("-device".to_string());
                args.push(device);
                scsi_unit += 1;
            }
        }
    }

    args
}

/// Build the CD-ROM arguments for a VM.
///
/// `-cdrom` is shorthand for a single drive at a fixed index, so giving it
/// twice makes QEMU refuse to start with "drive with bus=2, unit=0 exists".
/// Each ISO therefore becomes its own CD-ROM drive at a distinct index.
/// Indices start at 2 because 0 and 1 are taken by the automatic slots.
pub fn build_cdrom_args(cdroms: &[PathBuf]) -> Vec<String> {
    let mut args = Vec::new();
    let mut index = 2u32;

    for cdrom in cdroms {
        if cdrom.as_os_str().is_empty() {
            continue;
        }
        args.push("-drive".to_string());
        args.push(format!(
            "media=cdrom,index={},file={}",
            index,
            escape_opt_value(&cdrom.display().to_string())
        ));
        index += 1;
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

/// Construct command-line arguments to launch QEMU for the given `VmConfig`.
pub fn build_qemu_args(vm: &VmConfig) -> Vec<String> {
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
    args.extend(build_drive_args(&vm.drives));

    // 5. CD-ROMs
    args.extend(build_cdrom_args(&vm.cdroms));

    // 6. BIOS / UEFI
    if let Some(bios) = &vm.bios_uefi {
        if !bios.as_os_str().is_empty() {
            args.push("-bios".to_string());
            args.push(bios.display().to_string());
        }
    }

    // 7. Display and VGA
    let vga_type = vm.display.vga_type.trim();
    if vga_type.eq_ignore_ascii_case("none") {
        args.push("-vga".to_string());
        args.push("none".to_string());
    } else if !vga_type.is_empty() {
        let vga = if vga_type.eq_ignore_ascii_case("virtio-vga") {
            "virtio"
        } else {
            vga_type
        };
        args.push("-vga".to_string());
        args.push(vga.to_string());
    }

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

    // Absolute pointing device (USB tablet) so the mouse cursor is always visible and synced
    let has_tablet = vm.extra_args.contains("tablet") || vm.extra_args.contains("mouse");
    if !has_tablet && !vga_type.eq_ignore_ascii_case("none") {
        args.push("-usb".to_string());
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

    args
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
