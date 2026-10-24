// FARO QEMU MANAGER (C) R.Trunk 2026
// Unit tests for Sidebar and Settings Dialog components

mod common;

use egui::Context;
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

use faro_qem::config::{AppSettings, Arch, VmConfig};
use faro_qem::i18n::I18n;
use faro_qem::ui::{SettingsDialog, SidebarAction, SidebarView, VmStatus};

#[test]
fn test_vm_status_helpers_and_display() {
    let stopped = VmStatus::Stopped;
    assert!(!stopped.is_running());
    assert_eq!(stopped.pid(), None);
    assert!(!stopped.is_error());
    assert_eq!(stopped.to_string(), "Stopped");

    let running = VmStatus::Running { pid: 4242 };
    assert!(running.is_running());
    assert_eq!(running.pid(), Some(4242));
    assert!(!running.is_error());
    assert_eq!(running.to_string(), "Running (PID: 4242)");

    let error = VmStatus::Error("Device I/O error".to_string());
    assert!(!error.is_running());
    assert_eq!(error.pid(), None);
    assert!(error.is_error());
    assert_eq!(error.to_string(), "Error: Device I/O error");
}

#[test]
fn test_sidebar_layout_geometry() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let vm = VmConfig::new_default("Debian 12", Arch::X86_64);
    let vms = vec![vm.clone()];
    let statuses = HashMap::new();
    let mut search = String::new();

    let raw_input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1120.0, 720.0),
        )),
        ..Default::default()
    };

    let _ = ctx.run(raw_input, |ctx| {
        egui::SidePanel::left("sidebar_panel")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                let view = SidebarView::new(&vms, Some(vm.id), &mut search, &statuses, &i18n);
                let _ = view.show(ui);
                // Confirm the sidebar does not overflow below window height (720.0)
                assert!(ui.min_rect().bottom() <= 720.0);
            });
    });
}

#[test]
fn test_sidebar_empty_list_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let vms: Vec<VmConfig> = Vec::new();
    let mut search = String::new();
    let statuses = HashMap::new();

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = SidebarView::new(&vms, None, &mut search, &statuses, &i18n).show(ui);
            assert_eq!(action, None);
        });
    });
}

#[test]
fn test_sidebar_with_vms_and_statuses_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");

    let mut vm1 = VmConfig::new_default("Debian 12 Bookworm", Arch::X86_64);
    vm1.cpu_cores = 4;
    vm1.memory_mb = 4096;

    let mut vm2 = VmConfig::new_default("Ubuntu ARM64", Arch::Aarch64);
    vm2.cpu_cores = 2;
    vm2.memory_mb = 2048;

    let vm3 = VmConfig::new_default("FreeBSD RISC-V", Arch::Riscv64);

    let vms = vec![vm1.clone(), vm2.clone(), vm3.clone()];

    let mut statuses = HashMap::new();
    statuses.insert(vm1.id, VmStatus::Running { pid: 9876 });
    statuses.insert(vm2.id, VmStatus::Stopped);
    statuses.insert(vm3.id, VmStatus::Error("Kernel panic".to_string()));

    let mut search = String::new();
    let mut confirm_del = None;

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let action = SidebarView::new(&vms, Some(vm1.id), &mut search, &statuses, &i18n)
                .with_delete_confirmation(&mut confirm_del)
                .show(ui);
            assert_eq!(action, None);
        });
    });
}

#[test]
fn test_sidebar_search_filtering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");

    let vm1 = VmConfig::new_default("Debian Server", Arch::X86_64);
    let vm2 = VmConfig::new_default("Windows 11 Workstation", Arch::X86_64);
    let vm3 = VmConfig::new_default("Alpine ARM", Arch::Aarch64);
    let vms = vec![vm1.clone(), vm2.clone(), vm3.clone()];
    let statuses = HashMap::new();

    // 1. Search by name substring
    let mut search = "debian".to_string();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let view = SidebarView::new(&vms, None, &mut search, &statuses, &i18n);
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    // 2. Search by architecture string
    search = "aarch64".to_string();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let view = SidebarView::new(&vms, None, &mut search, &statuses, &i18n);
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    // 3. Search query with no match
    search = "nonexistent_vm_query".to_string();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let view = SidebarView::new(&vms, None, &mut search, &statuses, &i18n);
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });
}

