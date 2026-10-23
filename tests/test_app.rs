// FARO QEMU MANAGER (C) R.Trunk 2026
// Integration & End-to-End Application State Tests

mod common;

use egui::Context;
use std::path::Path;
use std::sync::mpsc;
use tempfile::tempdir;

use faro_qem::app::FaroQemApp;
use faro_qem::config::{
    claim_vm_folder, load_settings, load_vms, save_settings, vm_folder_name, Accelerator,
    AppSettings, Arch, CreationMode, DriveConfig, DriveFormat, DriveInterface, Firmware, VmConfig,
};
use faro_qem::qemu::{nvram_path_for, spawn_qemu, QemuEvent};
use faro_qem::templates::VmTemplate;
use faro_qem::ui::console::LogLevel;
use faro_qem::ui::sidebar::{SidebarAction, VmStatus};
use faro_qem::ui::vm_detail::{DetailTab, VmDetailAction};

#[test]
fn test_faro_qem_app_initialization_defaults() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let vms_path = dir.path().join("vms.json");

    let app = FaroQemApp::with_paths(settings_path.clone(), vms_path.clone(), None);

    // Default VM was initialized
    assert_eq!(app.vms.len(), 1);
    let vm = &app.vms[0];
    assert_eq!(vm.name, "Debian 12");
    assert_eq!(vm.arch, Arch::X86_64);
    assert_eq!(app.selected_vm_id, Some(vm.id));

    // Default settings were initialized
    assert_eq!(app.settings.theme, "dark");
    assert_eq!(app.settings.language, "en");
    assert_eq!(app.settings.default_arch, Arch::X86_64);

    // Initial state flags
    assert!(!app.settings_open);
    assert_eq!(app.active_tab, DetailTab::System);
    assert!(app.running_processes.is_empty());
    assert!(app.status_map.is_empty());

    // File persistence of initial default VM
    let loaded_vms = load_vms(&vms_path);
    assert_eq!(loaded_vms.len(), 1);
    assert_eq!(loaded_vms[0].id, vm.id);
}

#[test]
fn test_faro_qem_app_sidebar_actions() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let vms_path = dir.path().join("vms.json");

    let mut app = FaroQemApp::with_paths(settings_path, vms_path.clone(), None);
    let initial_vm_id = app.vms[0].id;

    // This test is about the direct way, which a fresh installation no longer
    // defaults to. The wizard way is covered by its own test.
    app.settings.creation_mode = CreationMode::Direct;

    // 1. New VM
    app.handle_sidebar_action(SidebarAction::NewVm, None);
    assert_eq!(app.vms.len(), 2);
    let second_vm_id = app.vms[1].id;
    assert_eq!(app.vms[1].name, "VM 2");
    assert_eq!(app.selected_vm_id, Some(second_vm_id));
    assert_eq!(load_vms(&vms_path).len(), 2);

    // 2. Duplicate VM
    app.handle_sidebar_action(SidebarAction::DuplicateVm(second_vm_id), None);
    assert_eq!(app.vms.len(), 3);
    let dup_vm_id = app.vms[2].id;
    assert_ne!(dup_vm_id, second_vm_id);
    assert_eq!(app.vms[2].name, "VM 2 (Copy)");
    assert_eq!(app.selected_vm_id, Some(dup_vm_id));
    assert_eq!(load_vms(&vms_path).len(), 3);

    // 3. Select VM
    app.handle_sidebar_action(SidebarAction::SelectVm(initial_vm_id), None);
    assert_eq!(app.selected_vm_id, Some(initial_vm_id));

    // 4. Delete VM
    app.handle_sidebar_action(SidebarAction::DeleteVm(dup_vm_id), None);
    assert_eq!(app.vms.len(), 2);
    assert!(app.vms.iter().all(|v| v.id != dup_vm_id));
    assert_eq!(load_vms(&vms_path).len(), 2);

    // 5. Open Settings
    assert!(!app.settings_open);
    app.handle_sidebar_action(SidebarAction::OpenSettings, None);
    assert!(app.settings_open);
}

