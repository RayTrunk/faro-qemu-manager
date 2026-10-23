// FARO QEMU MANAGER (C) R.Trunk 2026
// What can the installed QEMU actually do?

use faro_qem::config::Arch;
use faro_qem::qemu::{find_qemu_binary, probe_capabilities, QemuCapabilities};

#[test]
fn test_capabilities_report_what_this_qemu_can_do() {
    let Some(bin) = find_qemu_binary(None, Arch::X86_64) else {
        eprintln!("skipped: no qemu-system-x86_64 on this machine");
        return;
    };
    let caps = probe_capabilities(&bin);

    assert!(caps.was_probed());
    assert!(caps.has_machine("q35"));
    assert!(caps.has_netdev("user"));
    assert!(caps.has_device("ahci"), "SATA needs the AHCI controller");
    assert!(caps.has_device("nvme"));
    assert!(
        caps.has_device("virtio-scsi-pci"),
        "SCSI needs this controller"
    );
    assert!(
        !caps.has_machine("pc-i440fx"),
        "the bare i440fx name is not a machine type; only versioned ones are"
    );
}

#[test]
fn test_unprobed_capabilities_claim_nothing_is_missing() {
    let caps = QemuCapabilities::default();
    assert!(!caps.was_probed());
    assert!(
        caps.has_device("ahci") && caps.has_netdev("bridge"),
        "without a probe we must not claim an option is unavailable"
    );
}
