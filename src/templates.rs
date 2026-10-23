// FARO QEMU MANAGER (C) R.Trunk 2026
// The six OS templates.

use std::path::Path;

use crate::config::{
    Arch, DisplayConfig, DriveConfig, DriveFormat, DriveInterface, Firmware, VmConfig,
};
use crate::qemu::nvram_path_for;

/// A starting assignment for a new virtual machine.
///
/// A template is not a band: once the machine exists it is an ordinary
/// machine, every field free to change, and nothing here writes itself
/// through again. The key is kept on the machine only so later hints can
/// mention where it came from.
pub struct VmTemplate {
    /// Stable identifier, stored on the machine and used by the download
    /// catalogue. Never shown to the user.
    pub key: &'static str,
    /// Shown in the gallery. A product name, so it is not translated.
    pub name: &'static str,
    pub arch: Arch,
    /// UEFI with a per-VM variable store, as opposed to plain BIOS.
    pub uefi: bool,
    /// SATA for the Windows templates, so Setup sees the disk without
    /// loading a driver first. Linux has the virtio drivers in the kernel.
    pub interface: DriveInterface,
    pub cpu_cores: u32,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub nic_model: &'static str,
    /// Extra arguments the template starts with. Visible and editable, which
    /// is why the host-clock setting lives here rather than in a hidden flag.
    pub extra_args: &'static str,
    /// Catalogue key of a warning this template needs shown.
    pub notice_key: Option<&'static str>,
    /// Key into the download catalogue, where one exists. Microsoft hands out
    /// session-bound links, so the Windows templates have none.
    pub iso_source: Option<&'static str>,
}

/// Figures from the design document, section 5.5. The disk interface for the
/// Windows templates was the client's decision; the rest are proposals that
/// survived review.
static TEMPLATES: [VmTemplate; 6] = [
    VmTemplate {
        key: "windows-11",
        name: "Windows 11",
        arch: Arch::X86_64,
        uefi: true,
        interface: DriveInterface::Sata,
        cpu_cores: 4,
        memory_mb: 8192,
        disk_gb: 64,
        nic_model: "e1000e",
        extra_args: "-rtc base=localtime",
        notice_key: Some("template_notice_windows"),
        iso_source: None,
    },
    VmTemplate {
        key: "windows-10",
        name: "Windows 10",
        arch: Arch::X86_64,
        uefi: true,
        interface: DriveInterface::Sata,
        cpu_cores: 2,
        memory_mb: 4096,
        disk_gb: 64,
        nic_model: "e1000e",
        extra_args: "-rtc base=localtime",
        notice_key: Some("template_notice_windows"),
        iso_source: None,
    },
    VmTemplate {
        key: "debian",
        name: "Debian",
        arch: Arch::X86_64,
        uefi: true,
        interface: DriveInterface::VirtIO,
        cpu_cores: 2,
        memory_mb: 4096,
        disk_gb: 32,
        nic_model: "virtio-net-pci",
        extra_args: "",
        notice_key: None,
        iso_source: Some("debian"),
    },
    VmTemplate {
        key: "ubuntu",
        name: "Ubuntu",
        arch: Arch::X86_64,
        uefi: true,
        interface: DriveInterface::VirtIO,
        cpu_cores: 4,
        memory_mb: 4096,
        disk_gb: 40,
        nic_model: "virtio-net-pci",
        extra_args: "",
        notice_key: None,
        iso_source: Some("ubuntu"),
    },
    VmTemplate {
        key: "arch",
        name: "Arch Linux",
        arch: Arch::X86_64,
        uefi: true,
        interface: DriveInterface::VirtIO,
        cpu_cores: 2,
        memory_mb: 4096,
        disk_gb: 32,
        nic_model: "virtio-net-pci",
        extra_args: "",
        notice_key: None,
        iso_source: Some("arch"),
    },
    VmTemplate {
        key: "alpine",
        name: "Alpine Linux",
        arch: Arch::X86_64,
        uefi: false,
        interface: DriveInterface::VirtIO,
        cpu_cores: 1,
        memory_mb: 1024,
        disk_gb: 8,
        nic_model: "virtio-net-pci",
        extra_args: "",
        notice_key: None,
        iso_source: Some("alpine"),
    },
];

impl VmTemplate {
    /// Every template, in the order the gallery shows them.
    pub fn all() -> &'static [VmTemplate] {
        &TEMPLATES
    }

    /// The template with this key, if there is one.
    pub fn by_key(key: &str) -> Option<&'static VmTemplate> {
        TEMPLATES.iter().find(|t| t.key == key)
    }

    /// Build the configuration this template stands for.
    ///
    /// Creates nothing: the caller writes the disk and the variable store,
    /// where a failure can still undo the whole creation. The variable store
    /// is placed after the disk on purpose, because its path is derived from
    /// the disk's folder.
    ///
    /// No boot index is set. With UEFI and an empty disk the firmware boots
    /// the CD; once the guest is installed its own NVRAM entry takes over,
    /// which needs neither an ejected ISO nor a record of "install finished".
    pub fn instantiate(&self, name: &str, folder: &Path) -> VmConfig {
        let mut vm = VmConfig::new_default(name, self.arch);
        vm.machine_type = self.arch.default_machine().to_string();
        vm.cpu_cores = self.cpu_cores;
        vm.memory_mb = self.memory_mb;
        vm.display = DisplayConfig {
            vga_type: "virtio-vga".to_string(),
            display_backend: "default".to_string(),
            fullscreen: false,
        };
        vm.network.nic_model = self.nic_model.to_string();
        vm.extra_args = self.extra_args.to_string();
        vm.template = Some(self.key.to_string());

        vm.drives.push(DriveConfig {
            path: folder.join("disk.qcow2"),
            format: DriveFormat::Qcow2,
            interface: self.interface,
            boot_index: None,
        });

        if self.uefi {
            vm.firmware = Firmware::Uefi {
                nvram: nvram_path_for(&vm),
            };
        }
        vm
    }
}
