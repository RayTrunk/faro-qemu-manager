use faro_qem::config::{
    Accelerator, Arch, DisplayConfig, DriveConfig, DriveFormat, DriveInterface, Firmware,
    NetworkConfig, PortForwardRule, VmConfig,
};
use faro_qem::qemu::{
    build_cdrom_args, build_drive_args, build_firmware_args, build_network_args, build_qemu_args,
    create_disk_image, find_qemu_binary, find_qemu_img, IdeLayout, UefiFirmware,
};

/// The drive arguments for `drives` on the machine the UI offers for x86.
fn drive_args(drives: &[DriveConfig]) -> Vec<String> {
    build_drive_args(drives, "q35").0
}
use std::fs::File;
use std::path::PathBuf;

#[test]
fn test_build_qemu_args_basic() {
    let mut vm = VmConfig::new_default("Test Linux", Arch::X86_64);
    vm.cpu_cores = 4;
    vm.memory_mb = 4096;
    vm.accel = Accelerator::TCG;
    vm.extra_args = "-usb -device usb-tablet".to_string();

    let args = build_qemu_args(&vm, None).expect("a BIOS machine needs no firmware");
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

    let args = build_qemu_args(&vm, None).expect("a BIOS machine needs no firmware");
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
    vm.firmware = Firmware::Bios {
        rom: Some(PathBuf::from("/path/to/bios.bin")),
    };
    vm.cdroms.push(PathBuf::from("/path/to/install.iso"));
    vm.display = DisplayConfig {
        vga_type: "virtio-vga".to_string(),
        display_backend: "default".to_string(),
        fullscreen: true,
    };

    let args = build_qemu_args(&vm, None).expect("a BIOS machine needs no firmware");
    let joined = args.join(" ");
    assert!(joined.contains("-machine virt,accel=tcg"));
    assert!(joined.contains("-cpu cortex-a72"));
    assert!(joined.contains("-smp 8"));
    assert!(joined.contains("-m 8192M"));
    assert!(joined.contains("-bios /path/to/bios.bin"));
    // An ISO is its own CD-ROM drive now: -cdrom cannot be given twice, so it
    // cannot be the construction when more than one ISO is allowed.
    assert!(joined.contains("media=cdrom,file=/path/to/install.iso"));
    // This machine is aarch64, where there is no VGA adapter at all: the
    // assertion used to read "-vga virtio", which QEMU rejects outright.
    assert!(joined.contains("-device virtio-gpu-pci"), "got: {}", joined);
    assert!(!joined.contains("-vga"));
    assert!(joined.contains("-display default"));
    assert!(joined.contains("-full-screen"));
}

