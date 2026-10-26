// FARO QEMU MANAGER (C) R.Trunk 2026
// Integration & End-to-End Application State Tests

mod common;

use egui::Context;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use tempfile::tempdir;

use faro_qem::app::FaroQemApp;
use faro_qem::config::{
    claim_vm_folder, load_settings, load_vms, save_settings, save_vms, try_load_vms,
    vm_folder_name, Accelerator, AppSettings, Arch, CreationMode, DriveConfig, DriveFormat,
    DriveInterface, Firmware, VmConfig,
};
use faro_qem::qemu::{nvram_path_for, spawn_qemu, QemuEvent};
use faro_qem::runstate::{load, put, RunRecord};
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

    // No machine is invented on a first start, and nothing is selected --
    // there is nothing to select. A sample machine used to be seeded here,
    // which left someone who had just installed FARO looking at a machine
    // they never made.
    assert!(app.vms.is_empty());
    assert_eq!(app.selected_vm_id, None);

    // Default settings were initialized
    assert_eq!(app.settings.theme, "dark");
    assert_eq!(app.settings.language, "en");
    assert_eq!(app.settings.default_arch, Arch::X86_64);

    // Initial state flags
    assert!(!app.settings_open);
    assert_eq!(app.active_tab, DetailTab::System);
    assert!(app.running_processes.is_empty());
    assert!(app.status_map.is_empty());

    // And nothing was written: the user has not asked for anything yet.
    assert!(!vms_path.exists());
    assert!(load_vms(&vms_path).is_empty());
}

#[test]
fn test_faro_qem_app_sidebar_actions() {
    let dir = tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    let vms_path = dir.path().join("vms.json");

    let mut app = FaroQemApp::with_paths(settings_path, vms_path.clone(), None);
    let initial_vm_id = common::push_plain_vm(&mut app);

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
    app.handle_sidebar_action(
        SidebarAction::DeleteVm {
            id: dup_vm_id,
            with_folder: false,
        },
        None,
    );
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
    let vm_id = common::push_plain_vm(&mut app);

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
    let vm_id = common::push_plain_vm(&mut app);

    // Simulate process registration with a dummy channel
    let (tx, rx) = mpsc::channel();
    // To construct a VmProcess for mock testing, run a fast system command (rustc --version)
    let rustc_bin = std::path::PathBuf::from("rustc");
    let (dummy_tx, _dummy_rx) = mpsc::channel();
    let vm_proc = spawn_qemu(&rustc_bin, &["--version".to_string()], dummy_tx, None)
        .expect("Failed to spawn rustc for process test fixture");

    app.running_processes
        .insert(vm_id, faro_qem::app::Attached::Owned { proc: vm_proc, rx });

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
    let vm_id = common::push_plain_vm(&mut app);

    let (tx, rx) = mpsc::channel();
    let rustc_bin = std::path::PathBuf::from("rustc");
    let (dummy_tx, _dummy_rx) = mpsc::channel();
    let vm_proc = spawn_qemu(&rustc_bin, &["--version".to_string()], dummy_tx, None).unwrap();

    app.running_processes
        .insert(vm_id, faro_qem::app::Attached::Owned { proc: vm_proc, rx });

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
    let vm_id = common::push_plain_vm(&mut app);

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
    let vm_id = common::push_plain_vm(&mut app);

    let (_tx, rx) = mpsc::channel();
    let rustc_bin = std::path::PathBuf::from("rustc");
    let (dummy_tx, _dummy_rx) = mpsc::channel();
    let vm_proc = spawn_qemu(&rustc_bin, &["--version".to_string()], dummy_tx, None).unwrap();

    app.running_processes
        .insert(vm_id, faro_qem::app::Attached::Owned { proc: vm_proc, rx });
    app.status_map
        .insert(vm_id, VmStatus::Running { pid: 7777 });

    let res = app.force_stop_vm(vm_id);
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
    let _ = app.force_stop_vm(id);

    assert!(
        matches!(status, Some(VmStatus::Running { .. })),
        "status was {:?}, QEMU said:\n{}",
        status,
        said
    );
}

// --------------------------------------------------------------------------
// Part 3a, task 7: a copy gets its own image, deleting asks about the folder
// --------------------------------------------------------------------------

/// A machine from the alpine template, with a real disk made by qemu-img.
fn alpine_in(app: &mut FaroQemApp, name: &str) -> uuid::Uuid {
    let draft = VmTemplate::by_key("alpine")
        .unwrap()
        .instantiate(name, Path::new(""));
    app.create_from_wizard(draft, 1)
        .expect("the machine is created")
}

/// A machine the user built by hand, its disk somewhere of their choosing.
fn hand_made_pointing_at(app: &mut FaroQemApp, disk: &Path) -> uuid::Uuid {
    let mut vm = VmConfig::new_default("hand-made", Arch::X86_64);
    vm.drives.push(DriveConfig {
        path: disk.to_path_buf(),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: None,
    });
    let id = vm.id;
    app.vms.push(vm);
    id
}

#[test]
fn test_duplicating_gives_the_copy_its_own_disk_file() {
    // Spec section 5.7: two QEMU processes writing one qcow2 destroy it.
    let dir = tempdir().unwrap();
    let mut app = common::app_with_vm_dir(&dir.path().join("vms"));
    let id = alpine_in(&mut app, "Alpine");
    let original = app.vms.iter().find(|v| v.id == id).unwrap().drives[0]
        .path
        .clone();
    std::fs::write(&original, b"ORIGINAL IMAGE").unwrap();

    app.handle_sidebar_action(SidebarAction::DuplicateVm(id), None);

    let copy = app
        .vms
        .iter()
        .find(|v| v.id != id && v.name.contains("Alpine"))
        .expect("the copy is in the list");
    assert_ne!(
        copy.drives[0].path, original,
        "the copy must not point at the original image"
    );
    assert_eq!(
        std::fs::read(&copy.drives[0].path).unwrap(),
        b"ORIGINAL IMAGE",
        "the copy's image carries the original's contents"
    );
    assert_eq!(
        std::fs::read(&original).unwrap(),
        b"ORIGINAL IMAGE",
        "the original is untouched"
    );
}

