use faro_qem::config::{
    Accelerator, Arch, DisplayConfig, DriveConfig, DriveFormat, DriveInterface, PortForwardRule,
    VmConfig,
};
use faro_qem::qemu::{build_qemu_args, create_disk_image, find_qemu_binary, find_qemu_img};
use std::fs::File;
use std::path::PathBuf;

#[test]
fn test_build_qemu_args_basic() {
    let mut vm = VmConfig::new_default("Test Linux", Arch::X86_64);
    vm.cpu_cores = 4;
    vm.memory_mb = 4096;
    vm.accel = Accelerator::TCG;
    vm.extra_args = "-usb -device usb-tablet".to_string();

    let args = build_qemu_args(&vm);
    assert!(args.contains(&"-m".to_string()));
    assert!(args.contains(&"4096M".to_string()));
    assert!(args.contains(&"-smp".to_string()));
    assert!(args.contains(&"4".to_string()));
    assert!(args.contains(&"-usb".to_string()));
    assert!(args.contains(&"usb-tablet".to_string()));
    let joined = args.join(" ");
    assert!(joined.contains("-machine q35,accel=tcg"));
}

#[test]
fn test_build_qemu_args_drives_and_net() {
    let mut vm = VmConfig::new_default("Win11", Arch::X86_64);
    vm.drives.push(DriveConfig {
        path: PathBuf::from("disk.qcow2"),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: Some(1),
    });
    vm.network.port_forwards.push(PortForwardRule {
        host_port: 8080,
        guest_port: 80,
        proto: "tcp".to_string(),
    });

    let args = build_qemu_args(&vm);
    let joined = args.join(" ");
    assert!(joined.contains("format=qcow2"));
    assert!(joined.contains("hostfwd=tcp::8080-:80"));
    assert!(joined.contains("bootindex=1"));
    assert!(joined.contains("-device virtio-net-pci,netdev=net0"));
}

#[test]
fn test_build_qemu_args_all_components() {
    let mut vm = VmConfig::new_default("Comprehensive VM", Arch::Aarch64);
    vm.cpu_model = "cortex-a72".to_string();
    vm.cpu_cores = 8;
    vm.memory_mb = 8192;
    vm.accel = Accelerator::TCG;
    vm.bios_uefi = Some(PathBuf::from("/path/to/bios.bin"));
    vm.cdroms.push(PathBuf::from("/path/to/install.iso"));
    vm.display = DisplayConfig {
        vga_type: "virtio-vga".to_string(),
        display_backend: "default".to_string(),
        fullscreen: true,
    };

    let args = build_qemu_args(&vm);
    let joined = args.join(" ");
    assert!(joined.contains("-machine virt,accel=tcg"));
    assert!(joined.contains("-cpu cortex-a72"));
    assert!(joined.contains("-smp 8"));
    assert!(joined.contains("-m 8192M"));
    assert!(joined.contains("-bios /path/to/bios.bin"));
    assert!(joined.contains("-cdrom /path/to/install.iso"));
    assert!(joined.contains("-vga virtio"));
    assert!(joined.contains("-display default"));
    assert!(joined.contains("-full-screen"));
}

#[test]
fn test_build_qemu_args_accel_auto() {
    let vm = VmConfig::new_default("Auto Accel VM", Arch::X86_64);
    let args = build_qemu_args(&vm);
    let joined = args.join(" ");

    #[cfg(target_os = "windows")]
    assert!(joined.contains("accel=whpx"));

    #[cfg(target_os = "linux")]
    assert!(joined.contains("accel=kvm"));

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    assert!(joined.contains("accel=tcg"));
}

#[test]
fn test_find_qemu_binary_with_custom_dir() {
    let temp = tempfile::tempdir().expect("Failed to create tempdir");
    let bin_name = if cfg!(windows) {
        "qemu-system-x86_64.exe"
    } else {
        "qemu-system-x86_64"
    };
    let mock_bin = temp.path().join(bin_name);
    File::create(&mock_bin).expect("Failed to create mock binary");

    let found = find_qemu_binary(Some(temp.path()), Arch::X86_64);
    assert_eq!(found, Some(mock_bin));
}

#[test]
fn test_find_qemu_img_with_custom_dir() {
    let temp = tempfile::tempdir().expect("Failed to create tempdir");
    let img_name = if cfg!(windows) {
        "qemu-img.exe"
    } else {
        "qemu-img"
    };
    let mock_img = temp.path().join(img_name);
    File::create(&mock_img).expect("Failed to create mock qemu-img");

    let found = find_qemu_img(Some(temp.path()));
    assert_eq!(found, Some(mock_img));
}

#[test]
fn test_create_disk_image_nonexistent_binary() {
    let temp = tempfile::tempdir().expect("Failed to create tempdir");
    let bogus_img = temp.path().join("does_not_exist_qemu_img_binary");
    let target = temp.path().join("test.qcow2");

    let res = create_disk_image(&bogus_img, &target, DriveFormat::Qcow2, 10);
    assert!(res.is_err());
}

#[test]
fn test_create_disk_image_with_system_qemu_img_if_present() {
    if let Some(qemu_img) = find_qemu_img(None) {
        let temp = tempfile::tempdir().expect("Failed to create tempdir");
        let target = temp.path().join("vm_disk.qcow2");
        let res = create_disk_image(&qemu_img, &target, DriveFormat::Qcow2, 1);
        assert!(res.is_ok(), "create_disk_image failed: {:?}", res.err());
        assert!(target.exists(), "Created disk image file does not exist");
    }
}

#[test]
fn test_build_qemu_args_none_flags() {
    let mut vm = VmConfig::new_default("Headless Isolated VM", Arch::X86_64);
    vm.display.vga_type = "none".to_string();
    vm.network.backend = "none".to_string();

    let args = build_qemu_args(&vm);
    let joined = args.join(" ");

    assert!(
        joined.contains("-vga none"),
        "Args should contain '-vga none': {:?}",
        joined
    );
    assert!(
        joined.contains("-nic none"),
        "Args should contain '-nic none': {:?}",
        joined
    );
    assert!(
        !joined.contains("-netdev"),
        "Args should not configure -netdev when backend is none: {:?}",
        joined
    );
}

#[test]
fn test_find_qemu_binary_file_candidate_name_filtering() {
    let temp = tempfile::tempdir().expect("Failed to create tempdir");
    let valid_name = if cfg!(windows) {
        "qemu-system-x86_64.exe"
    } else {
        "qemu-system-x86_64"
    };
    let valid_file = temp.path().join(valid_name);
    File::create(&valid_file).expect("create valid file");

    let invalid_file = temp.path().join("not_qemu.txt");
    File::create(&invalid_file).expect("create invalid file");

    let found_valid = find_qemu_binary(Some(&valid_file), Arch::X86_64);
    assert_eq!(found_valid, Some(valid_file));

    let found_invalid = find_qemu_binary(Some(&invalid_file), Arch::X86_64);
    assert_ne!(found_invalid, Some(invalid_file.clone()));
}
