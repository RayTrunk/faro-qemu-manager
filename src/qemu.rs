use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::config::{Accelerator, Arch, DriveFormat, VmConfig};

/// Resolve the accelerator string passed to QEMU.
/// If `Accelerator::Auto`, picks WHPX on Windows, KVM on Linux, and TCG as fallback.
pub fn resolve_accelerator(accel: Accelerator) -> &'static str {
    match accel {
        Accelerator::Auto => {
            #[cfg(target_os = "windows")]
            {
                "whpx"
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
        Accelerator::WHPX => "whpx",
        Accelerator::HAXM => "haxm",
        Accelerator::KVM => "kvm",
        Accelerator::TCG => "tcg",
    }
}

/// Helper to search for candidate binary names in custom directory, PATH, and standard directories.
fn search_binary_candidates(custom_dir: Option<&Path>, candidate_names: &[String]) -> Option<PathBuf> {
    // 1. Check custom directory
    if let Some(dir) = custom_dir {
        if dir.is_file() {
            if candidate_names
                .iter()
                .any(|c| dir.file_name().is_some_and(|f| f == c.as_str()))
                || dir.exists()
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
        args.push("-cpu".to_string());
        args.push(cpu_model.to_string());
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
    for drive in &vm.drives {
        let mut drive_spec = format!(
            "file={},format={},if={}",
            drive.path.display(),
            drive.format.as_str(),
            drive.interface.as_str()
        );
        if let Some(boot_idx) = drive.boot_index {
            drive_spec.push_str(&format!(",bootindex={}", boot_idx));
        }
        args.push("-drive".to_string());
        args.push(drive_spec);
    }

    // 5. CD-ROMs
    for cdrom in &vm.cdroms {
        if !cdrom.as_os_str().is_empty() {
            args.push("-cdrom".to_string());
            args.push(cdrom.display().to_string());
        }
    }

    // 6. BIOS / UEFI
    if let Some(bios) = &vm.bios_uefi {
        if !bios.as_os_str().is_empty() {
            args.push("-bios".to_string());
            args.push(bios.display().to_string());
        }
    }

    // 7. Display and VGA
    let vga_type = vm.display.vga_type.trim();
    if !vga_type.is_empty() && !vga_type.eq_ignore_ascii_case("none") {
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
        args.push("-display".to_string());
        args.push(display_backend.to_string());
    }

    if vm.display.fullscreen {
        args.push("-full-screen".to_string());
    }

    // 8. Network configuration
    let backend = if vm.network.backend.trim().is_empty() {
        "user"
    } else {
        vm.network.backend.trim()
    };

    if !backend.eq_ignore_ascii_case("none") {
        let mut netdev = format!("{},id=net0", backend);
        for rule in &vm.network.port_forwards {
            netdev.push_str(&format!(
                ",hostfwd={}::{}-:{}",
                rule.proto, rule.host_port, rule.guest_port
            ));
        }
        args.push("-netdev".to_string());
        args.push(netdev);

        let nic = if vm.network.nic_model.trim().is_empty() {
            "virtio-net-pci"
        } else {
            vm.network.nic_model.trim()
        };
        args.push("-device".to_string());
        args.push(format!("{},netdev=net0", nic));
    }

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
        .map_err(|e| format!("Failed to execute qemu-img at '{}': {}", qemu_img.display(), e))?;

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

/// Handle to a running virtual machine process.
pub struct VmProcess {
    /// Operating system process ID of the child process.
    pub pid: u32,
    /// Channel to signal process termination to supervisor.
    pub kill_tx: Sender<()>,
    child: Arc<Mutex<Child>>,
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
        if !self.alive.load(Ordering::SeqCst) {
            return false;
        }
        if let Ok(mut guard) = self.child.try_lock() {
            if let Ok(Some(_status)) = guard.try_wait() {
                self.alive.store(false, Ordering::SeqCst);
                return false;
            }
        }
        self.alive.load(Ordering::SeqCst)
    }

    /// Terminate the virtual machine process.
    pub fn kill(&mut self) -> Result<(), String> {
        if !self.is_alive() {
            return Ok(());
        }
        let mut guard = self.child.lock().map_err(|e| e.to_string())?;
        if let Ok(Some(_)) = guard.try_wait() {
            self.alive.store(false, Ordering::SeqCst);
            return Ok(());
        }
        let res = guard.kill().map_err(|e| e.to_string());
        self.alive.store(false, Ordering::SeqCst);
        let _ = self.kill_tx.send(());
        res
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
                            let _ = tx_stdout.send(QemuEvent::Error(format!("Stdout read error: {}", e)));
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
                            let _ = tx_stderr.send(QemuEvent::Error(format!("Stderr read error: {}", e)));
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

    let (kill_tx, kill_rx) = channel();
    let child_arc = Arc::new(Mutex::new(child));
    let alive = Arc::new(AtomicBool::new(true));

    let sup_child = Arc::clone(&child_arc);
    let sup_alive = Arc::clone(&alive);
    let sup_tx = event_tx;

    std::thread::Builder::new()
        .name("qemu-supervisor".to_string())
        .spawn(move || {
            let mut exit_code = None;
            loop {
                // 1. Check if kill signal was received
                match kill_rx.try_recv() {
                    Ok(()) => {
                        if let Ok(mut guard) = sup_child.lock() {
                            let _ = guard.kill();
                            if let Ok(status) = guard.wait() {
                                exit_code = status.code();
                            }
                        }
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        // VmProcess was dropped; wait for child process to finish
                        if let Ok(mut guard) = sup_child.lock() {
                            if let Ok(status) = guard.wait() {
                                exit_code = status.code();
                            }
                        }
                        break;
                    }
                    Err(TryRecvError::Empty) => {}
                }

                // 2. Check if process exited on its own
                let finished = match sup_child.lock() {
                    Ok(mut guard) => match guard.try_wait() {
                        Ok(Some(status)) => {
                            exit_code = status.code();
                            true
                        }
                        Ok(None) => false,
                        Err(e) => {
                            let _ = sup_tx.send(QemuEvent::Error(e.to_string()));
                            true
                        }
                    },
                    Err(_) => true,
                };

                if finished {
                    break;
                }

                std::thread::sleep(Duration::from_millis(15));
            }

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
        .map_err(|e| {
            if let Ok(mut guard) = child_arc.lock() {
                let _ = guard.kill();
            }
            format!("Failed to spawn supervisor thread: {}", e)
        })?;

    Ok(VmProcess {
        pid,
        kill_tx,
        child: child_arc,
        alive,
    })
}


