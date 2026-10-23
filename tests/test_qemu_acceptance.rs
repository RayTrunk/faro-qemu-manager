// FARO QEMU MANAGER (C) R.Trunk 2026
// Does the installed QEMU actually accept the command lines we build?

mod common;

use common::{qemu_accepts, Acceptance};
use faro_qem::config::{Arch, DriveConfig, DriveFormat, DriveInterface, VmConfig};
use faro_qem::qemu::build_qemu_args;

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