#[test]
fn test_faro_qem_app_detail_actions_and_persistence() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let vms_path = dir.path().join("vms.json");

    let mut app = FaroQemApp::with_paths(settings_path, vms_path.clone(), None);
    let vm_id = app.vms[0].id;

    // Mutate VM configuration
    app.vms[0].name = "Arch Linux Production".to_string();
    app.vms[0].memory_mb = 8192;
    app.vms[0].cpu_cores = 8;
    app.vms[0].accel = Accelerator::TCG;

    // Save action
    app.handle_vm_detail_action(VmDetailAction::Save(vm_id), None);

    // Read back directly from disk
    let reloaded_vms = load_vms(&vms_path);
    assert_eq!(reloaded_vms.len(), 1);
    assert_eq!(reloaded_vms[0].name, "Arch Linux Production");
    assert_eq!(reloaded_vms[0].memory_mb, 8192);
    assert_eq!(reloaded_vms[0].cpu_cores, 8);
    assert_eq!(reloaded_vms[0].accel, Accelerator::TCG);
}

#[test]
fn test_faro_qem_app_process_qemu_events_lifecycle() {
    let dir = tempdir().unwrap();
    let mut app =
        FaroQemApp::with_paths(dir.path().join("s.json"), dir.path().join("v.json"), None);
    let vm_id = app.vms[0].id;

    // Simulate process registration with a dummy channel
    let (tx, rx) = mpsc::channel();
    // To construct a VmProcess for mock testing, run a fast system command (rustc --version)
    let rustc_bin = std::path::PathBuf::from("rustc");
    let (dummy_tx, _dummy_rx) = mpsc::channel();
    let vm_proc = spawn_qemu(&rustc_bin, &["--version".to_string()], dummy_tx)
        .expect("Failed to spawn rustc for process test fixture");

    app.running_processes.insert(vm_id, (vm_proc, rx));

    // 1. Started event
    tx.send(QemuEvent::Started(4242)).unwrap();
    app.process_qemu_events(None);

    assert_eq!(
        app.status_map.get(&vm_id),
        Some(&VmStatus::Running { pid: 4242 })
    );
    let logs = app.console_logs.get(&vm_id).unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs.entries[0].level, LogLevel::System);
    assert!(logs.entries[0].text.contains("4242"));

    // 2. Stdout and Stderr events
    tx.send(QemuEvent::Stdout("GRUB boot loader started".to_string()))
        .unwrap();
    tx.send(QemuEvent::Stderr("ACPI table warning".to_string()))
        .unwrap();
    app.process_qemu_events(None);

    let logs = app.console_logs.get(&vm_id).unwrap();
    assert_eq!(logs.len(), 3);
    assert_eq!(logs.entries[1].level, LogLevel::Stdout);
    assert_eq!(logs.entries[1].text, "GRUB boot loader started");
    assert_eq!(logs.entries[2].level, LogLevel::Stderr);
    assert_eq!(logs.entries[2].text, "ACPI table warning");

    // 3. Exited normally
    tx.send(QemuEvent::Exited(Some(0))).unwrap();
    app.process_qemu_events(None);

    assert_eq!(app.status_map.get(&vm_id), Some(&VmStatus::Stopped));
    let logs = app.console_logs.get(&vm_id).unwrap();
    assert_eq!(logs.len(), 4);
    assert_eq!(logs.entries[3].level, LogLevel::System);
    assert!(logs.entries[3].text.contains("normally"));

    // Process should be removed from running_processes
    assert!(!app.running_processes.contains_key(&vm_id));
}

