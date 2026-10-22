use faro_qem::config::{
    load_settings, load_vms, save_settings, save_vms, Accelerator, AppSettings, Arch,
    DisplayConfig, DriveConfig, DriveFormat, DriveInterface, NetworkConfig, PortForwardRule,
    VmConfig,
};
use std::path::PathBuf;
use tempfile::tempdir;

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