#[test]
fn test_deleting_without_consent_keeps_the_folder() {
    let dir = tempdir().unwrap();
    let mut app = common::app_with_vm_dir(&dir.path().join("vms"));
    let id = alpine_in(&mut app, "Alpine");
    let folder = app.vms.iter().find(|v| v.id == id).unwrap().drives[0]
        .path
        .parent()
        .unwrap()
        .to_path_buf();

    app.delete_vm(id, false);

    assert!(app.vms.iter().all(|v| v.id != id), "the machine is gone");
    assert!(
        folder.is_dir(),
        "the folder is the user's data, not ours to take unasked"
    );
}

#[test]
fn test_deleting_with_consent_takes_the_folder() {
    let dir = tempdir().unwrap();
    let mut app = common::app_with_vm_dir(&dir.path().join("vms"));
    let id = alpine_in(&mut app, "Alpine");
    let folder = app.vms.iter().find(|v| v.id == id).unwrap().drives[0]
        .path
        .parent()
        .unwrap()
        .to_path_buf();

    app.delete_vm(id, true);

    assert!(!folder.exists(), "consent was given");
}

#[test]
fn test_deleting_never_reaches_outside_the_vm_directory() {
    // A VM whose disk the user pointed somewhere else must not drag that
    // directory away with it.
    let dir = tempdir().unwrap();
    let outside = dir.path().join("my-documents");
    std::fs::create_dir_all(&outside).unwrap();
    let disk = outside.join("important.qcow2");
    std::fs::write(&disk, b"KEEP").unwrap();

    let mut app = common::app_with_vm_dir(&dir.path().join("vms"));
    let id = hand_made_pointing_at(&mut app, &disk);

    app.delete_vm(id, true);

    assert!(
        disk.is_file(),
        "only a folder under the VM directory may be taken"
    );
    assert!(outside.is_dir(), "the user's own directory stays");
}

#[test]
fn test_duplicating_a_hand_made_vm_says_the_image_stays_shared() {
    // An image outside the VM directory is not ours to copy; the user hears
    // that the two machines share it rather than finding out by corruption.
    let dir = tempdir().unwrap();
    let outside = dir.path().join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    let disk = outside.join("shared.qcow2");
    std::fs::write(&disk, b"SHARED").unwrap();

    let mut app = common::app_with_vm_dir(&dir.path().join("vms"));
    let id = hand_made_pointing_at(&mut app, &disk);
    app.notices.clear();

    app.handle_sidebar_action(SidebarAction::DuplicateVm(id), None);

    let said = app
        .notices
        .iter()
        .map(|n| n.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("shared.qcow2"),
        "a shared image must be said out loud, naming it; notices were: {:?}",
        said
    );
    let copy = app
        .vms
        .iter()
        .find(|v| v.id != id && v.name.contains("hand-made"))
        .expect("the copy is in the list");
    assert_eq!(
        copy.drives[0].path, disk,
        "an image outside the VM directory stays a shared reference"
    );
}

// --------------------------------------------------------------------------
// Final review of the branch: what the fix pass answers
// --------------------------------------------------------------------------

#[test]
fn test_a_copied_folder_with_the_same_marker_is_not_the_one_taken() {
    // C1: copying a VM folder in Explorer copies its vm-id marker too. Two
    // folders then claim the machine, and taking "the first one found" is a
    // coin toss over the user's backup.
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = alpine_in(&mut app, "Debian");

    let real = app.vms.iter().find(|v| v.id == id).unwrap().drives[0]
        .path
        .parent()
        .unwrap()
        .to_path_buf();
    // A backup whose name sorts ahead of the original.
    let backup = root.join("01-Backup of Debian");
    std::fs::create_dir_all(&backup).unwrap();
    std::fs::copy(real.join("vm-id"), backup.join("vm-id")).unwrap();
    std::fs::write(backup.join("KEEP.qcow2"), b"the user's backup").unwrap();

    app.delete_vm(id, true);

    assert!(
        backup.is_dir() && backup.join("KEEP.qcow2").is_file(),
        "the backup is not the machine's folder and must survive"
    );
    assert!(
        !real.exists(),
        "the folder that actually holds the disk is the one consent was for"
    );
}

#[test]
fn test_an_ambiguous_folder_is_taken_from_nobody() {
    // Same marker twice and no disk to break the tie: guessing would be a
    // guess about which directory to destroy.
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);

    let mut vm = VmConfig::new_default("empty", Arch::X86_64);
    let id = vm.id;
    vm.drives.clear();
    app.vms.push(vm);

    let a = claim_vm_folder(&root, "empty", id).unwrap();
    let b = root.join("00-also-empty");
    std::fs::create_dir_all(&b).unwrap();
    std::fs::copy(a.join("vm-id"), b.join("vm-id")).unwrap();

    app.delete_vm(id, true);

    assert!(
        a.is_dir() && b.is_dir(),
        "neither folder may be removed on a guess"
    );
}

#[test]
fn test_a_folder_holding_another_machines_disk_is_not_taken() {
    // I7: the user pointed VM B's disk into VM A's folder. Deleting A with
    // consent must not take B's disk with it; B's dialog never mentioned it.
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let a = alpine_in(&mut app, "A");
    let a_folder = app.vms.iter().find(|v| v.id == a).unwrap().drives[0]
        .path
        .parent()
        .unwrap()
        .to_path_buf();

    let b_disk = a_folder.join("b.qcow2");
    std::fs::write(&b_disk, b"B's DISK").unwrap();
    let _b = hand_made_pointing_at(&mut app, &b_disk);

    app.delete_vm(a, true);

    assert!(
        b_disk.is_file(),
        "another machine's disk lives in that folder"
    );
}

