// FARO QEMU MANAGER (C) R.Trunk 2026
// Application behaviour when the stored configuration cannot be read

use egui::Context;
use faro_qem::app::FaroQemApp;
use faro_qem::i18n::I18n;
use faro_qem::ui::notice::{Notice, NoticeBar, NoticeLevel};

/// Names of entries in `dir` that look like a quarantined configuration.
fn quarantined_entries(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains(".corrupt-"))
        .collect()
}

#[test]
fn test_notice_bar_renders_and_dismisses() {
    let i18n = I18n::new("en");
    let mut notices = vec![
        Notice::error("something broke"),
        Notice::warning("something is odd"),
        Notice::info("something happened"),
    ];
    assert_eq!(notices[0].level, NoticeLevel::Error);

    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            NoticeBar::new(&mut notices).show(ui, &i18n);
        });
    });
    assert_eq!(notices.len(), 3, "rendering alone dismisses nothing");

    notices.clear();
    let ctx2 = Context::default();
    let _ = ctx2.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            NoticeBar::new(&mut notices).show(ui, &i18n);
        });
    });
}

/// The regression for the data loss proven in spec section 3.4: a single
/// missing comma used to cost every VM the user had configured.
#[test]
fn test_corrupt_vms_json_is_preserved_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let vms_path = dir.path().join("vms.json");
    std::fs::write(
        &vms_path,
        r#"[{"id":"550e8400-e29b-41d4-a716-446655440000","name":"WICHTIGE VM","arch":"x86_64","cpu_cores":8,"memory_mb":16384},{"#,
    )
    .unwrap();

    let app = FaroQemApp::with_paths(dir.path().join("settings.json"), vms_path.clone(), None);

    assert!(
        app.vms.is_empty(),
        "a corrupt file must not be replaced by a default VM"
    );
    assert!(app.notices.iter().any(|n| n.level == NoticeLevel::Error));

    let kept = quarantined_entries(dir.path());
    assert_eq!(kept.len(), 1, "exactly one quarantined copy");
    assert!(
        std::fs::read_to_string(&kept[0])
            .unwrap()
            .contains("WICHTIGE VM"),
        "the original bytes survived"
    );

    let notice = app
        .notices
        .iter()
        .find(|n| n.level == NoticeLevel::Error)
        .unwrap();
    assert!(
        notice.text.contains(".corrupt-"),
        "the notice names where the file went: {}",
        notice.text
    );
}

#[test]
fn test_missing_vms_json_still_creates_the_default_vm() {
    let dir = tempfile::tempdir().unwrap();
    let app = FaroQemApp::with_paths(
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
        None,
    );

    assert_eq!(app.vms.len(), 1);
    assert_eq!(app.vms[0].name, "Debian 12");
    assert!(app.notices.is_empty(), "a first start is not a problem");
}

#[test]
fn test_corrupt_settings_json_is_preserved_and_defaults_are_used() {
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    std::fs::write(&settings_path, r#"{"language":"de","theme":"#).unwrap();

    let app = FaroQemApp::with_paths(settings_path.clone(), dir.path().join("vms.json"), None);

    assert_eq!(app.settings.language, "en", "falls back to defaults");
    assert!(app.notices.iter().any(|n| n.level == NoticeLevel::Error));

    let kept = quarantined_entries(dir.path());
    assert_eq!(kept.len(), 1, "the unreadable settings file was kept");
    assert!(std::fs::read_to_string(&kept[0])
        .unwrap()
        .contains("\"language\":\"de\""));
}

// Review Focus 4: the configuration path holds something that cannot be read
// as JSON at all. Whether it can be set aside or not, the user's bytes must
// survive and the failure must be reported.
#[test]
fn test_unreadable_config_path_is_reported_and_never_destroyed() {
    let dir = tempfile::tempdir().unwrap();
    let vms_path = dir.path().join("vms.json");
    std::fs::create_dir(&vms_path).unwrap();
    std::fs::write(vms_path.join("marker.txt"), "USER DATA").unwrap();

    let app = FaroQemApp::with_paths(dir.path().join("settings.json"), vms_path.clone(), None);

    assert!(
        app.vms.is_empty(),
        "an unreadable path must not be replaced by a default VM"
    );
    assert!(
        app.notices.iter().any(|n| n.level == NoticeLevel::Error),
        "the failure is reported rather than swallowed"
    );

    // Either it is still in place, or it was set aside — never destroyed.
    let surviving: Vec<std::path::PathBuf> = if vms_path.exists() {
        vec![vms_path.clone()]
    } else {
        quarantined_entries(dir.path())
    };
    assert_eq!(surviving.len(), 1, "the original survived somewhere");
    assert_eq!(
        std::fs::read_to_string(surviving[0].join("marker.txt")).unwrap(),
        "USER DATA"
    );
}

#[test]
fn test_failing_save_is_reported_to_the_user() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = FaroQemApp::with_paths(
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
        None,
    );
    app.notices.clear();

    // Make the target unwritable by putting a directory in its place.
    let blocked = dir.path().join("blocked").join("vms.json");
    std::fs::create_dir_all(&blocked).unwrap();
    app.vms_path = blocked;

    app.handle_sidebar_action(faro_qem::ui::sidebar::SidebarAction::NewVm, None);

    assert!(
        app.notices.iter().any(|n| n.level == NoticeLevel::Error),
        "a save that failed must not be silent"
    );
}

/// All text egui actually drew this frame.
fn rendered_text(output: &egui::FullOutput) -> String {
    fn walk(shape: &egui::Shape, out: &mut String) {
        match shape {
            egui::Shape::Text(t) => {
                out.push_str(t.galley.text());
                out.push('\n');
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    walk(s, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut out);
    }
    out
}

#[test]
fn test_render_ui_puts_the_notice_on_screen() {
    let dir = tempfile::tempdir().unwrap();
    let vms_path = dir.path().join("vms.json");
    std::fs::write(&vms_path, "{broken").unwrap();

    let mut app = FaroQemApp::with_paths(dir.path().join("settings.json"), vms_path, None);
    assert!(!app.notices.is_empty(), "precondition: a notice exists");

    let ctx = Context::default();
    // First frame warms up fonts and layout; read the second.
    let _ = ctx.run(Default::default(), |ctx| app.render_ui(ctx));
    let output = ctx.run(Default::default(), |ctx| app.render_ui(ctx));

    let drawn = rendered_text(&output);
    assert!(
        drawn.contains(".corrupt-"),
        "the notice naming the quarantined file must be visible, drew:\n{}",
        drawn
    );
    assert!(
        !app.notices.is_empty(),
        "a notice stays until it is dismissed"
    );
}

#[test]
fn test_render_ui_without_notices_draws_no_band() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = FaroQemApp::with_paths(
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
        None,
    );
    assert!(app.notices.is_empty(), "precondition: nothing to report");

    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| app.render_ui(ctx));
    let output = ctx.run(Default::default(), |ctx| app.render_ui(ctx));

    let i18n = I18n::new("en");
    assert!(
        !rendered_text(&output).contains(i18n.t("notice_dismiss")),
        "an empty band must not take up room"
    );
}