#[test]
fn test_sidebar_delete_confirmation_modal_rendering() {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let vm = VmConfig::new_default("Test VM to Delete", Arch::X86_64);
    let vms = vec![vm.clone()];
    let statuses = HashMap::new();
    let mut search = String::new();
    let mut confirm_del = Some(vm.id);

    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let view = SidebarView::new(&vms, Some(vm.id), &mut search, &statuses, &i18n)
                .with_delete_confirmation(&mut confirm_del);
            let action = view.show(ui);
            assert_eq!(action, None);
        });
    });

    // Confirm id remains until a button inside modal is clicked
    assert_eq!(confirm_del, Some(vm.id));
}

#[test]
fn test_sidebar_actions_variants() {
    let id = Uuid::new_v4();
    let sel = SidebarAction::SelectVm(id);
    let new_vm = SidebarAction::NewVm;
    let dup = SidebarAction::DuplicateVm(id);
    let del = SidebarAction::DeleteVm {
        id,
        with_folder: false,
    };
    let settings = SidebarAction::OpenSettings;

    assert_eq!(sel, SidebarAction::SelectVm(id));
    assert_eq!(new_vm, SidebarAction::NewVm);
    assert_eq!(dup, SidebarAction::DuplicateVm(id));
    assert_eq!(
        del,
        SidebarAction::DeleteVm {
            id,
            with_folder: false
        }
    );
    assert_eq!(settings, SidebarAction::OpenSettings);
    assert_ne!(new_vm, settings);
}

#[test]
fn test_settings_dialog_closed_does_not_render() {
    let ctx = Context::default();
    let mut i18n = I18n::new("en");
    let mut settings = AppSettings::default();
    let mut open = false;

    let modified = SettingsDialog::new(&mut open, &mut settings, &mut i18n).show(&ctx);
    assert!(!modified);
    assert!(!open);
}

#[test]
fn test_settings_dialog_window_growth() {
    let ctx = Context::default();
    let mut i18n = I18n::new("en");
    let mut settings = AppSettings::default();
    let mut open = true;

    let mut heights = Vec::new();
    let mut widths = Vec::new();

    for _frame in 0..10 {
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1120.0, 720.0),
            )),
            ..Default::default()
        };

        let _ = ctx.run(raw_input, |ctx| {
            let mut dialog = SettingsDialog::new(&mut open, &mut settings, &mut i18n);
            dialog.show(ctx);
        });

        let bb = ctx.used_rect();
        widths.push(bb.width());
        heights.push(bb.height());
    }

    // Verify window width and height are stable and do not continuously grow across frames
    if widths.len() >= 2 {
        let first_w = widths[1];
        let last_w = *widths.last().unwrap();
        assert!(
            (last_w - first_w).abs() < 5.0,
            "Settings window width grew from {} to {}",
            first_w,
            last_w
        );

        let first_h = heights[1];
        let last_h = *heights.last().unwrap();
        assert!(
            (last_h - first_h).abs() < 5.0,
            "Settings window height grew from {} to {}",
            first_h,
            last_h
        );
    }
}
#[test]
fn test_settings_dialog_open_rendering() {
    let ctx = Context::default();
    let mut i18n = I18n::new("en");
    let mut settings = AppSettings {
        qemu_dir: Some(PathBuf::from("/test/qemu")),
        language: "en".to_string(),
        theme: "dark".to_string(),
        default_arch: Arch::X86_64,
        vm_dir: None,
        creation_mode: Default::default(),
        schema: None,
    };
    let mut open = true;

    let _ = ctx.run(Default::default(), |ctx| {
        let mut dialog = SettingsDialog::new(&mut open, &mut settings, &mut i18n);
        let _ = dialog.show(ctx);
    });

    assert!(open);
    assert_eq!(settings.language, "en");
    assert_eq!(settings.theme, "dark");
}