#[test]
fn test_every_image_the_copy_still_shares_is_named() {
    // I1: whatever the reason a copy ends up on the original's image -- the
    // folder could not be claimed, the copy failed, the file is elsewhere --
    // the user hears which image, by name. Silence here costs a disk.
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = alpine_in(&mut app, "Alpine");

    // A second drive inside the VM directory whose file is not there, so the
    // copy of it cannot succeed.
    let folder = app.vms.iter().find(|v| v.id == id).unwrap().drives[0]
        .path
        .parent()
        .unwrap()
        .to_path_buf();
    let ghost = folder.join("second.qcow2");
    let pos = app.vms.iter().position(|v| v.id == id).unwrap();
    app.vms[pos].drives.push(DriveConfig {
        path: ghost.clone(),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: None,
    });
    let originals: Vec<_> = app.vms[pos].drives.iter().map(|d| d.path.clone()).collect();
    app.notices.clear();

    app.handle_sidebar_action(SidebarAction::DuplicateVm(id), None);

    let said = app
        .notices
        .iter()
        .map(|n| n.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let copy = app
        .vms
        .iter()
        .find(|v| v.id != id && v.name.contains("Alpine"))
        .expect("the copy is in the list");
    for (index, drive) in copy.drives.iter().enumerate() {
        if drive.path == originals[index] {
            let name = drive.path.file_name().unwrap().to_string_lossy();
            assert!(
                said.contains(name.as_ref()),
                "the copy still shares {} and nothing said so; notices were:\n{}",
                name,
                said
            );
        }
    }
}

#[test]
fn test_a_missing_image_is_not_called_outside_the_vm_directory() {
    // An image the user deleted in Explorer is inside the VM directory and
    // gone. Saying "lies outside the VM directory" is false in both halves.
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = alpine_in(&mut app, "Alpine");
    let disk = app.vms.iter().find(|v| v.id == id).unwrap().drives[0]
        .path
        .clone();
    std::fs::remove_file(&disk).unwrap();
    app.notices.clear();

    app.handle_sidebar_action(SidebarAction::DuplicateVm(id), None);

    let said = app
        .notices
        .iter()
        .map(|n| n.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains(&disk.file_name().unwrap().to_string_lossy().to_string()),
        "the image is named, got:\n{}",
        said
    );
    // The catalogue has one sentence for "outside the VM directory" and one
    // for "not there at all". A gone file must get the second.
    let i18n = faro_qem::i18n::I18n::new("en");
    // Both sentences open with the same words and differ in what follows the
    // path, so that is the part worth comparing.
    let marker_of = |key: &str| {
        i18n.t(key)
            .split("{path}")
            .nth(1)
            .unwrap_or("")
            .split('.')
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let missing = marker_of("duplicate_image_missing");
    let outside = marker_of("duplicate_image_shared");
    assert!(
        !missing.is_empty() && said.contains(&missing),
        "a gone image is reported as gone; said:\n{}",
        said
    );
    assert!(
        !said.contains(&outside),
        "a gone image is not an image outside the directory; said:\n{}",
        said
    );
}

#[test]
fn test_a_copy_whose_folder_cannot_be_made_still_names_every_shared_image() {
    // I1: the folder claim is the one failure that used to break out of the
    // loop, leaving the copy on the original's images in silence. The
    // trigger is ordinary: a name long enough that the copy's folder path
    // passes Windows' limit.
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let orig = root.join("orig");
    std::fs::create_dir_all(&orig).unwrap();
    let disk = orig.join("disk.qcow2");
    std::fs::write(&disk, b"IMAGE").unwrap();

    let mut app = common::app_with_vm_dir(&root);
    let long_name = "L".repeat(240);
    let mut vm = VmConfig::new_default(&long_name, Arch::X86_64);
    vm.drives.push(DriveConfig {
        path: disk.clone(),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: None,
    });
    let id = vm.id;
    app.vms.push(vm);
    app.notices.clear();

    app.handle_sidebar_action(SidebarAction::DuplicateVm(id), None);

    let copy = app
        .vms
        .iter()
        .find(|v| v.id != id && v.name.contains(&long_name))
        .expect("the copy is in the list");
    let said = app
        .notices
        .iter()
        .map(|n| n.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    if copy.drives[0].path == disk {
        assert!(
            said.contains("disk.qcow2"),
            "the copy shares the original's image and nothing named it; said:\n{}",
            said
        );
    }
}

#[test]
fn test_a_failed_creation_leaves_the_wizard_standing_with_its_entries() {
    // I4: a fresh installation defaults to the wizard and has no QEMU
    // directory set yet. Choosing a template, typing a name and picking a
    // 6 GB ISO, only to have all three thrown away with the message, means
    // doing it again -- with the same result, until the settings are found.
    let dir = tempdir().unwrap();
    let vm_dir = dir.path().join("FARO-VMs");
    std::fs::create_dir_all(&vm_dir).unwrap();
    let mut app = common::app_with_vm_dir(&vm_dir);

    // A VM directory that cannot hold a folder: the claim fails.
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, b"a file where the directory must go").unwrap();
    app.settings.vm_dir = Some(blocked);

    app.wizard.select("alpine");
    app.wizard.name = "Alpine".to_string();
    app.wizard.iso = r"C:\isos\alpine.iso".to_string();

    let Some(draft) = app.wizard.take_draft() else {
        panic!("the draft is handed out");
    };
    let err = app
        .create_from_wizard(draft, 1)
        .expect_err("an unusable VM directory must be reported");
    assert!(!err.is_empty());

    // The caller tells the wizard it did not work. Everything the user typed
    // is still there and the dialog is still on screen.
    app.wizard.creation_failed();
    assert!(app.wizard.open, "the dialog stays, or the entries are lost");
    assert_eq!(app.wizard.name, "Alpine");
    assert_eq!(app.wizard.iso, r"C:\isos\alpine.iso");
    assert!(
        app.wizard.why_not_create().is_none(),
        "the entries are complete, so Create is still offered"
    );
}

// --------------------------------------------------------------------------
// Part 3b, task 4: the two-click promise for someone with no ISO
// --------------------------------------------------------------------------

#[test]
fn test_creating_with_a_download_fills_the_iso_slot_right_away() {
    // Spec section 11.5: the machine is created at once, with the target
    // path as its ISO. The two-click promise has to hold for someone who
    // does not have an image yet.
    let dir = tempdir().unwrap();
    let mut app = common::app_with_vm_dir(&dir.path().join("vms"));
    let mut draft = VmTemplate::by_key("debian")
        .unwrap()
        .instantiate("Debian", Path::new(""));
    draft.cdroms.push(PathBuf::from("placeholder.iso"));

    let id = app.create_from_wizard(draft, 1).unwrap();

    let vm = app.vms.iter().find(|v| v.id == id).unwrap();
    assert_eq!(
        vm.cdroms.len(),
        1,
        "the machine exists with an ISO slot already filled"
    );
}

#[test]
fn test_a_download_for_a_template_without_a_source_starts_nothing() {
    // Windows has no stable link, so there is nothing to start and nothing
    // to leave half-done.
    let dir = tempdir().unwrap();
    let mut app = common::app_with_vm_dir(&dir.path().join("vms"));
    app.wizard.select("windows-11");
    app.wizard.name = "Win 11".to_string();
    app.wizard.request_download();

    assert!(
        faro_qem::download::source_for("windows-11").is_none(),
        "no mirror we can check"
    );
    assert!(
        faro_qem::download::official_page("windows-11").is_some(),
        "but a page the user can open"
    );
    assert!(app.downloads.is_empty());
}

// --------------------------------------------------------------------------
// From the whole-branch review of part 3b
// --------------------------------------------------------------------------

#[test]
fn test_a_machine_awaiting_a_download_never_looks_ready_to_start() {
    // I6: between creation and the moment a mirror answers, the machine had
    // no ISO at all -- so the start check found nothing missing and QEMU
    // launched with no installation medium. The same gap swallowed the case
    // where the mirror is unreachable: no ISO, no .part, no finding, and a
    // machine that installs nothing.
    let dir = tempdir().unwrap();
    let mut app = common::app_with_vm_dir(&dir.path().join("vms"));
    app.wizard.select("debian");
    app.wizard.name = "Debian".to_string();
    app.wizard.request_download();

    let draft = app.wizard.take_draft().expect("there is a draft");
    assert!(
        !draft.cdroms.is_empty(),
        "a machine that is waiting for an image must say so, not look complete"
    );

    let id = app.create_from_wizard(draft, 1).expect("created");
    let vm = app.vms.iter().find(|v| v.id == id).unwrap().clone();
    assert!(
        faro_qem::preflight::blocks(&app.findings_for(&vm)),
        "and the start check must hold the start: {:?}",
        app.findings_for(&vm)
    );
}

#[test]
fn test_choosing_a_file_by_hand_is_not_overwritten_by_the_download() {
    // I2: request_download was a one-way latch, so a user who clicked
    // Download, changed their mind and browsed for the ISO they already had,
    // got their choice thrown away seconds later -- and an 800 MB transfer
    // they had cancelled in their head. Spec section 1's anti-goal verbatim.
    let mut state = faro_qem::ui::wizard::WizardState::opened();
    state.select("debian");
    state.request_download();
    assert!(state.wants_download());

    // Naming a file is withdrawing the request to fetch one.
    state.iso = r"D:\isos\debian-13.7.0-amd64-netinst.iso".to_string();
    assert!(
        !state.wants_download(),
        "the user's own choice wins over a request they changed their mind about"
    );

    // And picking a template again starts over.
    state.request_download();
    state.select("alpine");
    assert!(!state.wants_download(), "a new template is a fresh start");
}

#[test]
fn test_deleting_a_machine_stops_its_download() {
    // I3: the worker kept writing into a folder that was being removed, and
    // on success renamed a multi-gigabyte image into a folder the user had
    // just deleted.
    let dir = tempdir().unwrap();
    let mut app = common::app_with_vm_dir(&dir.path().join("vms"));
    let id = alpine_in(&mut app, "Alpine");

    let body = vec![4u8; 64 * 1024];
    let target = dir.path().join("vms").join("x.iso");
    app.downloads.insert(
        id,
        faro_qem::download::start(
            std::sync::Arc::new(common::SlowHttp::serving(&body)),
            faro_qem::download::Resolved {
                url: "https://example/x.iso".to_string(),
                file_name: "x.iso".to_string(),
                sha256: "0".repeat(64),
            },
            target,
        ),
    );

    app.delete_vm(id, false);

    assert!(
        !app.downloads.contains_key(&id),
        "the handle goes with the machine"
    );
}

// --------------------------------------------------------------------------
// Part 4, task 2: the stop button shuts the guest down; killing is a second,
// named step
// --------------------------------------------------------------------------

#[test]
fn test_launching_puts_a_qmp_port_on_the_command_line() {
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    // A subdirectory, so the app's own files land inside the temporary
    // directory rather than beside it -- see app_with_vm_dir.
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_startable_vm(&mut app, &root);
    app.launch_vm(id, None).unwrap();
    let cli = app.cli_previews.get(&id).unwrap().clone();
    // The port the app remembers has to be the port the guest was started
    // with: a shutdown aimed at any other one reaches nothing. The preview
    // quotes the argument, so the port is checked, not the quoting.
    let port = *app
        .qmp_ports
        .get(&id)
        .expect("a started machine has a monitor port");
    let _ = app.force_stop_vm(id);
    assert!(
        cli.contains("-qmp") && cli.contains(&format!("tcp:127.0.0.1:{},server=on,wait=off", port)),
        "got: {}",
        cli
    );
}

#[test]
fn test_shutdown_asks_the_guest_and_shows_that_it_is_waiting() {
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    // A subdirectory, so the app's own files land inside the temporary
    // directory rather than beside it -- see app_with_vm_dir.
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_startable_vm(&mut app, &root);
    app.launch_vm(id, None).unwrap();

    let outcome = app.shutdown_vm(id);
    let status = app.status_map.get(&id).cloned();
    let attached = app.is_attached(id);
    let said = app
        .console_logs
        .get(&id)
        .map(|b| b.all_text())
        .unwrap_or_default();
    let _ = app.force_stop_vm(id);

    outcome.expect("the guest is asked to power down");
    assert!(
        matches!(status, Some(VmStatus::ShuttingDown { .. })),
        "got: {:?}",
        status
    );
    // A guest with no operating system acts on nothing: the process stays,
    // which is exactly the hanging-guest case of Review Focus 2.
    assert!(attached, "the process must still be there");
    // QEMU answers system_powerdown with a POWERDOWN event before its
    // return, and what the machine said belongs in its console.
    assert!(
        said.contains("POWERDOWN"),
        "the console must show what QEMU reported, said:\n{}",
        said
    );
}

#[test]
fn test_a_vm_without_a_qmp_port_is_not_quietly_killed() {
    // Spec section 7.1: a button that says shutdown never pulls the plug.
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    // A subdirectory, so the app's own files land inside the temporary
    // directory rather than beside it -- see app_with_vm_dir.
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_startable_vm(&mut app, &root);
    app.launch_vm(id, None).unwrap();
    app.drop_qmp_port(id); // what an older state file looks like

    let outcome = app.shutdown_vm(id);
    let attached = app.is_attached(id);
    let _ = app.force_stop_vm(id);

    let err = outcome.expect_err("without QMP there is no graceful path");
    assert!(!err.is_empty(), "the reason has to reach the user");
    assert!(attached, "the guest must still be running");
}

// --------------------------------------------------------------------------
// Part 4, task 7: nothing expensive happens while drawing a frame
// --------------------------------------------------------------------------

#[test]
fn test_no_qemu_search_happens_while_drawing() {
    // Spec section 7.2: searching the file system for QEMU meant a handful
    // of `is_file` calls per frame, sixty times a second.
    let app_src = std::fs::read_to_string("src/app.rs").unwrap();
    let drawing = app_src
        .split("fn render_ui(")
        .nth(1)
        .expect("render_ui is the frame entry");
    assert!(
        !drawing.contains("find_qemu_img("),
        "src/app.rs searches for qemu-img while drawing"
    );
    assert!(
        !drawing.contains("find_qemu_binary("),
        "src/app.rs searches for QEMU while drawing"
    );

    // The settings dialog is nothing but drawing.
    let settings = std::fs::read_to_string("src/ui/settings.rs").unwrap();
    assert!(
        !settings.contains("find_qemu_binary("),
        "the settings dialog searches while drawing"
    );

    let detail = common::concat_sources("src/ui/vm_detail");
    assert!(
        !detail.contains("find_qemu_img("),
        "the per-frame fallback in the detail panel is still there"
    );
}

#[test]
fn test_the_resolved_qemu_img_comes_from_the_chosen_directory() {
    // Resolved once, and again when the directory changes -- never per
    // frame. The plan's version pointed the setting at an empty directory
    // and expected the answer to change, but the search falls back to PATH
    // and the standard locations, where this machine really has a QEMU.
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let before = app.qemu_img.clone();

    let chosen = dir.path().join("chosen");
    std::fs::create_dir_all(&chosen).unwrap();
    for name in ["qemu-img", "qemu-img.exe"] {
        std::fs::write(chosen.join(name), b"not really a binary").unwrap();
    }
    app.settings.qemu_dir = Some(chosen.clone());
    app.refresh_capabilities();

    assert_ne!(
        app.qemu_img, before,
        "changing the directory is what re-resolves it"
    );
    assert_eq!(
        app.qemu_img.as_ref().and_then(|p| p.parent()),
        Some(chosen.as_path()),
        "and the answer comes from the chosen directory, got: {:?}",
        app.qemu_img
    );
}

#[test]
fn test_creating_a_disk_does_not_block_the_caller() {
    // create_disk_image ran on the frame thread; a 64 GB image froze the
    // interface until qemu-img was done.
    if faro_qem::qemu::find_qemu_img(None).is_none() {
        eprintln!("skipped: no qemu-img");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let target = dir.path().join("big.qcow2");

    let started = std::time::Instant::now();
    app.start_disk_creation(target.clone(), DriveFormat::Qcow2, 64);
    assert!(
        started.elapsed() < std::time::Duration::from_millis(250),
        "the call must return at once, took {:?}",
        started.elapsed()
    );

    for _ in 0..200 {
        app.poll_disk_creation();
        if target.is_file() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(target.is_file(), "and the image is there afterwards");
}

#[test]
fn test_a_disk_that_cannot_be_created_says_so() {
    // The dialog used to close the moment the work was handed over, taking
    // the only place the answer is shown with it: neither "created" nor the
    // reason for a failure ever reached the user.
    if faro_qem::qemu::find_qemu_img(None).is_none() {
        eprintln!("skipped: no qemu-img");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    app.create_disk_modal.open = true;

    // A file where a directory would have to be: qemu-img cannot write here.
    let blocker = dir.path().join("as-a-file");
    std::fs::write(&blocker, b"x").unwrap();
    app.start_disk_creation(blocker.join("disk.qcow2"), DriveFormat::Qcow2, 1);

    for _ in 0..200 {
        app.poll_disk_creation();
        if app
            .create_disk_modal
            .status_msg
            .as_ref()
            .is_some_and(|m| m.is_err())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    assert!(
        app.create_disk_modal
            .status_msg
            .as_ref()
            .is_some_and(|m| m.is_err()),
        "the reason has to be in the dialog, got: {:?}",
        app.create_disk_modal.status_msg
    );
    assert!(
        app.create_disk_modal.open,
        "and the dialog stays open, or nobody reads it"
    );
    assert!(
        app.notices
            .iter()
            .any(|n| n.level == faro_qem::ui::NoticeLevel::Error),
        "and it survives the dialog being closed, as a notice"
    );
}

#[test]
fn test_a_finished_disk_is_attached_to_the_machine_that_asked() {
    if faro_qem::qemu::find_qemu_img(None).is_none() {
        eprintln!("skipped: no qemu-img");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_plain_vm(&mut app);
    app.selected_vm_id = Some(id);
    let before = app.vms[0].drives.len();
    let target = dir.path().join("attached.qcow2");

    app.start_disk_creation(target.clone(), DriveFormat::Qcow2, 1);
    for _ in 0..200 {
        app.poll_disk_creation();
        if app.vms[0].drives.len() > before {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert_eq!(
        app.vms[0].drives.len(),
        before + 1,
        "an image nobody attaches is an image the user has to add by hand"
    );
    assert_eq!(
        app.vms[0].drives.last().map(|d| d.path.clone()),
        Some(target)
    );
}

// --------------------------------------------------------------------------
// Part 4, task 6: pick the survivors up again, by handshake not by PID
// --------------------------------------------------------------------------

/// A record pointing wherever the test wants it to point.
fn record_for(vm_id: uuid::Uuid, pid: u32, port: u16, log: PathBuf) -> RunRecord {
    RunRecord {
        vm_id,
        pid,
        qmp_port: port,
        started_at: 0,
        log,
    }
}

#[test]
fn test_a_vm_that_outlived_the_app_is_picked_up_again() {
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let (settings, vms) = (
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
    );
    let (id, pid) = {
        let mut app = common::app_with_paths_and_vm_dir(&settings, &vms, dir.path());
        let id = common::push_startable_vm(&mut app, dir.path());
        app.launch_vm(id, None).unwrap();
        let pid = app.status_map.get(&id).and_then(|s| s.pid()).unwrap();
        drop(app);
        (id, pid)
    };

    let mut app = FaroQemApp::with_paths(settings, vms, None);
    let status = app.status_map.get(&id).cloned();
    let outcome = app.shutdown_vm(id);
    // Killed before the assertions: a guest left behind by a failing test
    // outlives the test run, and this one is meant to outlive its app.
    let _ = app.force_stop_vm(id);
    let _ = common::kill_pid(pid);
    assert!(
        matches!(status, Some(VmStatus::Running { .. })),
        "got: {:?}",
        status
    );
    outcome.expect("a reattached VM takes a shutdown request");
}

#[test]
fn test_a_reattached_vm_has_a_console_again() {
    // Spec section 7.3, point 4: the pipes belonged to the parent that died.
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let (settings, vms) = (
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
    );
    let (id, pid) = {
        let mut app = common::app_with_paths_and_vm_dir(&settings, &vms, dir.path());
        let id = common::push_startable_vm(&mut app, dir.path());
        app.launch_vm(id, None).unwrap();
        let pid = app.status_map.get(&id).and_then(|s| s.pid()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(600));
        drop(app);
        (id, pid)
    };

    let mut app = FaroQemApp::with_paths(settings, vms, None);
    let said = app
        .console_logs
        .get(&id)
        .map(|b| b.all_text())
        .unwrap_or_default();
    let _ = app.force_stop_vm(id);
    let _ = common::kill_pid(pid);
    assert!(
        !said.trim().is_empty(),
        "a reattached VM with an empty console is half the truth"
    );
}

#[test]
fn test_a_recycled_pid_is_never_adopted() {
    // Review Focus 1 and spec section 7.3: process numbers are reused. This
    // record points at a live process that is not our VM and answers no QMP.
    let dir = tempdir().unwrap();
    let (settings, vms) = (
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
    );
    let mut stranger = common::spawn_long_sleep();
    let vm = VmConfig::new_default("ghost", Arch::X86_64);
    let id = vm.id;
    save_vms(&vms, &[vm]).unwrap();
    let runstate = dir.path().join("running.json");
    put(
        &runstate,
        record_for(
            id,
            stranger.id(),
            faro_qem::qmp::free_local_port().unwrap(), // nothing listens there
            dir.path().join("ghost.log"),
        ),
    )
    .unwrap();

    let app = FaroQemApp::with_paths(settings, vms, None);
    let status = app.status_map.get(&id).cloned();
    let left_alone = common::pid_is_alive(stranger.id());
    let left_over = load(&runstate);
    let _ = common::kill_pid(stranger.id());
    let _ = stranger.wait();

    assert!(
        !matches!(status, Some(VmStatus::Running { .. })),
        "a live PID is not proof: got {:?}",
        status
    );
    assert!(
        left_over.is_empty(),
        "an unconfirmable record is cleaned up, got: {:?}",
        left_over
    );
    assert!(left_alone, "and the stranger is left alone");
}

#[test]
fn test_another_machines_qemu_on_the_recorded_port_is_not_adopted() {
    // The collision a handshake alone cannot see: loopback ports are handed
    // out again, so the QEMU answering on the recorded port may be a
    // different machine of ours. Adopting it would mean shutting down the
    // wrong guest on the next click.
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let (settings, vms) = (
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
    );
    let mut app = common::app_with_paths_and_vm_dir(&settings, &vms, dir.path());
    let running = common::push_startable_vm(&mut app, dir.path());
    let impostor = VmConfig::new_default("impostor", Arch::X86_64);
    let impostor_id = impostor.id;
    app.vms.push(impostor);
    app.launch_vm(running, None).unwrap();
    let port = *app.qmp_ports.get(&running).unwrap();
    let pid = app.status_map.get(&running).and_then(|s| s.pid()).unwrap();
    drop(app);

    // The impostor claims the live machine's process and monitor.
    let runstate = dir.path().join("running.json");
    put(
        &runstate,
        record_for(impostor_id, pid, port, dir.path().join("impostor.log")),
    )
    .unwrap();

    let mut app = FaroQemApp::with_paths(settings, vms, None);
    let impostor_status = app.status_map.get(&impostor_id).cloned();
    let still_there = common::pid_is_alive(pid);
    let left_over = load(&runstate);
    let _ = app.force_stop_vm(running);
    let _ = common::kill_pid(pid);

    assert!(
        !matches!(impostor_status, Some(VmStatus::Running { .. })),
        "the wrong machine was adopted: got {:?}",
        impostor_status
    );
    assert!(
        !left_over.iter().any(|r| r.vm_id == impostor_id),
        "and its record is cleaned up"
    );
    assert!(still_there, "the real machine is left running");
}

#[test]
fn test_force_stopping_a_stale_record_never_kills_a_stranger() {
    // The review's critical finding: the identity was proven once, at
    // start-up, and never again. A reattached machine that ended on its own
    // leaves a record whose process number the operating system hands out
    // again -- and taskkill on that number hits whoever has it now.
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_plain_vm(&mut app);

    let mut stranger = common::spawn_long_sleep();
    let dead_port = faro_qem::qmp::free_local_port().unwrap(); // nothing listens
    app.running_processes.insert(
        id,
        faro_qem::app::Attached::Reattached { pid: stranger.id() },
    );
    app.qmp_ports.insert(id, dead_port);
    app.status_map
        .insert(id, VmStatus::Running { pid: stranger.id() });

    let _ = app.force_stop_vm(id);
    let survived = common::pid_is_alive(stranger.id());
    let _ = common::kill_pid(stranger.id());
    let _ = stranger.wait();

    assert!(
        survived,
        "the hard way must not reach a process whose identity was never confirmed"
    );
    assert_eq!(app.status_map.get(&id), Some(&VmStatus::Stopped));
    assert!(!app.is_attached(id), "and the machine counts as ended");
}

#[test]
fn test_shutting_down_a_stale_record_never_reaches_another_guest() {
    // Same hole, the other button: the recorded port can belong to a
    // different machine of ours by the time it is used.
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let running = common::push_startable_vm(&mut app, &root);
    let impostor = VmConfig::new_default("impostor", Arch::X86_64);
    let impostor_id = impostor.id;
    app.vms.push(impostor);
    app.launch_vm(running, None).unwrap();

    // The impostor's record points at the live machine's monitor.
    let port = *app.qmp_ports.get(&running).unwrap();
    let pid = app.status_map.get(&running).and_then(|s| s.pid()).unwrap();
    app.running_processes
        .insert(impostor_id, faro_qem::app::Attached::Reattached { pid });
    app.qmp_ports.insert(impostor_id, port);
    app.status_map
        .insert(impostor_id, VmStatus::Running { pid });

    let outcome = app.shutdown_vm(impostor_id);
    let real_status = app.status_map.get(&running).cloned();
    let _ = app.force_stop_vm(running);
    let _ = common::kill_pid(pid);

    assert!(
        outcome.is_err(),
        "a machine that cannot prove its identity takes no shutdown"
    );
    assert!(
        matches!(real_status, Some(VmStatus::Running { .. })),
        "and the machine that really owns that monitor is untouched, got: {:?}",
        real_status
    );
}

#[test]
fn test_a_reattached_guest_that_ended_on_its_own_shows_as_stopped() {
    // Without a check of its own, a reattached machine stays "running" for
    // the rest of the session: the start button does nothing and the only
    // way out is the hard one.
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_startable_vm(&mut app, &root);
    app.launch_vm(id, None).unwrap();
    let pid = app.status_map.get(&id).and_then(|s| s.pid()).unwrap();
    let port = *app.qmp_ports.get(&id).unwrap();

    // What the application looks like after a restart: the process is not
    // its child any more.
    app.running_processes
        .insert(id, faro_qem::app::Attached::Reattached { pid });
    app.qmp_ports.insert(id, port);
    app.status_map.insert(id, VmStatus::Running { pid });

    // The guest ends by itself -- the user shut it down from inside.
    let _ = common::kill_pid(pid);
    std::thread::sleep(std::time::Duration::from_millis(400));

    app.recheck_reattached();
    assert_eq!(
        app.status_map.get(&id),
        Some(&VmStatus::Stopped),
        "a machine whose monitor is gone counts as ended"
    );
    assert!(!app.is_attached(id));
    assert!(
        faro_qem::runstate::load(&app.runstate_path).is_empty(),
        "and its record is cleaned up"
    );
}

#[test]
fn test_a_record_whose_process_is_gone_is_cleaned_up() {
    let dir = tempdir().unwrap();
    let (settings, vms) = (
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
    );
    let vm = VmConfig::new_default("gone", Arch::X86_64);
    let id = vm.id;
    save_vms(&vms, &[vm]).unwrap();
    let runstate = dir.path().join("running.json");
    put(&runstate, record_for(id, 1, 1, dir.path().join("x.log"))).unwrap();

    let app = FaroQemApp::with_paths(settings, vms, None);
    assert!(!matches!(
        app.status_map.get(&id),
        Some(VmStatus::Running { .. })
    ));
    assert!(load(&runstate).is_empty());
}

#[test]
fn test_a_corrupt_machine_list_does_not_wipe_the_run_state() {
    // Part 1 took great care never to overwrite an unreadable vms.json.
    // Deleting the perfectly good running.json beside it would orphan every
    // running guest for good: the record is the only way back to them once
    // the configuration is repaired.
    let dir = tempdir().unwrap();
    let vms = dir.path().join("vms.json");
    std::fs::write(&vms, b"{ this is not a machine list").unwrap();
    let runstate = dir.path().join("running.json");
    let record = record_for(uuid::Uuid::new_v4(), 4242, 51234, dir.path().join("x.log"));
    put(&runstate, record.clone()).unwrap();

    let app = FaroQemApp::with_paths(dir.path().join("settings.json"), vms, None);
    assert!(
        !app.vms_loaded,
        "the test needs a list the app could not read"
    );
    assert_eq!(
        load(&runstate),
        vec![record],
        "the record has to survive a configuration the app could not read"
    );
}

#[test]
fn test_a_record_for_a_vm_that_no_longer_exists_is_dropped() {
    let dir = tempdir().unwrap();
    let runstate = dir.path().join("running.json");
    put(
        &runstate,
        record_for(uuid::Uuid::new_v4(), 1, 1, PathBuf::from("x")),
    )
    .unwrap();
    let _ = FaroQemApp::with_paths(
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
        None,
    );
    assert!(load(&runstate).is_empty());
}

// --------------------------------------------------------------------------
// Part 4, task 5: a log file per machine, and no lost ending
// --------------------------------------------------------------------------

#[test]
fn test_the_last_output_and_the_exit_are_not_lost_when_a_vm_stops() {
    // force_stop_vm dropped the receiving end along with the process, and
    // with it QEMU's last words and its exit event.
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_startable_vm(&mut app, &root);
    app.launch_vm(id, None).unwrap();
    app.force_stop_vm(id).unwrap();
    for _ in 0..40 {
        app.process_qemu_events(None);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let said = app
        .console_logs
        .get(&id)
        .map(|b| b.all_text())
        .unwrap_or_default();
    assert!(
        said.contains("[PROCESS]"),
        "the console must keep the ending, got:\n{}",
        said
    );
    assert!(
        said.to_lowercase().contains("exit"),
        "QEMU's exit is part of that ending, got:\n{}",
        said
    );
}

#[test]
fn test_a_started_machine_gets_a_log_file_of_its_own() {
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_startable_vm(&mut app, &root);
    let log = app.log_path_for(id);
    app.launch_vm(id, None).unwrap();
    let exists = log.is_file();
    let recorded = faro_qem::runstate::load(&app.runstate_path)
        .first()
        .map(|r| r.log.clone());
    let _ = app.force_stop_vm(id);
    assert!(
        exists,
        "there is no console to come back to without {:?}",
        log
    );
    assert_eq!(
        recorded,
        Some(log),
        "the record has to name the file the output actually went to"
    );
}

// --------------------------------------------------------------------------
// Part 4, task 4: closing the window is not a reason to kill a guest
// --------------------------------------------------------------------------

#[test]
fn test_dropping_the_app_leaves_the_guest_running() {
    // Spec section 7.3, the user's own decision. Drop used to kill every VM.
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let pid = {
        let mut app = common::app_with_vm_dir(&root);
        let id = common::push_startable_vm(&mut app, &root);
        app.launch_vm(id, None).unwrap();
        let pid = match app.status_map.get(&id) {
            Some(VmStatus::Running { pid }) => *pid,
            other => panic!("got {:?}", other),
        };
        drop(app);
        pid
    };
    std::thread::sleep(std::time::Duration::from_millis(800));
    let alive = common::pid_is_alive(pid);
    let _ = common::kill_pid(pid);
    assert!(
        alive,
        "the app closing is not a reason to kill a guest (pid {})",
        pid
    );
}

#[test]
fn test_dropping_the_app_still_saves_the_configuration() {
    let dir = tempdir().unwrap();
    let vms_path = dir.path().join("vms.json");
    {
        let mut app =
            FaroQemApp::with_paths(dir.path().join("settings.json"), vms_path.clone(), None);
        app.vms.push(VmConfig::new_default("kept", Arch::X86_64));
    }
    assert!(try_load_vms(&vms_path)
        .unwrap()
        .iter()
        .any(|v| v.name == "kept"));
}

#[test]
fn test_the_record_outlives_the_app() {
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let runstate = {
        let mut app = common::app_with_vm_dir(&root);
        let id = common::push_startable_vm(&mut app, &root);
        app.launch_vm(id, None).unwrap();
        let path = app.runstate_path.clone();
        drop(app);
        path
    };
    let records = faro_qem::runstate::load(&runstate);
    let killed: Vec<u32> = records.iter().map(|r| r.pid).collect();
    for pid in &killed {
        let _ = common::kill_pid(*pid);
    }
    assert_eq!(
        records.len(),
        1,
        "without this the VM cannot be found again"
    );
}

#[test]
fn test_closing_with_running_vms_names_them() {
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_startable_vm(&mut app, &root);
    app.launch_vm(id, None).unwrap();
    let names = app.surviving_vm_names();
    let _ = app.force_stop_vm(id);
    assert_eq!(
        names.len(),
        1,
        "say which guests keep running -- without asking, got: {:?}",
        names
    );
    assert_eq!(names[0], "startable");
}

#[test]
fn test_a_stopped_machine_is_not_named_as_surviving() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("vms");
    let app = common::app_with_vm_dir(&root);
    assert!(
        app.surviving_vm_names().is_empty(),
        "nothing is running, so nothing survives the window"
    );
}

#[test]
fn test_force_stop_is_the_second_step_and_it_works() {
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    // A subdirectory, so the app's own files land inside the temporary
    // directory rather than beside it -- see app_with_vm_dir.
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_startable_vm(&mut app, &root);
    app.launch_vm(id, None).unwrap();
    app.force_stop_vm(id).unwrap();
    assert!(matches!(app.status_map.get(&id), Some(VmStatus::Stopped)));
    assert!(!app.is_attached(id));
}

// --------------------------------------------------------------------------
// Nothing may be laid out past the right edge of the window
// --------------------------------------------------------------------------

#[test]
fn test_every_control_is_reachable_at_the_default_window_size() {
    // egui does not paint what lies outside the window, so a control laid
    // out past the edge cannot be clicked and leaves no trace at all --
    // which is why this asks for the labels rather than for an area. The
    // German labels are the longest of the three, and the window may be
    // made as small as the floor in main.rs.
    let dir = tempdir().unwrap();
    // With a context, so the application installs its own fonts: the
    // default ones are narrower, and a header that fits in them can still
    // overflow in the fonts the user actually sees.
    let font_ctx = Context::default();
    let mut app = FaroQemApp::with_paths(
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
        Some(&font_ctx),
    );
    let id = common::push_plain_vm(&mut app);
    app.selected_vm_id = Some(id);

    // The smallest window the application allows, and the one it opens
    // with. Below the floor the content is laid out past the right edge,
    // where egui paints nothing at all -- which is why the floor exists.
    for width in [960.0_f32, 1120.0, 1400.0] {
        for lang in ["en", "de", "fr"] {
            app.i18n.set_locale(lang);
            let ctx = font_ctx.clone();
            let input = || egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 745.0),
                )),
                ..Default::default()
            };
            let _ = ctx.run(input(), |ctx| app.render_ui(ctx));
            let out = ctx.run(input(), |ctx| app.render_ui(ctx));
            let drawn = common::rendered_text(&out);

            for key in [
                "launch",
                "shutdown",
                "save",
                "tab_system",
                "tab_storage",
                "tab_display",
                "tab_network",
                "tab_console",
                "settings",
            ] {
                let label = app.i18n.t(key);
                assert!(
                    drawn.contains(label),
                    "at {} points in {}: '{}' ({}) is nowhere on screen.
drew:
{}",
                    width,
                    lang,
                    label,
                    key,
                    drawn
                );
            }
        }
    }
}
