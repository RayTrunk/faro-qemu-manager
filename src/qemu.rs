use std::path::{Path, PathBuf};
use std::process::Command;
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
