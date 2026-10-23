// FARO QEMU MANAGER (C) R.Trunk 2026
// VM Detail Tabs and Hardware Controls component

// Which options are worth offering, given the architecture and what the
// installed QEMU reports. Pure functions, so the tests can read them directly
// instead of guessing from a closed combo box.

use crate::config::{Accelerator, Arch};
use crate::qemu::QemuCapabilities;

/// Graphics adapters worth offering for `arch`.
///
/// `-vga` and its named types are an x86 convenience. On every other
/// architecture QEMU has no VGA adapter and answers `Virtio VGA not
/// available`, so only the virtio device and no graphics at all are left.
pub fn offered_vga_types(arch: Arch) -> Vec<&'static str> {
    match arch {
        Arch::X86_64 | Arch::I386 => vec!["virtio-vga", "std", "qxl", "cirrus", "none"],
        _ => vec!["virtio-vga", "none"],
    }
}

/// Network backends worth offering for these capabilities.
///
/// `None` means QEMU was never asked, and then capability-dependent options
/// stay on offer: hiding one because we failed to ask would claim it is
/// unavailable when we do not know. `none` is ours rather than QEMU's, so it
/// is always in.
///
/// `tap` is left out whatever QEMU reports. It is listed by `-netdev help`,
/// but launching it answers "tap: no interface name" — it needs a host
/// interface the configuration has no field for. Offering a choice that cannot
/// work is the thing this function exists to prevent.
pub fn offered_netdev_backends(caps: Option<&QemuCapabilities>) -> Vec<&'static str> {
    ["user", "bridge"]
        .into_iter()
        .filter(|b| caps.is_none_or(|c| c.has_netdev(b)))
        .chain(std::iter::once("none"))
        .collect()
}

/// Machine types worth suggesting for this architecture.
///
/// The old fixed list offered `pc-i440fx`, which QEMU rejects outright — only
/// versioned names exist. Anything a probe does not confirm is dropped.
pub fn offered_machine_types(arch: Arch, caps: Option<&QemuCapabilities>) -> Vec<&'static str> {
    // microvm is deliberately absent: it has no PCI bus, so the default
    // virtio-vga, virtio-net-pci and usb-tablet all fail on it. It is a
    // specialist machine needing a different device set, not a one-click
    // suggestion.
    let candidates: &[&'static str] = match arch {
        Arch::X86_64 | Arch::I386 => &["q35"],
        Arch::Aarch64 | Arch::Arm => &["virt"],
        Arch::Riscv64 => &["virt", "sifive_u"],
    };
    candidates
        .iter()
        .copied()
        .filter(|m| caps.is_none_or(|c| c.has_machine(m)))
        .collect()
}

/// Accelerators worth offering on this platform.
///
/// HAXM is absent from modern Windows QEMU builds and used to be bent silently
/// onto WHPX. An option that does something other than it says is worse than
/// no option at all.
pub fn offered_accelerators() -> Vec<Accelerator> {
    Accelerator::all()
        .iter()
        .copied()
        .filter(|a| !(cfg!(target_os = "windows") && *a == Accelerator::HAXM))
        .collect()
}
