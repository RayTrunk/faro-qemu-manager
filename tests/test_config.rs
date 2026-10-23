use faro_qem::config::{
    claim_vm_folder, default_vm_dir, load_settings, load_vms, save_settings, save_vms,
    try_load_vms, vm_folder_name, Accelerator, AppSettings, Arch, DisplayConfig, DriveConfig,
    DriveFormat, DriveInterface, Firmware, NetworkConfig, PortForwardRule, VmConfig,
};
use std::path::PathBuf;
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn test_vm_config_default_and_roundtrip() {
    let vm = VmConfig::new_default("Test VM", Arch::X86_64);
    assert_eq!(vm.name, "Test VM");
    assert_eq!(vm.cpu_cores, 2);
    assert_eq!(vm.memory_mb, 2048);
    assert_eq!(vm.arch, Arch::X86_64);

    let json = serde_json::to_string_pretty(&vm).expect("serialize");
    let deserialized: VmConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.id, vm.id);
    assert_eq!(deserialized.name, vm.name);
    assert_eq!(deserialized.cpu_cores, vm.cpu_cores);
    assert_eq!(deserialized.memory_mb, vm.memory_mb);
    assert_eq!(deserialized.arch, vm.arch);
}

#[test]
fn test_display_and_network_config_serialization() {
    let mut net = NetworkConfig::default();
    assert_eq!(net.backend, "user");
    assert_eq!(net.nic_model, "virtio-net-pci");

    net.port_forwards.push(PortForwardRule {
        host_port: 2222,
        guest_port: 22,
        proto: "tcp".to_string(),
    });

    let display = DisplayConfig {
        vga_type: "std".to_string(),
        display_backend: "gtk".to_string(),
        fullscreen: true,
    };

    let net_json = serde_json::to_string(&net).unwrap();
    let net_parsed: NetworkConfig = serde_json::from_str(&net_json).unwrap();
    assert_eq!(net_parsed, net);

    let disp_json = serde_json::to_string(&display).unwrap();
    let disp_parsed: DisplayConfig = serde_json::from_str(&disp_json).unwrap();
    assert_eq!(disp_parsed, display);
}

#[test]
fn test_drive_config_serialization() {
    let drive = DriveConfig {
        path: PathBuf::from("disk.qcow2"),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: Some(1),
    };
    let json = serde_json::to_string(&drive).unwrap();
    let parsed: DriveConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.format, DriveFormat::Qcow2);
    assert_eq!(parsed.interface, DriveInterface::VirtIO);
    assert_eq!(parsed.boot_index, Some(1));
    assert_eq!(parsed.path, PathBuf::from("disk.qcow2"));
}

#[test]
fn test_app_settings_default_and_serialization() {
    let settings = AppSettings::default();
    assert_eq!(settings.language, "en");
    assert_eq!(settings.theme, "dark");
    assert_eq!(settings.qemu_dir, None);
    assert_eq!(settings.default_arch, Arch::X86_64);

    let json = serde_json::to_string(&settings).expect("serialize settings");
    let deserialized: AppSettings = serde_json::from_str(&json).expect("deserialize settings");
    assert_eq!(deserialized, settings);
}

#[test]
fn test_save_and_load_vms_persistence() {
    let dir = tempdir().expect("create temp dir");
    let path = dir.path().join("vms.json");

    // Initially nonexistent file should return empty vec
    let initial_vms = load_vms(&path);
    assert!(initial_vms.is_empty());

    // Save list of VMs
    let mut vm1 = VmConfig::new_default("Debian 12", Arch::X86_64);
    vm1.cpu_cores = 4;
    vm1.memory_mb = 4096;
    vm1.drives.push(DriveConfig {
        path: PathBuf::from("debian.qcow2"),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: Some(0),
    });

    let vm2 = VmConfig::new_default("ARM Alpine", Arch::Aarch64);

    let vms = vec![vm1.clone(), vm2.clone()];
    save_vms(&path, &vms).expect("save vms successfully");

    // Load back
    let loaded = load_vms(&path);
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].id, vm1.id);
    assert_eq!(loaded[0].name, "Debian 12");
    assert_eq!(loaded[0].cpu_cores, 4);
    assert_eq!(loaded[0].memory_mb, 4096);
    assert_eq!(loaded[0].drives.len(), 1);
    assert_eq!(loaded[1].id, vm2.id);
    assert_eq!(loaded[1].name, "ARM Alpine");
    assert_eq!(loaded[1].arch, Arch::Aarch64);
}

#[test]
fn test_save_and_load_settings_persistence() {
    let dir = tempdir().expect("create temp dir");
    let path = dir.path().join("settings.json");

    // Nonexistent file returns default settings
    let default_loaded = load_settings(&path);
    assert_eq!(default_loaded.language, "en");
    assert_eq!(default_loaded.theme, "dark");

    let custom_settings = AppSettings {
        qemu_dir: Some(PathBuf::from(r"C:\Program Files\qemu")),
        language: "de".to_string(),
        theme: "light".to_string(),
        default_arch: Arch::Aarch64,
        vm_dir: Some(PathBuf::from(r"D:\FARO-VMs")),
    };

    save_settings(&path, &custom_settings).expect("save settings successfully");

    let loaded = load_settings(&path);
    assert_eq!(loaded, custom_settings);
}

