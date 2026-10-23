// FARO QEMU MANAGER (C) R.Trunk 2026
// Does the installed QEMU actually accept the command lines we build?

mod common;

/// Memory for a probe machine.
///
/// The probes run in parallel and each holds its allocation for two seconds,
/// so the default 2048 MB exhausts the host and QEMU answers "cannot set up
/// guest memory" — a resource failure that reads exactly like a rejection.
/// Acceptance is about the arguments, not the sizing.
const PROBE_MEMORY_MB: u32 = 128;

use common::{qemu_accepts, Acceptance};
use faro_qem::config::{Arch, DriveConfig, DriveFormat, DriveInterface, Firmware, VmConfig};
use faro_qem::qemu::{
    build_qemu_args, find_qemu_binary, find_uefi_firmware, prepare_nvram, probe_capabilities,
};
use faro_qem::ui::{offered_machine_types, offered_netdev_backends};

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
        vm.memory_mb = PROBE_MEMORY_MB;
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
    vm.memory_mb = PROBE_MEMORY_MB;
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
    vm.memory_mb = PROBE_MEMORY_MB;
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
    vm.memory_mb = PROBE_MEMORY_MB;
    vm.firmware = Firmware::Uefi { nvram };
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::Sata,
        boot_index: None,
    });

    common::assert_accepted("uefi + sata", Arch::X86_64, &build_qemu_args(&vm));
}

// --------------------------------------------------------------------------
// The whole selectable surface, one dimension per test
// --------------------------------------------------------------------------

/// A baseline machine with one working disk; the caller varies one field.
fn probe_vm(dir: &std::path::Path, tweak: impl FnOnce(&mut VmConfig)) -> VmConfig {
    let disk = dir.join("probe.qcow2");
    common::make_empty_qcow2(&disk);
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    vm.memory_mb = PROBE_MEMORY_MB;
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: None,
    });
    tweak(&mut vm);
    vm
}