#[test]
fn test_settings_dialog_language_change() {
    let mut i18n = I18n::new("en");
    let mut settings = AppSettings::default();
    assert_eq!(settings.language, "en");
    assert_eq!(i18n.current_locale(), "en");

    // Change language to German
    settings.language = "de".to_string();
    i18n.set_locale("de");

    assert_eq!(settings.language, "de");
    assert_eq!(i18n.current_locale(), "de");
    assert_eq!(i18n.t("settings"), "Einstellungen");
    assert_eq!(i18n.t("new_vm"), "Neue VM");

    // Change language to French
    settings.language = "fr".to_string();
    i18n.set_locale("fr");

    assert_eq!(settings.language, "fr");
    assert_eq!(i18n.current_locale(), "fr");
    assert_eq!(i18n.t("settings"), "Paramètres");
    assert_eq!(i18n.t("new_vm"), "Nouvelle MV");
}

#[test]
fn test_settings_dialog_theme_change() {
    let ctx = Context::default();
    let mut settings = AppSettings::default();

    // Default theme is dark
    assert!(settings.theme == "dark");

    // Switch to light theme
    settings.theme = "light".to_string();
    faro_qem::ui::apply_theme(&ctx, &settings.theme);
    assert!(!ctx.style().visuals.dark_mode);

    // Switch back to dark theme
    settings.theme = "dark".to_string();
    faro_qem::ui::apply_theme(&ctx, &settings.theme);
    assert!(ctx.style().visuals.dark_mode);
}

#[test]
fn test_settings_dialog_qemu_dir_modification() {
    let mut settings = AppSettings::default();
    assert_eq!(settings.qemu_dir, None);

    let custom_path = PathBuf::from(r"C:\Custom\QEMU");
    settings.qemu_dir = Some(custom_path.clone());
    assert_eq!(settings.qemu_dir, Some(custom_path));

    // Clear path
    settings.qemu_dir = None;
    assert_eq!(settings.qemu_dir, None);
}

#[test]
fn test_the_settings_dialog_offers_the_vm_directory_and_the_creation_mode() {
    let i18n = I18n::new("en");
    let mut settings = AppSettings::default();
    let mut dialog_i18n = I18n::new("en");
    let drawn = common::rendered_settings(&mut settings, &mut dialog_i18n);
    assert!(drawn.contains(i18n.t("vm_dir")), "drew:\n{}", drawn);
    assert!(drawn.contains(i18n.t("creation_mode")), "drew:\n{}", drawn);
    assert!(
        drawn.contains(i18n.t("creation_mode_wizard")),
        "drew:\n{}",
        drawn
    );
    assert!(
        drawn.contains(i18n.t("creation_mode_direct")),
        "drew:\n{}",
        drawn
    );
}

#[test]
fn test_the_creation_mode_texts_are_in_all_three_catalogues() {
    for lang in ["en", "de", "fr"] {
        let keys = common::catalogue_keys(lang);
        for key in [
            "vm_dir",
            "creation_mode",
            "creation_mode_wizard",
            "creation_mode_direct",
        ] {
            assert!(keys.contains(key), "{} missing in {}", key, lang);
        }
    }
}

// --------------------------------------------------------------------------
// The delete dialog's consent box: the one irreversible thing in the app
// --------------------------------------------------------------------------

/// Everything the sidebar draws with the delete dialog open for `vm`.
fn rendered_delete_dialog(
    vm: &VmConfig,
    folder: Option<&std::path::Path>,
    folder_for: Option<Uuid>,
    consent: &mut bool,
) -> String {
    let ctx = Context::default();
    let i18n = I18n::new("en");
    let vms = vec![vm.clone()];
    let statuses = HashMap::new();
    let mut search = String::new();
    let mut confirm = Some(vm.id);

    // Twice: egui needs a layout pass before text is placed.
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            SidebarView::new(&vms, Some(vm.id), &mut search, &statuses, &i18n)
                .with_delete_confirmation(&mut confirm)
                .with_delete_folder(consent, folder, folder_for)
                .show(ui);
        });
    });
    let output = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            SidebarView::new(&vms, Some(vm.id), &mut search, &statuses, &i18n)
                .with_delete_confirmation(&mut confirm)
                .with_delete_folder(consent, folder, folder_for)
                .show(ui);
        });
    });
    common::rendered_text(&output)
}

