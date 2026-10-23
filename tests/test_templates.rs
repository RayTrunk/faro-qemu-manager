// FARO QEMU MANAGER (C) R.Trunk 2026
// The six templates: a starting assignment for a new VM, not a band.

mod common;

use tempfile::tempdir;

use faro_qem::config::{Arch, DriveInterface, Firmware, VmConfig};
use faro_qem::templates::VmTemplate;

#[test]
fn test_all_six_templates_are_there_with_the_decided_numbers() {
    let want = [
        // key, uefi, interface, cores, memory, disk_gb, nic
        (
            "windows-11",
            true,
            DriveInterface::Sata,
            4,
            8192,
            64,
            "e1000e",
        ),
        (
            "windows-10",
            true,
            DriveInterface::Sata,
            2,
            4096,
            64,
            "e1000e",
        ),
        (
            "debian",
            true,
            DriveInterface::VirtIO,
            2,
            4096,
            32,
            "virtio-net-pci",
        ),
        (
            "ubuntu",
            true,
            DriveInterface::VirtIO,
            4,
            4096,
            40,
            "virtio-net-pci",
        ),
        (
            "arch",
            true,
            DriveInterface::VirtIO,
            2,
            4096,
            32,
            "virtio-net-pci",
        ),
        (
            "alpine",
            false,
            DriveInterface::VirtIO,
            1,
            1024,
            8,
            "virtio-net-pci",
        ),
    ];
    assert_eq!(VmTemplate::all().len(), want.len());
    for (key, uefi, iface, cores, mem, disk, nic) in want {
        let t = VmTemplate::by_key(key).unwrap_or_else(|| panic!("missing {}", key));
        assert_eq!(
            (
                t.uefi,
                t.interface,
                t.cpu_cores,
                t.memory_mb,
                t.disk_gb,
                t.nic_model
            ),
            (uefi, iface, cores, mem, disk, nic),
            "{}",
            key
        );
        assert_eq!(t.arch, Arch::X86_64, "{}", key);
        assert!(!t.name.is_empty(), "{}", key);
    }
}

#[test]
fn test_windows_templates_keep_the_host_clock_and_warn_about_tpm() {
    for key in ["windows-11", "windows-10"] {
        let t = VmTemplate::by_key(key).unwrap();
        assert!(
            t.extra_args.contains("-rtc base=localtime"),
            "{} shows the wrong time without it",
            key
        );
        assert!(t.notice_key.is_some(), "{} needs the TPM hint", key);
        assert_eq!(
            t.iso_source, None,
            "Microsoft hands out session-bound links, so there is nothing to download"
        );
    }
}

#[test]
fn test_the_four_linux_templates_name_a_download_source() {
    for key in ["debian", "ubuntu", "arch", "alpine"] {
        assert!(
            VmTemplate::by_key(key).unwrap().iso_source.is_some(),
            "{}",
            key
        );
    }
}

#[test]
fn test_no_template_sets_a_boot_index() {
    // Spec 11.1: UEFI boots the CD while the disk is empty, and the guest's
    // own NVRAM entry wins afterwards. An explicit index would fight that.
    let dir = tempdir().unwrap();
    for t in VmTemplate::all() {
        let vm = t.instantiate("probe", dir.path());
        assert!(
            vm.drives.iter().all(|d| d.boot_index.is_none()),
            "{}",
            t.key
        );
    }
}

#[test]
fn test_instantiating_puts_disk_and_nvram_in_the_vm_folder() {
    let dir = tempdir().unwrap();
    let vm = VmTemplate::by_key("windows-11")
        .unwrap()
        .instantiate("Win 11", dir.path());
    assert_eq!(vm.name, "Win 11");
    assert_eq!(vm.template.as_deref(), Some("windows-11"));
    assert_eq!(vm.drives.len(), 1);
    assert!(
        vm.drives[0].path.starts_with(dir.path()),
        "got: {}",
        vm.drives[0].path.display()
    );
    match &vm.firmware {
        Firmware::Uefi { nvram } => assert!(nvram.starts_with(dir.path())),
        other => panic!("windows-11 is a UEFI template, got {:?}", other),
    }
    assert_eq!(vm.machine_type, "q35");
    assert_eq!(vm.network.nic_model, "e1000e");
}

#[test]
fn test_instantiating_creates_no_files() {
    // Creating files is the app's job, under an error path that can undo them.
    let dir = tempdir().unwrap();
    let vm = VmTemplate::by_key("debian")
        .unwrap()
        .instantiate("Debian", dir.path());
    assert!(!vm.drives[0].path.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn test_alpine_is_the_one_bios_template() {
    let dir = tempdir().unwrap();
    let vm = VmTemplate::by_key("alpine")
        .unwrap()
        .instantiate("Alpine", dir.path());
    assert_eq!(vm.firmware, Firmware::Bios { rom: None });
}

#[test]
fn test_a_template_is_a_starting_assignment_not_a_band() {
    let dir = tempdir().unwrap();
    let mut vm = VmTemplate::by_key("debian")
        .unwrap()
        .instantiate("Debian", dir.path());
    vm.memory_mb = 123;
    vm.firmware = Firmware::Bios { rom: None };
    let back: VmConfig = serde_json::from_str(&serde_json::to_string(&vm).unwrap()).unwrap();
    assert_eq!(back.memory_mb, 123);
    assert_eq!(back.firmware, Firmware::Bios { rom: None });
    assert_eq!(
        back.template.as_deref(),
        Some("debian"),
        "the key is descriptive, not binding"
    );
}

#[test]
fn test_an_unknown_key_is_not_a_template() {
    assert!(VmTemplate::by_key("gentoo").is_none());
    assert!(VmTemplate::by_key("").is_none());
}

#[test]
fn test_every_template_notice_is_in_all_three_catalogues() {
    // I18n::t falls back to English, so it cannot tell a missing German entry
    // from an English one standing in for it. The files are asked instead.
    for lang in ["en", "de", "fr"] {
        let keys = common::catalogue_keys(lang);
        for t in VmTemplate::all() {
            if let Some(key) = t.notice_key {
                assert!(keys.contains(key), "{} missing {} in {}", t.key, key, lang);
            }
        }
    }
}