#[test]
fn test_faro_qem_app_process_qemu_events_error_exit() {
    let dir = tempdir().unwrap();
    let mut app =
        FaroQemApp::with_paths(dir.path().join("s.json"), dir.path().join("v.json"), None);
    let vm_id = app.vms[0].id;

    let (tx, rx) = mpsc::channel();
    let rustc_bin = std::path::PathBuf::from("rustc");
    let (dummy_tx, _dummy_rx) = mpsc::channel();
    let vm_proc = spawn_qemu(&rustc_bin, &["--version".to_string()], dummy_tx).unwrap();

    app.running_processes.insert(vm_id, (vm_proc, rx));

    // Send error followed by non-zero exit code
    tx.send(QemuEvent::Error(
        "Hardware acceleration unavailable".to_string(),
    ))
    .unwrap();
    tx.send(QemuEvent::Exited(Some(1))).unwrap();
    app.process_qemu_events(None);

    assert!(matches!(
        app.status_map.get(&vm_id),
        Some(VmStatus::Error(_))
    ));
    let logs = app.console_logs.get(&vm_id).unwrap();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs.entries[0].level, LogLevel::Error);
    assert!(logs.entries[0]
        .text
        .contains("Hardware acceleration unavailable"));
    assert_eq!(logs.entries[1].level, LogLevel::Error);
    assert!(logs.entries[1].text.contains("non-zero exit code: 1"));
    assert!(!app.running_processes.contains_key(&vm_id));
}

#[test]
fn test_faro_qem_app_launch_binary_not_found() {
    let dir = tempdir().unwrap();
    let mut app =
        FaroQemApp::with_paths(dir.path().join("s.json"), dir.path().join("v.json"), None);
    let vm_id = app.vms[0].id;

    // Set custom QEMU directory to an empty temp dir where no binaries exist
    let empty_dir = dir.path().join("empty_qemu");
    std::fs::create_dir_all(&empty_dir).unwrap();
    app.settings.qemu_dir = Some(empty_dir);

    // Launch should fail gracefully if binary is not on system or path
    // For arch riscv64 or arm if not installed:
    app.vms[0].arch = Arch::Riscv64;
    let res = app.launch_vm(vm_id, None);
    if res.is_err() {
        assert!(matches!(
            app.status_map.get(&vm_id),
            Some(VmStatus::Error(_))
        ));
        let logs = app.console_logs.get(&vm_id).unwrap();
        assert!(!logs.is_empty());
        assert_eq!(logs.entries[0].level, LogLevel::Error);
    }
}

#[test]
fn test_faro_qem_app_stop_vm_cleans_up_process() {
    let dir = tempdir().unwrap();
    let mut app =
        FaroQemApp::with_paths(dir.path().join("s.json"), dir.path().join("v.json"), None);
    let vm_id = app.vms[0].id;

    let (_tx, rx) = mpsc::channel();
    let rustc_bin = std::path::PathBuf::from("rustc");
    let (dummy_tx, _dummy_rx) = mpsc::channel();
    let vm_proc = spawn_qemu(&rustc_bin, &["--version".to_string()], dummy_tx).unwrap();

    app.running_processes.insert(vm_id, (vm_proc, rx));
    app.status_map
        .insert(vm_id, VmStatus::Running { pid: 7777 });

    let res = app.stop_vm(vm_id);
    assert!(res.is_ok());
    assert_eq!(app.status_map.get(&vm_id), Some(&VmStatus::Stopped));
    assert!(!app.running_processes.contains_key(&vm_id));
}

#[test]
fn test_faro_qem_app_frame_rendering_all_tabs() {
    let dir = tempdir().unwrap();
    let mut app =
        FaroQemApp::with_paths(dir.path().join("s.json"), dir.path().join("v.json"), None);

    let ctx = Context::default();

    // Iterate through all tabs and render frames
    for tab in DetailTab::all() {
        app.active_tab = *tab;
        let _ = ctx.run(Default::default(), |ctx| {
            app.render_ui(ctx);
        });
    }

    // Verify search query filtering works during render
    app.search_query = "Debian".to_string();
    let _ = ctx.run(Default::default(), |ctx| {
        app.render_ui(ctx);
    });

    app.search_query = "Nonexistent".to_string();
    let _ = ctx.run(Default::default(), |ctx| {
        app.render_ui(ctx);
    });
}