#[test]
fn test_the_delete_dialog_names_the_folder_it_would_take() {
    let vm = VmConfig::new_default("Debian", Arch::X86_64);
    let folder = PathBuf::from(r"C:\Users\someone\FARO-VMs\Debian");
    let mut consent = false;

    let drawn = rendered_delete_dialog(&vm, Some(&folder), Some(vm.id), &mut consent);

    assert!(
        drawn.contains("Debian"),
        "the path belongs in the text, got:\n{}",
        drawn
    );
    assert!(
        drawn.contains("FARO-VMs"),
        "the whole path, not just the leaf, got:\n{}",
        drawn
    );
}

#[test]
fn test_the_delete_dialog_offers_no_box_when_there_is_no_folder() {
    // No folder of its own, or an ambiguous one: nothing to consent to.
    let vm = VmConfig::new_default("hand-made", Arch::X86_64);
    let i18n = I18n::new("en");
    let mut consent = false;

    let drawn = rendered_delete_dialog(&vm, None, Some(vm.id), &mut consent);

    let label = i18n
        .t("delete_take_folder")
        .split("{path}")
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    assert!(!label.is_empty());
    assert!(
        !drawn.contains(&label),
        "a box with nothing behind it must not be drawn, got:\n{}",
        drawn
    );
}

#[test]
fn test_a_folder_worked_out_for_another_machine_is_not_offered() {
    // The frame the dialog opens on a second machine: the caller has not
    // reset the tick yet, and a stale consent must not reach delete_vm.
    let vm = VmConfig::new_default("Debian", Arch::X86_64);
    let other = Uuid::new_v4();
    let folder = PathBuf::from(r"C:\Users\someone\FARO-VMs\SomethingElse");
    let mut consent = true;

    let drawn = rendered_delete_dialog(&vm, Some(&folder), Some(other), &mut consent);

    assert!(
        !drawn.contains("SomethingElse"),
        "that folder belongs to another machine, got:\n{}",
        drawn
    );
}

// ---------------------------------------------------------------------------
// Part 4, task 7: the dialog shows what the application resolved
// ---------------------------------------------------------------------------

#[test]
fn test_the_settings_dialog_names_the_emulator_it_was_handed() {
    // The dialog used to search the file system on every frame it was open.
    // It now shows what the application resolved -- and has to actually
    // show it, or the user cannot tell whether QEMU was found.
    let mut settings = AppSettings::default();
    let mut i18n = I18n::new("en");
    let path = std::path::PathBuf::from("C:/somewhere/qemu-system-x86_64.exe");

    let drawn = common::rendered_settings_with_qemu(&mut settings, &mut i18n, Some(&path));
    assert!(
        drawn.contains("qemu-system-x86_64"),
        "the resolved emulator has to appear, drew:
{}",
        drawn
    );

    let drawn = common::rendered_settings_with_qemu(&mut settings, &mut i18n, None);
    assert!(
        drawn.contains(i18n.t("qemu_not_found")),
        "and its absence has to be said, drew:
{}",
        drawn
    );
}

#[test]
fn test_the_settings_window_does_not_grow_with_a_long_path_in_it() {
    // Reported from the running application: the window grows by itself
    // until it is too large. The existing growth test runs with no QEMU
    // found and no directories set, which is the one state where the
    // content is narrow enough to hide the feedback loop.
    let ctx = Context::default();
    let mut i18n = I18n::new("de");
    let mut settings = AppSettings {
        qemu_dir: Some(PathBuf::from(r"C:\Program Files\qemu")),
        vm_dir: Some(PathBuf::from(
            r"C:\Users\someone\Documents\FARO-VMs\a-rather-long-folder-name",
        )),
        ..Default::default()
    };
    let detected = PathBuf::from(r"C:\Program Files\qemu\qemu-system-x86_64.exe");
    let mut open = true;
    let mut edited = false;

    let mut widths = Vec::new();
    for _frame in 0..10 {
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1120.0, 720.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(raw_input, |ctx| {
            SettingsDialog::new(&mut open, &mut settings, &mut i18n)
                .with_detected_qemu(Some(&detected))
                .editing_qemu_dir(&mut edited)
                .show(ctx);
        });
        widths.push(ctx.used_rect().width());
    }

    let settled = widths[1];
    let last = *widths.last().unwrap();
    assert!(
        (last - settled).abs() < 5.0,
        "the window grew from {} to {} over ten frames: {:?}",
        settled,
        last,
        widths
    );
}
