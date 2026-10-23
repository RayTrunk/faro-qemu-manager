// FARO QEMU MANAGER (C) R.Trunk 2026
// Does the installed QEMU actually accept the command lines we build?

mod common;

use common::{qemu_accepts, Acceptance};
use faro_qem::config::{Arch, DriveConfig, DriveFormat, DriveInterface, Firmware, VmConfig};
use faro_qem::qemu::{build_qemu_args, find_uefi_firmware, prepare_nvram};

#[test]
fn test_the_probe_can_tell_good_arguments_from_bad() {
    match qemu_accepts(Arch::X86_64, &["-machine".into(), "q35".into()]) {
        Acceptance::NoQemu => eprintln!("skipped: no qemu-system-x86_64 on this machine"),
        Acceptance::Accepted => {}
        Acceptance::Rejected { first_line } => panic!("q35 must be accepted: {}", first_line),
    }

    // A bus type QEMU does not have: this is one of the eight bugs.
    match qemu_accepts(
        Arch::X86_64,
        &[
            "-machine".into(),
            "q35".into(),
            "-drive".into(),
            "if=sata,file=nonexistent.qcow2".into(),
        ],
    ) {
        Acceptance::NoQemu => {}
        Acceptance::Rejected { first_line } => assert!(
            first_line.contains("sata"),
            "the rejection must name the offending option, got: {}",
            first_line
        ),
        Acceptance::Accepted => panic!("if=sata must be rejected — the probe has no teeth"),
    }
}

#[test]
fn test_qemu_accepts_every_drive_interface() {
    let dir = tempfile::tempdir().unwrap();
    for iface in DriveInterface::all() {
        let disk = dir.path().join(format!("{}.qcow2", iface.as_str()));
        common::make_empty_qcow2(&disk);

        let mut vm = VmConfig::new_default("probe", Arch::X86_64);
        vm.drives.push(DriveConfig {
            path: disk,
            format: DriveFormat::Qcow2,
            interface: *iface,
            boot_index: None,
        });

        common::assert_accepted(iface.as_str(), Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_two_isos_alongside_a_disk() {
    let dir = tempfile::tempdir().unwrap();
    let disk = dir.path().join("d.qcow2");
    common::make_empty_qcow2(&disk);
    let a = dir.path().join("a.iso");
    let b = dir.path().join("b.iso");
    std::fs::write(&a, b"").unwrap();
    std::fs::write(&b, b"").unwrap();

    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: None,
    });
    vm.cdroms.push(a);
    vm.cdroms.push(b);

    common::assert_accepted("two ISOs + disk", Arch::X86_64, &build_qemu_args(&vm));
}

#[test]
fn test_qemu_accepts_user_networking_with_two_port_forwards() {
    use faro_qem::config::PortForwardRule;

    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    for (host, guest) in [(2222u16, 22u16), (8080, 80)] {
        vm.network.port_forwards.push(PortForwardRule {
            host_port: host,
            guest_port: guest,
            proto: "tcp".to_string(),
        });
    }
    common::assert_accepted("user + 2 forwards", Arch::X86_64, &build_qemu_args(&vm));
}

// Review Focus 5
#[test]
fn test_two_uefi_vms_get_separate_nvram_files() {
    let dir = tempfile::tempdir().unwrap();
    let Some(uefi) = find_uefi_firmware(None) else {
        eprintln!("skipped: no edk2 firmware on this machine");
        return;
    };

    let mut built = Vec::new();
    for name in ["one", "two"] {
        let nvram = dir.path().join(name).join("nvram.fd");
        prepare_nvram(&uefi.vars_template, &nvram).expect("nvram copy");
        built.push(nvram);
    }
    assert_ne!(built[0], built[1]);
    assert!(built.iter().all(|p| p.is_file()));

    // A second call must not wipe the boot entries the guest wrote.
    std::fs::write(&built[0], b"GUEST BOOT ENTRIES").unwrap();
    prepare_nvram(&uefi.vars_template, &built[0]).unwrap();
    assert_eq!(
        std::fs::read(&built[0]).unwrap(),
        b"GUEST BOOT ENTRIES",
        "an existing NVRAM file is the guest's, not ours to replace"
    );
}

#[test]
fn test_qemu_accepts_a_uefi_machine() {
    let dir = tempfile::tempdir().unwrap();
    let Some(uefi) = find_uefi_firmware(None) else {
        eprintln!("skipped: no edk2 firmware on this machine");
        return;
    };

    let disk = dir.path().join("d.qcow2");
    common::make_empty_qcow2(&disk);
    let nvram = dir.path().join("nvram.fd");
    prepare_nvram(&uefi.vars_template, &nvram).unwrap();

    let mut vm = VmConfig::new_default("win11-probe", Arch::X86_64);
    vm.firmware = Firmware::Uefi { nvram };
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::Sata,
        boot_index: None,
    });

    common::assert_accepted("uefi + sata", Arch::X86_64, &build_qemu_args(&vm));
}