#[test]
fn test_qemu_accepts_every_vga_type() {
    let dir = tempfile::tempdir().unwrap();
    for vga in ["virtio-vga", "std", "qxl", "cirrus", "none"] {
        let vm = probe_vm(dir.path(), |vm| vm.display.vga_type = vga.to_string());
        common::assert_accepted(vga, Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_every_nic_model() {
    let dir = tempfile::tempdir().unwrap();
    for nic in ["virtio-net-pci", "e1000", "rtl8139", "e1000e", "vmxnet3"] {
        let vm = probe_vm(dir.path(), |vm| vm.network.nic_model = nic.to_string());
        common::assert_accepted(nic, Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_every_offered_network_backend() {
    let dir = tempfile::tempdir().unwrap();
    let Some(bin) = find_qemu_binary(None, Arch::X86_64) else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let caps = probe_capabilities(&bin);

    for backend in offered_netdev_backends(Some(&caps)) {
        let vm = probe_vm(dir.path(), |vm| vm.network.backend = backend.to_string());
        common::assert_accepted(backend, Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_every_drive_format() {
    let dir = tempfile::tempdir().unwrap();
    for format in DriveFormat::all() {
        let disk = dir.path().join(format!("f-{}.img", format.as_str()));
        common::make_disk_image(&disk, *format);

        let mut vm = VmConfig::new_default("probe", Arch::X86_64);
        vm.memory_mb = PROBE_MEMORY_MB;
        vm.drives.push(DriveConfig {
            path: disk,
            format: *format,
            interface: DriveInterface::VirtIO,
            boot_index: None,
        });
        common::assert_accepted(format.as_str(), Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_every_offered_machine_type() {
    let dir = tempfile::tempdir().unwrap();
    let Some(bin) = find_qemu_binary(None, Arch::X86_64) else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let caps = probe_capabilities(&bin);

    for machine in offered_machine_types(Arch::X86_64, Some(&caps)) {
        let vm = probe_vm(dir.path(), |vm| vm.machine_type = machine.to_string());
        common::assert_accepted(machine, Arch::X86_64, &build_qemu_args(&vm));
    }
}

/// Display backends cannot be established by launching QEMU: the acceptance
/// probe passes `-display none` itself and the last one wins, so every value
/// would look accepted. QEMU's own list is the honest source.
#[test]
fn test_every_offered_display_backend_is_one_qemu_knows() {
    let Some(bin) = find_qemu_binary(None, Arch::X86_64) else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let caps = probe_capabilities(&bin);
    assert!(caps.was_probed());

    for backend in ["gtk", "sdl", "none"] {
        assert!(
            caps.has_display(backend),
            "the UI offers {} but QEMU does not list it: {:?}",
            backend,
            caps.displays
        );
    }
    // "default" is accepted by QEMU but absent from -display help, so it is
    // checked by launching instead — the probe's own -display none cannot mask
    // a value QEMU would have rejected outright at parse time.
    let dir = tempfile::tempdir().unwrap();
    let vm = probe_vm(dir.path(), |vm| {
        vm.display.display_backend = "default".to_string()
    });
    common::assert_accepted("display=default", Arch::X86_64, &build_qemu_args(&vm));
}

// --------------------------------------------------------------------------
// Combinations, not single dimensions
//
// One disk of each kind proves almost nothing: the index and bus arithmetic
// only goes wrong from the second disk on, and a boot index only lands in the
// wrong namespace when there is a format beside it. These tables probe the
// combinations a person reaches with ordinary clicks.
// --------------------------------------------------------------------------

/// A machine with `count` disks of one interface, boot-indexed or not.
fn vm_with_disks(
    dir: &std::path::Path,
    iface: DriveInterface,
    count: u32,
    boot_indexed: bool,
) -> VmConfig {
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    vm.memory_mb = PROBE_MEMORY_MB;
    for n in 0..count {
        let disk = dir.join(format!("{}-{}.qcow2", iface.as_str(), n));
        common::make_empty_qcow2(&disk);
        vm.drives.push(DriveConfig {
            path: disk,
            format: DriveFormat::Qcow2,
            interface: iface,
            boot_index: boot_indexed.then_some(n),
        });
    }
    vm
}

#[test]
fn test_qemu_accepts_one_two_and_three_disks_of_every_interface() {
    let dir = tempfile::tempdir().unwrap();
    for iface in DriveInterface::all() {
        for count in 1..=3 {
            let vm = vm_with_disks(dir.path(), *iface, count, false);
            common::assert_accepted(
                &format!("{} x{}", iface.as_str(), count),
                Arch::X86_64,
                &build_qemu_args(&vm),
            );
        }
    }
}

#[test]
fn test_qemu_accepts_a_boot_index_on_every_interface() {
    let dir = tempfile::tempdir().unwrap();
    for iface in DriveInterface::all() {
        for count in 1..=2 {
            let vm = vm_with_disks(dir.path(), *iface, count, true);
            common::assert_accepted(
                &format!("{} x{} boot-indexed", iface.as_str(), count),
                Arch::X86_64,
                &build_qemu_args(&vm),
            );
        }
    }
}

#[test]
fn test_qemu_accepts_ide_disks_alongside_isos() {
    let dir = tempfile::tempdir().unwrap();
    for disks in 1..=3 {
        for isos in 0..=2 {
            let mut vm = vm_with_disks(dir.path(), DriveInterface::Ide, disks, false);
            for n in 0..isos {
                let iso = dir.path().join(format!("iso-{}.iso", n));
                std::fs::write(&iso, b"").unwrap();
                vm.cdroms.push(iso);
            }
            common::assert_accepted(
                &format!("{} IDE disks + {} ISOs", disks, isos),
                Arch::X86_64,
                &build_qemu_args(&vm),
            );
        }
    }
}

#[test]
fn test_qemu_accepts_a_path_with_a_space_and_a_comma() {
    let dir = tempfile::tempdir().unwrap();
    let odd = dir.path().join("My VMs, really");
    std::fs::create_dir_all(&odd).unwrap();

    let disk = odd.join("disk,1.qcow2");
    common::make_empty_qcow2(&disk);
    let iso = odd.join("setup,disc 1.iso");
    std::fs::write(&iso, b"").unwrap();

    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    vm.memory_mb = PROBE_MEMORY_MB;
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::Sata,
        boot_index: Some(0),
    });
    vm.cdroms.push(iso);

    common::assert_accepted("comma path", Arch::X86_64, &build_qemu_args(&vm));
}

/// The IDE bus arithmetic differs between the AHCI controller of q35 and the
/// PIIX pair of i440fx. Only q35 is offered by the UI, but a saved
/// configuration may still name the other, and a wrong mapping produces a VM
/// that cannot start — so QEMU is asked about both.
#[test]
fn test_qemu_accepts_boot_indexed_ide_disks_on_both_x86_machines() {
    let dir = tempfile::tempdir().unwrap();
    for machine in ["q35", "pc", "pc-i440fx-9.2"] {
        for disks in 1..=3 {
            let mut vm = vm_with_disks(dir.path(), DriveInterface::Ide, disks, true);
            vm.machine_type = machine.to_string();
            let iso = dir.path().join("boot.iso");
            std::fs::write(&iso, b"").unwrap();
            vm.cdroms.push(iso);
            common::assert_accepted(
                &format!("{}: {} boot-indexed IDE disks + ISO", machine, disks),
                Arch::X86_64,
                &build_qemu_args(&vm),
            );
        }
    }
}