#[test]
fn test_faro_qem_app_empty_state_rendering_and_recovery() {
    let dir = tempdir().unwrap();
    let mut app =
        FaroQemApp::with_paths(dir.path().join("s.json"), dir.path().join("v.json"), None);

    // Delete all VMs
    app.vms.clear();
    app.selected_vm_id = None;

    let ctx = Context::default();

    // Frame should render the empty-state greeting card without panic
    let _ = ctx.run(Default::default(), |ctx| {
        app.render_ui(ctx);
    });

    // Create a new VM to recover from empty state, the direct way: the
    // wizard would open a dialog instead and add nothing yet.
    app.settings.creation_mode = CreationMode::Direct;
    app.handle_sidebar_action(SidebarAction::NewVm, None);
    assert_eq!(app.vms.len(), 1);
    assert!(app.selected_vm_id.is_some());

    // Render frame again
    let _ = ctx.run(Default::default(), |ctx| {
        app.render_ui(ctx);
    });
}

#[test]
fn test_faro_qem_app_settings_dialog_save_and_reload() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let vms_path = dir.path().join("vms.json");

    let mut app = FaroQemApp::with_paths(settings_path.clone(), vms_path.clone(), None);

    // Open settings and modify them
    app.settings_open = true;
    app.settings.theme = "light".to_string();
    app.settings.language = "de".to_string();
    app.settings.default_arch = Arch::Aarch64;

    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        app.render_ui(ctx);
    });

    // Save and reload in fresh app instance
    let _ = faro_qem::config::save_settings(&settings_path, &app.settings);
    let reloaded_settings = load_settings(&settings_path);
    assert_eq!(reloaded_settings.theme, "light");
    assert_eq!(reloaded_settings.language, "de");
    assert_eq!(reloaded_settings.default_arch, Arch::Aarch64);

    let fresh_app = FaroQemApp::with_paths(settings_path, vms_path, None);
    assert_eq!(fresh_app.settings.theme, "light");
    assert_eq!(fresh_app.settings.language, "de");
    assert_eq!(fresh_app.settings.default_arch, Arch::Aarch64);
}

// --------------------------------------------------------------------------
// Firmware: a UEFI machine the user ticked must be able to start
// --------------------------------------------------------------------------

/// A directory shaped like a portable QEMU install, firmware included.
///
/// The two file names are the ones `find_uefi_firmware` looks for; the
/// contents do not matter here, only that the pair is found and copied.
fn portable_qemu_dir(root: &std::path::Path) -> std::path::PathBuf {
    let share = root.join("share");
    std::fs::create_dir_all(&share).unwrap();
    std::fs::write(share.join("edk2-x86_64-code.fd"), b"CODE").unwrap();
    std::fs::write(share.join("edk2-i386-vars.fd"), b"VARS").unwrap();
    root.to_path_buf()
}

fn app_with_qemu_dir(dir: &std::path::Path) -> FaroQemApp {
    let mut app = FaroQemApp::with_paths(dir.join("settings.json"), dir.join("vms.json"), None);
    app.settings.qemu_dir = Some(portable_qemu_dir(&dir.join("qemu")));
    app.refresh_capabilities();
    app
}

#[test]
fn test_firmware_is_found_in_the_configured_qemu_directory() {
    let dir = tempdir().unwrap();
    let app = app_with_qemu_dir(dir.path());
    let found = app
        .uefi_firmware
        .as_ref()
        .expect("a portable QEMU outside PATH still has its firmware beside it");
    // Without this the test would pass on the firmware of the system QEMU,
    // which is the very fallback that made a UEFI machine boot as BIOS.
    assert!(
        found.code.starts_with(dir.path()),
        "the configured directory must win, got: {}",
        found.code.display()
    );
}

#[test]
fn test_starting_a_uefi_vm_creates_the_variable_store_it_needs() {
    let dir = tempdir().unwrap();
    let app = app_with_qemu_dir(dir.path());

    let nvram = dir.path().join("win11").join("nvram.fd");
    let mut vm = VmConfig::new_default("Windows 11", Arch::X86_64);
    vm.firmware = Firmware::Uefi {
        nvram: nvram.clone(),
    };

    app.ensure_nvram(&vm).expect("the store is ours to create");
    assert!(
        nvram.is_file(),
        "without this file QEMU answers: could not open {}: file not found",
        nvram.display()
    );
    assert_eq!(std::fs::read(&nvram).unwrap(), b"VARS");

    // A second start must not wipe the boot entries the guest wrote.
    std::fs::write(&nvram, b"GUEST BOOT ENTRIES").unwrap();
    app.ensure_nvram(&vm).unwrap();
    assert_eq!(std::fs::read(&nvram).unwrap(), b"GUEST BOOT ENTRIES");
}