#[test]
fn test_build_qemu_args_accel_auto() {
    let vm = VmConfig::new_default("Auto Accel VM", Arch::X86_64);
    let args = build_qemu_args(&vm, None).expect("a BIOS machine needs no firmware");
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

    let args = build_qemu_args(&vm, None).expect("a BIOS machine needs no firmware");
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

#[test]
fn test_build_qemu_args_mouse_tablet_and_show_cursor() {
    let vm = VmConfig::new_default("Debian Desktop", Arch::X86_64);
    let args = build_qemu_args(&vm, None).expect("a BIOS machine needs no firmware");
    let joined = args.join(" ");

    assert!(
        joined.contains("-usb -device usb-tablet"),
        "Must configure usb-tablet for absolute pointer"
    );
    assert!(
        joined.contains("-display default,show-cursor=on"),
        "Must enable show-cursor for desktop GUI"
    );
}

// --------------------------------------------------------------------------
// Part 2: drives are attached through real devices, not invented bus types
// --------------------------------------------------------------------------

fn drive(path: &str, iface: DriveInterface) -> DriveConfig {
    DriveConfig {
        path: PathBuf::from(path),
        format: DriveFormat::Qcow2,
        interface: iface,
        boot_index: None,
    }
}

#[test]
fn test_sata_drive_uses_an_ahci_controller() {
    let args = drive_args(&[drive("d.qcow2", DriveInterface::Sata)]).join(" ");
    assert!(args.contains("-device ahci,id=faro-ahci"), "got: {}", args);
    assert!(args.contains("if=none,id=faro-drive-0"));
    assert!(args.contains("-device ide-hd,drive=faro-drive-0,bus=faro-ahci.0"));
    assert!(
        !args.contains("if=sata"),
        "if=sata is not a bus type QEMU has"
    );
}

#[test]
fn test_nvme_drive_gets_the_serial_qemu_demands() {
    let args = drive_args(&[drive("d.qcow2", DriveInterface::Nvme)]).join(" ");
    assert!(
        args.contains("-device nvme,drive=faro-drive-0,serial="),
        "got: {}",
        args
    );
    assert!(!args.contains("if=nvme"));
}

#[test]
fn test_scsi_drive_uses_a_virtio_scsi_controller() {
    let args = drive_args(&[drive("d.qcow2", DriveInterface::Scsi)]).join(" ");
    assert!(
        args.contains("-device virtio-scsi-pci,id=faro-scsi"),
        "got: {}",
        args
    );
    assert!(args.contains("-device scsi-hd,drive=faro-drive-0,bus=faro-scsi.0"));
    assert!(!args.contains("if=scsi"), "if=scsi is rejected on q35");
}

#[test]
fn test_virtio_uses_a_block_device_and_plain_ide_keeps_the_short_form() {
    let args = drive_args(&[
        drive("a.qcow2", DriveInterface::VirtIO),
        drive("b.qcow2", DriveInterface::Ide),
    ])
    .join(" ");
    // if=virtio cannot carry a boot index, so virtio always goes through the
    // device form; the hardware the guest sees is the same virtio-blk-pci.
    assert!(
        args.contains("-device virtio-blk-pci,drive=faro-drive-0"),
        "got: {}",
        args
    );
    assert!(!args.contains("if=virtio"));
    // An IDE disk without a boot index keeps the form QEMU allocates itself,
    // which is the only one that works on every machine type.
    assert!(args.contains("if=ide"));
    assert!(
        !args.contains("faro-ahci"),
        "no controller is needed for these"
    );
}

#[test]
fn test_a_boot_index_on_an_ide_disk_moves_the_group_onto_explicit_devices() {
    let mut first = drive("a.qcow2", DriveInterface::Ide);
    first.boot_index = Some(0);
    let drives = [first, drive("b.qcow2", DriveInterface::Ide)];

    // q35's controller is AHCI: one unit per bus.
    let args = build_drive_args(&drives, "q35").0.join(" ");
    assert!(
        args.contains("-device ide-hd,drive=faro-drive-0,bus=ide.0,unit=0,bootindex=0"),
        "got: {}",
        args
    );
    assert!(args.contains("-device ide-hd,drive=faro-drive-1,bus=ide.1,unit=0"));
    assert!(
        !args.contains("if=ide"),
        "mixing the forms makes the two fight over the same slot"
    );

    // The i440fx machines keep the PIIX pair: two units per bus.
    let args = build_drive_args(&drives, "pc").0.join(" ");
    assert!(
        args.contains("-device ide-hd,drive=faro-drive-1,bus=ide.0,unit=1"),
        "got: {}",
        args
    );
}

#[test]
fn test_cdroms_follow_the_disks_onto_explicit_devices() {
    let mut booting = drive("a.qcow2", DriveInterface::Ide);
    booting.boot_index = Some(0);
    let (_, mut ide) = build_drive_args(&[booting], "q35");
    assert!(ide.is_explicit());

    let args = build_cdrom_args(&[PathBuf::from("x.iso")], &mut ide).join(" ");
    assert!(
        args.contains("-device ide-cd,drive=faro-cdrom-0,bus=ide.1,unit=0"),
        "a CD-ROM left on the short form would claim a slot the disk holds, got: {}",
        args
    );
}

// Review Focus 1
#[test]
fn test_three_sata_drives_share_one_controller_with_distinct_buses() {
    let args = drive_args(&[
        drive("a.qcow2", DriveInterface::Sata),
        drive("b.qcow2", DriveInterface::Sata),
        drive("c.qcow2", DriveInterface::Sata),
    ])
    .join(" ");
    assert_eq!(args.matches("-device ahci,id=faro-ahci").count(), 1);
    for bus in ["faro-ahci.0", "faro-ahci.1", "faro-ahci.2"] {
        assert!(args.contains(bus), "missing {} in {}", bus, args);
    }
}

// Review Focus 2
#[test]
fn test_all_five_interfaces_in_one_vm_do_not_collide() {
    let args = drive_args(&[
        drive("a.qcow2", DriveInterface::VirtIO),
        drive("b.qcow2", DriveInterface::Sata),
        drive("c.qcow2", DriveInterface::Nvme),
        drive("d.qcow2", DriveInterface::Scsi),
        drive("e.qcow2", DriveInterface::Ide),
    ]);
    let joined = args.join(" ");
    assert_eq!(joined.matches("-device ahci,").count(), 1);
    assert_eq!(joined.matches("-device virtio-scsi-pci,").count(), 1);

    let ids: Vec<&String> = args
        .iter()
        .filter(|a| a.contains("id=faro-drive-"))
        .collect();
    let unique: std::collections::HashSet<&&String> = ids.iter().collect();
    assert_eq!(
        ids.len(),
        unique.len(),
        "drive ids must be unique: {:?}",
        ids
    );
}

// Review Focus 3
#[test]
fn test_a_path_with_a_comma_does_not_split_the_drive_option() {
    let args = drive_args(&[drive(r"C:\My VMs\disk,1.qcow2", DriveInterface::VirtIO)]);
    let spec = args
        .iter()
        .find(|a| a.contains("disk"))
        .expect("the drive spec is there");
    assert!(
        spec.contains("disk,,1.qcow2"),
        "a comma in a path must be escaped by doubling it, got: {}",
        spec
    );
}

// --------------------------------------------------------------------------
// Part 2: more than one ISO, and port forwards only where they work
// --------------------------------------------------------------------------

#[test]
fn test_two_isos_become_two_cdrom_drives() {
    let args = build_cdrom_args(
        &[PathBuf::from("a.iso"), PathBuf::from("b.iso")],
        &mut IdeLayout::new("q35"),
    )
    .join(" ");
    assert_eq!(args.matches("media=cdrom").count(), 2, "got: {}", args);
    assert!(
        !args.contains("-cdrom"),
        "two -cdrom flags make QEMU refuse to start"
    );
    // A fixed index collides with IDE disks, which occupy the same slots.
    assert!(
        !args.contains("index="),
        "QEMU hands out the free slot itself, got: {}",
        args
    );
}

#[test]
fn test_empty_iso_paths_are_skipped() {
    assert!(build_cdrom_args(&[PathBuf::new()], &mut IdeLayout::new("q35")).is_empty());
}

#[test]
fn test_port_forwards_only_reach_the_user_backend() {
    let mut net = NetworkConfig::default();
    net.port_forwards.push(PortForwardRule {
        host_port: 2222,
        guest_port: 22,
        proto: "tcp".to_string(),
    });

    let as_user = build_network_args(&net).join(" ");
    assert!(as_user.contains("hostfwd=tcp::2222-:22"));

    net.backend = "tap".to_string();
    let as_tap = build_network_args(&net).join(" ");
    assert!(
        !as_tap.contains("hostfwd"),
        "hostfwd is a user-backend option; QEMU refuses it on tap, got: {}",
        as_tap
    );
    assert!(as_tap.contains("-netdev tap,id=net0"));
}

#[test]
fn test_backend_none_disables_networking() {
    let net = NetworkConfig {
        backend: "none".to_string(),
        ..Default::default()
    };
    assert_eq!(build_network_args(&net), vec!["-nic", "none"]);
}

#[test]
fn test_uefi_emits_the_pflash_pair_with_a_per_vm_nvram() {
    let uefi = UefiFirmware {
        code: PathBuf::from("share/edk2-x86_64-code.fd"),
        vars_template: PathBuf::from("share/edk2-i386-vars.fd"),
    };
    let fw = Firmware::Uefi {
        nvram: PathBuf::from("vms/win11/nvram.fd"),
    };
    let args = build_firmware_args(&fw, Some(&uefi)).join(" ");

    assert!(
        args.contains("if=pflash,format=raw,unit=0,readonly=on"),
        "got: {}",
        args
    );
    assert!(args.contains("edk2-x86_64-code.fd"));
    assert!(args.contains("if=pflash,format=raw,unit=1"));
    assert!(args.contains("nvram.fd"));
    assert!(
        !args.contains("-bios"),
        "-bios loses the NVRAM, which is why Windows will not boot after install"
    );
    assert!(
        !args.contains("secure-code"),
        "we do not claim Secure Boot we cannot enforce"
    );
}

#[test]
fn test_bios_without_a_rom_adds_nothing() {
    assert!(build_firmware_args(&Firmware::Bios { rom: None }, None).is_empty());
}

#[test]
fn test_bios_with_a_rom_uses_minus_bios() {
    let args = build_firmware_args(
        &Firmware::Bios {
            rom: Some(PathBuf::from("my.rom")),
        },
        None,
    );
    assert_eq!(args, vec!["-bios", "my.rom"]);
}

// Review Focus 4
#[test]
fn test_uefi_without_discovered_firmware_emits_nothing_rather_than_falling_back() {
    let fw = Firmware::Uefi {
        nvram: PathBuf::from("n.fd"),
    };
    assert!(
        build_firmware_args(&fw, None).is_empty(),
        "a UEFI VM must not quietly start as BIOS; the start check reports the missing firmware"
    );
}

#[test]
fn test_a_uefi_machine_without_firmware_is_refused_not_built_as_bios() {
    let mut vm = VmConfig::new_default("Windows 11", Arch::X86_64);
    vm.firmware = Firmware::Uefi {
        nvram: PathBuf::from("nvram.fd"),
    };
    let err = build_qemu_args(&vm, None)
        .expect_err("a quiet BIOS fallback installs a guest that can never boot again");
    assert!(err.contains("UEFI"), "got: {}", err);
    assert!(err.contains("Windows 11"), "name the machine, got: {}", err);
}

// Important 8
#[test]
fn test_graphics_outside_x86_are_a_device_not_a_vga_type() {
    let mut vm = VmConfig::new_default("Alpine ARM", Arch::Aarch64);
    vm.display.vga_type = "virtio-vga".to_string();
    let joined = build_qemu_args(&vm, None).unwrap().join(" ");
    assert!(
        !joined.contains("-vga virtio"),
        "QEMU answers: Virtio VGA not available. Got: {}",
        joined
    );
    assert!(
        joined.contains("-device virtio-gpu-pci"),
        "virtio graphics outside x86 is a plain PCI device, got: {}",
        joined
    );

    // x86 keeps the short form it has always used.
    let mut vm = VmConfig::new_default("Debian", Arch::X86_64);
    vm.display.vga_type = "virtio-vga".to_string();
    let joined = build_qemu_args(&vm, None).unwrap().join(" ");
    assert!(joined.contains("-vga virtio"), "got: {}", joined);
}

// Minor 14
#[test]
fn test_a_help_flag_the_binary_rejects_yields_no_capabilities() {
    // This test binary is not QEMU: it rejects every -X help flag and exits
    // non-zero. Reading its complaints as option names would make the list
    // non-empty, and a non-empty list counts as authoritative.
    let not_qemu = std::env::current_exe().expect("the test binary has a path");
    let caps = faro_qem::qemu::probe_capabilities(&not_qemu);
    assert!(
        !caps.was_probed(),
        "error text is not a list of options: {:?}",
        caps
    );
}
