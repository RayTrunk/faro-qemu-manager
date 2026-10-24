// FARO QEMU MANAGER (C) R.Trunk 2026
// The template wizard, variant C: gallery, two always-visible fields, two
// collapsed blocks that show their core values, a notice area.

mod common;

use egui::Context;

use faro_qem::config::{Arch, Firmware};
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
    WizardState::opened()
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
    let mut state = WizardState::opened();
    state.name = "Blank".to_string();
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

/// Everything the dialog draws in a window of this size.
fn render_wizard_in(state: &mut WizardState, width: f32, height: f32) -> String {
    let i18n = I18n::new("en");
    let caps = QemuCapabilities::default();
    let ctx = Context::default();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width, height),
        )),
        ..Default::default()
    };
    let _ = ctx.run(input(), |ctx| {
        WizardDialog::new(state, &i18n)
            .with_capabilities(&caps)
            .show(ctx);
    });
    let output = ctx.run(input(), |ctx| {
        WizardDialog::new(state, &i18n)
            .with_capabilities(&caps)
            .show(ctx);
    });
    common::rendered_text(&output)
}

#[test]
fn test_the_create_button_stays_reachable_in_a_small_window() {
    // The dialog is anchored and not resizable, so content taller than the
    // viewport cannot be reached by dragging or stretching -- and its height
    // is the user's to set, through the two blocks. Windows 11 is the tallest
    // case: it carries a notice paragraph as well.
    let i18n = I18n::new("en");
    let mut state = opened();
    state.select("windows-11");
    state.name = "Win 11".to_string();
    state.hardware_open = true;
    state.network_open = true;

    let drawn = render_wizard_in(&mut state, 760.0, 300.0);

    assert!(
        drawn.contains(i18n.t("create")),
        "the Create button is off the screen with no way to reach it; drew:\n{}",
        drawn
    );
    assert!(
        drawn.contains(i18n.t("cancel")),
        "and so is Cancel; drew:\n{}",
        drawn
    );
}

#[test]
fn test_the_hardware_block_lets_the_firmware_be_changed() {
    // Spec 5.4: "Aufgeklappt sind alle Werte aenderbar." The block is called
    // "Hardware and firmware" and its collapsed header prints UEFI or BIOS,
    // so a user who opens it is looking for that switch. Without it there is
    // no BIOS Debian and no UEFI Alpine from the wizard.
    let i18n = I18n::new("en");
    let mut state = opened();
    state.select("debian");
    state.hardware_open = true;
    let drawn = render_wizard(&mut state);
    assert!(
        drawn.contains(i18n.t("firmware_bios")) && drawn.contains(i18n.t("firmware_uefi")),
        "the block names firmware in its title; drew:
{}",
        drawn
    );
}

#[test]
fn test_the_firmware_switch_changes_the_draft() {
    let mut state = opened();
    state.select("alpine");
    assert!(
        matches!(
            state.draft.as_ref().unwrap().firmware,
            Firmware::Bios { .. }
        ),
        "alpine starts as BIOS"
    );
    state.set_uefi(true);
    assert!(
        matches!(
            state.draft.as_ref().unwrap().firmware,
            Firmware::Uefi { .. }
        ),
        "and can be switched"
    );
    state.set_uefi(false);
    assert!(matches!(
        state.draft.as_ref().unwrap().firmware,
        Firmware::Bios { .. }
    ));
}

#[test]
fn test_the_network_block_lets_a_port_forward_be_added() {
    // The block is called "Network and port forwarding" and its header even
    // prints a rule count -- which could never be anything but zero, because
    // nothing in the wizard could add a rule.
    let i18n = I18n::new("en");
    let mut state = opened();
    state.select("debian");
    state.network_open = true;

    // With no rule yet, the block must at least offer to add one.
    let drawn = render_wizard(&mut state);
    assert!(
        drawn.contains(i18n.t("add_port_forward")),
        "the block names port forwarding in its title; drew:
{}",
        drawn
    );

    // With a rule, both ports are editable right there.
    state.add_port_forward();
    let drawn = render_wizard(&mut state);
    assert!(
        drawn.contains(i18n.t("host_port")) && drawn.contains(i18n.t("guest_port")),
        "a rule must be editable where it stands; drew:
{}",
        drawn
    );
}

#[test]
fn test_a_port_forward_added_in_the_wizard_reaches_the_machine() {
    let mut state = opened();
    state.select("debian");
    state.name = "Debian".to_string();
    state.add_port_forward();
    let draft = state.draft.as_ref().unwrap();
    assert_eq!(draft.network.port_forwards.len(), 1);

    let vm = state.take_draft().expect("there is a draft");
    assert_eq!(
        vm.network.port_forwards.len(),
        1,
        "the rule travels with it"
    );
}

// --------------------------------------------------------------------------
// Part 3b, task 4: downloading at the ISO field
// --------------------------------------------------------------------------

#[test]
fn test_the_download_button_appears_only_where_there_is_a_stable_link() {
    let i18n = I18n::new("en");
    for key in ["debian", "ubuntu", "arch", "alpine"] {
        let mut state = opened();
        state.select(key);
        assert!(
            render_wizard(&mut state).contains(i18n.t("download")),
            "{} has a stable, documented, checkable address",
            key
        );
    }
    for key in ["windows-11", "windows-10"] {
        let mut state = opened();
        state.select(key);
        let drawn = render_wizard(&mut state);
        assert!(
            !drawn.contains(i18n.t("download")),
            "{} has no stable link: Microsoft hands out session-bound ones",
            key
        );
        assert!(
            drawn.contains(i18n.t("open_official_page")),
            "{} drew:
{}",
            key,
            drawn
        );
    }
}

#[test]
fn test_the_wizard_states_what_a_checksum_proves_and_what_it_does_not() {
    // Spec section 6: the limit must be in the interface, not only in the
    // design document.
    let i18n = I18n::new("en");
    let mut state = opened();
    state.select("debian");
    assert!(render_wizard(&mut state).contains(i18n.t("download_limit_notice")));
}

#[test]
fn test_asking_for_a_download_is_remembered_until_the_machine_is_made() {
    let mut state = opened();
    state.select("debian");
    state.name = "Debian".to_string();
    assert!(!state.wants_download(), "nobody asked yet");
    state.request_download();
    assert!(state.wants_download());

    // The request survives handing out the draft, because the caller starts
    // the download only after the machine exists.
    let _ = state.take_draft().expect("there is a draft");
    assert!(state.wants_download());

    state.created();
    assert!(!state.wants_download(), "and is cleared for the next time");
}