#[test]
fn test_a_uefi_vm_is_refused_when_no_firmware_was_found() {
    let dir = tempdir().unwrap();
    let mut app = FaroQemApp::with_paths(
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
        None,
    );
    // What a machine without any edk2 files looks like. It cannot be staged
    // through the settings: with no binary in the configured directory the
    // search falls back to the system QEMU, and finds its firmware.
    app.uefi_firmware = None;

    let mut vm = VmConfig::new_default("Windows 11", Arch::X86_64);
    vm.firmware = Firmware::Uefi {
        nvram: dir.path().join("nvram.fd"),
    };

    let err = app
        .ensure_nvram(&vm)
        .expect_err("a UEFI machine with no firmware must not quietly boot as BIOS");
    assert!(
        err.to_lowercase().contains("uefi"),
        "the message must name what is missing, got: {}",
        err
    );
}

// --------------------------------------------------------------------------
// Part 3a, task 5: creating a machine, and which way the New button takes
// --------------------------------------------------------------------------

#[test]
fn test_a_fresh_installation_starts_in_the_wizard_and_an_existing_one_does_not() {
    // Spec 11.7: a schema marker tells the two apart. A plain serde default
    // would push the wizard onto people who already have machines.
    let dir = tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let vms = dir.path().join("vms.json");

    let fresh = FaroQemApp::with_paths(settings.clone(), vms.clone(), None);
    assert_eq!(fresh.settings.creation_mode, CreationMode::Wizard);
    drop(fresh);

    // An old installation: a settings file without the marker, beside a list.
    std::fs::write(&settings, br#"{"language":"de","theme":"dark"}"#).unwrap();
    std::fs::write(&vms, b"[]").unwrap();
    let legacy = FaroQemApp::with_paths(settings, vms, None);
    assert_eq!(
        legacy.settings.creation_mode,
        CreationMode::Direct,
        "an existing installation keeps the way it knows"
    );
}

#[test]
fn test_a_settings_file_without_the_marker_but_without_a_list_is_fresh() {
    let dir = tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    std::fs::write(&settings, br#"{"language":"en","theme":"dark"}"#).unwrap();
    let app = FaroQemApp::with_paths(settings, dir.path().join("vms.json"), None);
    assert_eq!(app.settings.creation_mode, CreationMode::Wizard);
}

#[test]
fn test_a_saved_settings_file_carries_the_marker_from_then_on() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    save_settings(&path, &AppSettings::default()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("schema"),
        "without the marker every start looks fresh again: {}",
        text
    );
}

#[test]
fn test_creating_from_the_wizard_makes_the_folder_the_disk_and_the_nvram() {
    let dir = tempdir().unwrap();
    let vm_dir = dir.path().join("FARO-VMs");
    let mut app = common::app_with_vm_dir(&vm_dir);

    let draft = VmTemplate::by_key("windows-11")
        .unwrap()
        .instantiate("Win 11", Path::new(""));
    let id = app
        .create_from_wizard(draft, 1)
        .expect("the machine is created");

    let vm = app.vms.iter().find(|v| v.id == id).unwrap();
    assert!(
        vm.drives[0].path.is_file(),
        "no disk at {}",
        vm.drives[0].path.display()
    );
    assert!(vm.drives[0].path.starts_with(&vm_dir));
    match &vm.firmware {
        Firmware::Uefi { nvram } => {
            assert!(nvram.is_file(), "no variable store at {}", nvram.display());
            assert_eq!(
                nvram.parent(),
                vm.drives[0].path.parent(),
                "the store travels with the machine"
            );
        }
        other => panic!("windows-11 is a UEFI template, got {:?}", other),
    }
}

#[test]
fn test_an_unwritable_vm_directory_leaves_no_half_made_vm() {
    let dir = tempdir().unwrap();
    let blocker = dir.path().join("vms-root");
    std::fs::write(&blocker, b"a file where the directory must go").unwrap();
    let mut app = common::app_with_vm_dir(&blocker);
    let before = app.vms.len();

    let draft = VmTemplate::by_key("debian")
        .unwrap()
        .instantiate("Debian", Path::new(""));
    let err = app
        .create_from_wizard(draft, 1)
        .expect_err("an unusable VM directory must be reported");
    assert!(err.contains("vms-root"), "name the path, got: {}", err);
    assert_eq!(
        app.vms.len(),
        before,
        "a failed creation must not leave a machine in the list"
    );
}

#[test]
fn test_a_failed_disk_leaves_no_vm_pointing_at_nothing() {
    let dir = tempdir().unwrap();
    let vm_dir = dir.path().join("FARO-VMs");
    std::fs::create_dir_all(&vm_dir).unwrap();
    let mut app = common::app_with_vm_dir(&vm_dir);

    let draft = VmTemplate::by_key("alpine")
        .unwrap()
        .instantiate("Alpine", Path::new(""));
    // Claim the folder the app will claim, then put a directory where the
    // disk image has to go: qemu-img cannot write a file over a directory.
    let folder = claim_vm_folder(&vm_dir, "Alpine", draft.id).unwrap();
    std::fs::create_dir_all(folder.join("disk.qcow2")).unwrap();

    let before = app.vms.len();
    let err = app
        .create_from_wizard(draft, 1)
        .expect_err("a disk that cannot be made must be reported");
    assert!(!err.is_empty());
    assert_eq!(
        app.vms.len(),
        before,
        "no machine may point at a disk that is not there"
    );
}

#[test]
fn test_the_empty_tile_creates_a_machine_with_no_disk() {
    let dir = tempdir().unwrap();
    let vm_dir = dir.path().join("FARO-VMs");
    let mut app = common::app_with_vm_dir(&vm_dir);

    let draft = VmConfig::new_default("Blank", Arch::X86_64);
    let id = app.create_from_wizard(draft, 32).unwrap();
    let vm = app.vms.iter().find(|v| v.id == id).unwrap();
    assert!(vm.drives.is_empty(), "the empty tile promises nothing");
    assert_eq!(vm.template, None);
}

#[test]
fn test_the_folder_a_machine_gets_is_named_after_it() {
    let dir = tempdir().unwrap();
    let vm_dir = dir.path().join("FARO-VMs");
    let mut app = common::app_with_vm_dir(&vm_dir);
    let draft = VmTemplate::by_key("alpine")
        .unwrap()
        .instantiate("Alpine Test", Path::new(""));
    let expected = vm_folder_name("Alpine Test", draft.id);
    let id = app.create_from_wizard(draft, 1).unwrap();
    let vm = app.vms.iter().find(|v| v.id == id).unwrap();
    assert_eq!(
        vm.drives[0].path.parent().unwrap().file_name().unwrap(),
        expected.as_str()
    );
}

#[test]
fn test_the_new_button_takes_the_way_the_settings_say() {
    let dir = tempdir().unwrap();
    let vm_dir = dir.path().join("FARO-VMs");

    let mut app = common::app_with_vm_dir(&vm_dir);
    app.settings.creation_mode = CreationMode::Wizard;
    let before = app.vms.len();
    app.handle_sidebar_action(SidebarAction::NewVm, None);
    assert!(app.wizard.open, "the wizard way opens the wizard");
    assert_eq!(app.vms.len(), before, "and creates nothing yet");

    let mut app = common::app_with_vm_dir(&vm_dir);
    app.settings.creation_mode = CreationMode::Direct;
    let before = app.vms.len();
    app.handle_sidebar_action(SidebarAction::NewVm, None);
    assert!(!app.wizard.open, "the direct way does not");
    assert_eq!(
        app.vms.len(),
        before + 1,
        "it adds an empty machine, exactly as before"
    );
}

// Important 6
#[test]
fn test_duplicating_a_uefi_vm_gives_the_copy_its_own_variable_store() {
    let dir = tempdir().unwrap();
    let app_dir = dir.path();
    let mut app = app_with_qemu_dir(app_dir);

    let disk = app_dir.join("vm").join("disk.qcow2");
    std::fs::create_dir_all(disk.parent().unwrap()).unwrap();
    std::fs::write(&disk, b"DISK").unwrap();

    let mut vm = VmConfig::new_default("Windows 11", Arch::X86_64);
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::Sata,
        boot_index: None,
    });
    vm.firmware = Firmware::Uefi {
        nvram: nvram_path_for(&vm),
    };
    let original_nvram = match &vm.firmware {
        Firmware::Uefi { nvram } => nvram.clone(),
        _ => unreachable!(),
    };
    std::fs::write(&original_nvram, b"ORIGINAL BOOT ENTRIES").unwrap();

    let original_id = vm.id;
    app.vms.push(vm);
    app.handle_sidebar_action(SidebarAction::DuplicateVm(original_id), None);

    let copy = app
        .vms
        .iter()
        .find(|v| v.id != original_id && v.name.contains("Windows 11"))
        .expect("the copy is in the list");
    let copy_nvram = match &copy.firmware {
        Firmware::Uefi { nvram } => nvram.clone(),
        other => panic!("the copy must stay a UEFI machine, got {:?}", other),
    };

    assert_ne!(
        copy_nvram, original_nvram,
        "one store for two machines means they overwrite each other's boot entries"
    );
    assert_eq!(
        std::fs::read(&copy_nvram).unwrap(),
        b"ORIGINAL BOOT ENTRIES",
        "the copy of an installed machine keeps its boot entries"
    );
    assert_eq!(
        std::fs::read(&original_nvram).unwrap(),
        b"ORIGINAL BOOT ENTRIES",
        "and the original is untouched"
    );
}

