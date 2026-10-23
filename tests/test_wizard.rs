// FARO QEMU MANAGER (C) R.Trunk 2026
// The template wizard, variant C: gallery, two always-visible fields, two
// collapsed blocks that show their core values, a notice area.

mod common;

use egui::Context;

use faro_qem::config::Arch;
use faro_qem::i18n::I18n;
use faro_qem::qemu::QemuCapabilities;
use faro_qem::templates::VmTemplate;
use faro_qem::ui::wizard::{WizardDialog, WizardState};

/// Everything the dialog draws, after a layout pass.
fn render_wizard(state: &mut WizardState) -> String {
    let i18n = I18n::new("en");
    let caps = QemuCapabilities::default();
    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        WizardDialog::new(state, &i18n)
            .with_capabilities(&caps)
            .show(ctx);
    });
    let output = ctx.run(Default::default(), |ctx| {
        WizardDialog::new(state, &i18n)
            .with_capabilities(&caps)
            .show(ctx);
    });
    common::rendered_text(&output)
}

fn opened() -> WizardState {
    WizardState {
        open: true,
        ..Default::default()
    }
}

#[test]
fn test_the_gallery_shows_all_six_templates_and_the_empty_tile() {
    let i18n = I18n::new("en");
    let drawn = render_wizard(&mut opened());
    for t in VmTemplate::all() {
        assert!(
            drawn.contains(t.name),
            "missing tile {}, drew:\n{}",
            t.name,
            drawn
        );
    }
    assert!(
        drawn.contains(i18n.t("wizard_empty_tile")),
        "drew:\n{}",
        drawn
    );
}

#[test]
fn test_name_and_iso_are_the_only_always_visible_fields() {
    let i18n = I18n::new("en");
    let mut state = opened();
    state.select("debian");
    let drawn = render_wizard(&mut state);
    assert!(drawn.contains(i18n.t("vm_name")), "drew:\n{}", drawn);
    assert!(drawn.contains(i18n.t("iso_path")), "drew:\n{}", drawn);
    assert!(drawn.contains(i18n.t("wizard_hardware_block")));
    assert!(drawn.contains(i18n.t("wizard_network_block")));
    assert!(
        !drawn.contains(i18n.t("accel")),
        "a collapsed block must not draw its fields, drew:\n{}",
        drawn
    );
}

#[test]
fn test_a_collapsed_block_shows_its_core_values_as_a_line() {
    // Spec 5.4: one sees what is set without having to click.
    let mut state = opened();
    state.select("windows-11");
    let drawn = render_wizard(&mut state);
    for needle in ["4", "8192", "64", "UEFI", "sata"] {
        assert!(
            drawn.to_lowercase().contains(&needle.to_lowercase()),
            "missing {}, drew:\n{}",
            needle,
            drawn
        );
    }
}

#[test]
fn test_opening_a_block_shows_its_fields() {
    let i18n = I18n::new("en");
    let mut state = opened();
    state.select("debian");
    state.hardware_open = true;
    let drawn = render_wizard(&mut state);
    assert!(drawn.contains(i18n.t("accel")), "drew:\n{}", drawn);
}

#[test]
fn test_choosing_windows_shows_the_tpm_hint() {
    let i18n = I18n::new("en");
    let mut state = opened();
    state.select("windows-11");
    let drawn = render_wizard(&mut state);
    assert!(
        drawn.contains(i18n.t("template_notice_windows")),
        "drew:\n{}",
        drawn
    );
}

#[test]
fn test_a_linux_template_shows_no_windows_hint() {
    let i18n = I18n::new("en");
    let mut state = opened();
    state.select("alpine");
    let drawn = render_wizard(&mut state);
    assert!(!drawn.contains(i18n.t("template_notice_windows")));
}

#[test]
fn test_choosing_a_template_fills_the_draft_and_another_replaces_it() {
    let mut state = opened();
    state.select("windows-11");
    assert_eq!(state.draft.as_ref().unwrap().memory_mb, 8192);
    state.select("alpine");
    assert_eq!(state.draft.as_ref().unwrap().memory_mb, 1024);
}

#[test]
fn test_an_edit_survives_until_a_template_is_chosen_again() {
    // A template is a starting assignment: editing is allowed, and picking a
    // template starts over rather than merging two assignments.
    let mut state = opened();
    state.select("debian");
    state.draft.as_mut().unwrap().memory_mb = 777;
    assert_eq!(state.draft.as_ref().unwrap().memory_mb, 777);
    state.select("debian");
    assert_eq!(state.draft.as_ref().unwrap().memory_mb, 4096);
}

#[test]
fn test_create_is_refused_without_a_name_and_without_a_choice() {
    let mut state = opened();
    assert!(state.why_not_create().is_some(), "nothing is chosen yet");
    state.select("debian");
    state.name = "  ".to_string();
    assert!(
        state.why_not_create().is_some(),
        "a VM without a name cannot get a folder"
    );
    state.name = "Debian 13".to_string();
    assert_eq!(state.why_not_create(), None);
}

#[test]
fn test_the_reason_create_is_blocked_is_shown_next_to_the_button() {
    let i18n = I18n::new("en");
    let mut state = opened();
    let drawn = render_wizard(&mut state);
    assert!(
        drawn.contains(i18n.t("wizard_needs_template")),
        "a disabled button without a reason is a dead end, drew:\n{}",
        drawn
    );
}

#[test]
fn test_the_empty_tile_promises_nothing() {
    let mut state = WizardState {
        open: true,
        name: "Blank".to_string(),
        ..Default::default()
    };
    state.select_empty(Arch::X86_64);
    let draft = state.draft.as_ref().unwrap();
    assert_eq!(draft.template, None);
    assert!(draft.drives.is_empty(), "the empty tile promises nothing");
    assert_eq!(state.why_not_create(), None);
}

#[test]
fn test_the_name_reaches_the_draft_when_create_fires() {
    // The draft is built when a tile is clicked, which is usually before the
    // name is typed. Whatever stands in the field at the end is the name.
    let mut state = opened();
    state.select("debian");
    state.name = "Mein Debian".to_string();
    let vm = state.take_draft().expect("there is a draft");
    assert_eq!(vm.name, "Mein Debian");
}

#[test]
fn test_the_wizard_texts_are_in_all_three_catalogues() {
    for lang in ["en", "de", "fr"] {
        let keys = common::catalogue_keys(lang);
        for key in [
            "wizard_title",
            "wizard_empty_tile",
            "wizard_hardware_block",
            "wizard_network_block",
            "wizard_needs_template",
            "wizard_needs_name",
        ] {
            assert!(keys.contains(key), "{} missing in {}", key, lang);
        }
    }
}