#[test]
fn test_arch_and_accelerator_variants() {
    let arches = [
        Arch::X86_64,
        Arch::I386,
        Arch::Aarch64,
        Arch::Riscv64,
        Arch::Arm,
    ];
    for arch in &arches {
        let json = serde_json::to_string(arch).unwrap();
        let parsed: Arch = serde_json::from_str(&json).unwrap();
        assert_eq!(*arch, parsed);
    }

    let accels = [
        Accelerator::Auto,
        Accelerator::WHPX,
        Accelerator::HAXM,
        Accelerator::KVM,
        Accelerator::TCG,
    ];
    for accel in &accels {
        let json = serde_json::to_string(accel).unwrap();
        let parsed: Accelerator = serde_json::from_str(&json).unwrap();
        assert_eq!(*accel, parsed);
    }
}

// --------------------------------------------------------------------------
// Part 2: firmware as its own concept
// --------------------------------------------------------------------------

#[test]
fn test_old_bios_uefi_field_still_loads() {
    let json = r#"[{"id":"550e8400-e29b-41d4-a716-446655440000","name":"Old","arch":"x86_64",
                    "cpu_cores":2,"memory_mb":2048,"bios_uefi":"/fw/OVMF.fd"}]"#;
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");
    std::fs::write(&path, json).unwrap();

    let vms = try_load_vms(&path).expect("an old file still loads");
    assert_eq!(
        vms[0].firmware,
        Firmware::Bios {
            rom: Some(PathBuf::from("/fw/OVMF.fd"))
        },
        "an old bios_uefi path means BIOS with that ROM"
    );
}

#[test]
fn test_firmware_defaults_to_plain_bios() {
    let vm = VmConfig::new_default("fresh", Arch::X86_64);
    assert_eq!(vm.firmware, Firmware::Bios { rom: None });
}

#[test]
fn test_firmware_round_trips() {
    let fw = Firmware::Uefi {
        nvram: PathBuf::from("vm/nvram.fd"),
    };
    let json = serde_json::to_string(&fw).unwrap();
    assert_eq!(serde_json::from_str::<Firmware>(&json).unwrap(), fw);
}

#[test]
fn test_a_uefi_vm_survives_a_save_and_load_round_trip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");

    let mut vm = VmConfig::new_default("Windows 11", Arch::X86_64);
    vm.firmware = Firmware::Uefi {
        nvram: PathBuf::from("win11/nvram.fd"),
    };
    save_vms(&path, &[vm.clone()]).unwrap();

    let loaded = try_load_vms(&path).unwrap();
    assert_eq!(loaded[0].firmware, vm.firmware);
}

// --------------------------------------------------------------------------
// Part 3a: a place for VM files, and one folder per VM
// --------------------------------------------------------------------------

#[test]
fn test_default_vm_dir_sits_in_the_user_profile() {
    let dir = default_vm_dir();
    assert!(dir.ends_with("FARO-VMs"), "got: {}", dir.display());
    assert!(
        dir.is_absolute(),
        "a relative default would land in the directory the app was launched from"
    );
}

#[test]
fn test_a_name_that_cannot_be_a_folder_still_yields_one() {
    const FORBIDDEN: [char; 9] = ['/', '\\', ':', '?', '*', '|', '"', '<', '>'];
    let id = Uuid::new_v4();
    for raw in [
        r"Mein VM: C:\test?",
        "",
        "...",
        "CON",
        "   ",
        r"a/b\c|d*e",
        r"..\..\escape",
    ] {
        let folder = vm_folder_name(raw, id);
        assert!(!folder.is_empty(), "{:?} yielded nothing", raw);
        assert!(
            !folder.contains(FORBIDDEN),
            "{:?} yielded {:?}",
            raw,
            folder
        );
        assert_ne!(folder.trim_matches('.'), "", "{:?} yielded only dots", raw);
        assert!(
            !matches!(
                folder.to_ascii_uppercase().as_str(),
                "CON" | "PRN" | "AUX" | "NUL"
            ),
            "{:?} yielded a name Windows reserves",
            raw
        );
    }
}

#[test]
fn test_two_vms_with_one_name_get_two_folders() {
    let dir = tempdir().unwrap();
    let a = claim_vm_folder(dir.path(), "Windows 11", Uuid::new_v4()).unwrap();
    let b = claim_vm_folder(dir.path(), "Windows 11", Uuid::new_v4()).unwrap();
    assert_ne!(a, b, "two QEMU processes on one qcow2 destroy it");
    assert!(a.is_dir() && b.is_dir());
}

#[test]
fn test_claiming_the_same_folder_twice_for_one_vm_returns_the_same_path() {
    let dir = tempdir().unwrap();
    let id = Uuid::new_v4();
    let first = claim_vm_folder(dir.path(), "Debian", id).unwrap();
    let again = claim_vm_folder(dir.path(), "Debian", id).unwrap();
    assert_eq!(
        first, again,
        "re-opening a VM must not create a second folder"
    );
}

#[test]
fn test_an_unusable_root_is_reported_not_panicked() {
    let dir = tempdir().unwrap();
    let blocker = dir.path().join("root");
    std::fs::write(&blocker, b"a file where the directory must go").unwrap();
    let err = claim_vm_folder(&blocker, "Alpine", Uuid::new_v4())
        .expect_err("an unusable VM directory must be reported");
    assert!(err.contains("root"), "name the path, got: {}", err);
}

#[test]
fn test_settings_without_a_vm_dir_fall_back_to_the_default() {
    let settings = AppSettings::default();
    assert_eq!(
        settings.vm_dir, None,
        "unset means 'use the default', not a frozen path"
    );
    assert_eq!(settings.vm_dir_or_default(), default_vm_dir());
}