/// The whole UEFI path, through the app rather than through a builder.
///
/// Every other firmware test checks one piece: that the store is created,
/// that the arguments are refused without firmware, that the probe accepts a
/// pflash pair. None of them answers the question the user asks — does the
/// machine I ticked UEFI for actually start — and for the whole of this
/// branch the answer was no while every piece passed.
#[test]
fn test_a_uefi_machine_made_the_way_the_interface_makes_one_actually_starts() {
    if faro_qem::qemu::find_qemu_binary(None, Arch::X86_64).is_none() {
        eprintln!("skipped: no qemu-system-x86_64 on this machine");
        return;
    }
    let dir = tempdir().unwrap();
    let mut app = FaroQemApp::with_paths(
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
        None,
    );
    if app.uefi_firmware.is_none() {
        eprintln!("skipped: no edk2 firmware on this machine");
        return;
    }

    let disk = dir.path().join("win11").join("disk.qcow2");
    std::fs::create_dir_all(disk.parent().unwrap()).unwrap();
    let img = faro_qem::qemu::find_qemu_img(None).expect("qemu-img sits beside qemu");
    faro_qem::qemu::create_disk_image(&img, &disk, DriveFormat::Qcow2, 1).unwrap();

    // Exactly what the interface produces: SATA so the installer sees the
    // disk, and the firmware switch set to UEFI.
    let mut vm = VmConfig::new_default("Windows 11", Arch::X86_64);
    vm.memory_mb = 256;
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::Sata,
        boot_index: Some(0),
    });
    vm.firmware = Firmware::Uefi {
        nvram: nvram_path_for(&vm),
    };
    // No window, so the suite stays quiet.
    vm.display.vga_type = "none".to_string();
    vm.display.display_backend = "none".to_string();

    let id = vm.id;
    app.vms.push(vm);
    app.launch_vm(id, None).expect("the machine starts");

    // Give QEMU a moment to reject the arguments if it is going to.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let status = app.status_map.get(&id).cloned();
    let said = app
        .console_logs
        .get(&id)
        .map(|b| b.all_text())
        .unwrap_or_default();
    let _ = app.stop_vm(id);

    assert!(
        matches!(status, Some(VmStatus::Running { .. })),
        "status was {:?}, QEMU said:\n{}",
        status,
        said
    );
}
