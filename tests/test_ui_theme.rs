// FARO QEM (C) R.Trunk 2026
// Unit tests for UI Theme and typography

use egui::{Color32, Context};
use faro_qem::ui::*;

#[test]
fn test_theme_colors() {
    assert_eq!(COLOR_RUNNING, Color32::from_rgb(16, 185, 129));
    assert_eq!(COLOR_STOPPED, Color32::from_rgb(113, 113, 122));
    assert_eq!(COLOR_ERROR, Color32::from_rgb(225, 29, 72));
    assert_eq!(COLOR_PRIMARY, Color32::from_rgb(0, 47, 167));
    assert_eq!(COLOR_PRIMARY_HOVER, Color32::from_rgb(26, 68, 189));
    assert_eq!(COLOR_ACCENT, Color32::from_rgb(99, 102, 241));
    assert_eq!(COLOR_WARNING, Color32::from_rgb(245, 158, 11));
}

#[test]
fn test_icons_constants() {
    assert_eq!(icons::PLAY, "\u{25B6}");
    assert_eq!(icons::STOP, "\u{23F9}");
    assert_eq!(icons::SETTINGS, "\u{2699}");
    assert_eq!(icons::ADD, "\u{2795}");
    assert_eq!(icons::VM, "\u{1F5A5}");
    assert_eq!(icons::CPU, "\u{26A1}");
    assert_eq!(icons::DISK, "\u{1F5B4}");
    assert_eq!(icons::NETWORK, "\u{1F310}");
    assert_eq!(icons::CONSOLE, "\u{1F4BB}");
    assert_eq!(icons::COPY, "\u{1F4CB}");
}

#[test]
fn test_configure_modern_fonts() {
    let ctx = Context::default();
    configure_modern_fonts(&ctx);

    // Verify style text styles are populated with expected sizes
    let style = ctx.style();
    assert!(style.text_styles.contains_key(&egui::TextStyle::Heading));
    assert!(style.text_styles.contains_key(&egui::TextStyle::Body));
    assert!(style.text_styles.contains_key(&egui::TextStyle::Button));
    assert!(style.text_styles.contains_key(&egui::TextStyle::Monospace));
    assert!(style.text_styles.contains_key(&egui::TextStyle::Small));

    assert_eq!(
        style
            .text_styles
            .get(&egui::TextStyle::Heading)
            .unwrap()
            .size,
        20.0
    );
    assert_eq!(
        style.text_styles.get(&egui::TextStyle::Body).unwrap().size,
        14.0
    );
    assert_eq!(
        style
            .text_styles
            .get(&egui::TextStyle::Button)
            .unwrap()
            .size,
        14.0
    );
    assert_eq!(
        style
            .text_styles
            .get(&egui::TextStyle::Monospace)
            .unwrap()
            .size,
        13.0
    );
    assert_eq!(
        style.text_styles.get(&egui::TextStyle::Small).unwrap().size,
        11.0
    );

    // Verify spacing settings
    assert_eq!(style.spacing.item_spacing, egui::vec2(8.0, 8.0));
    assert_eq!(style.spacing.button_padding, PADDING_BUTTON);
    assert_eq!(style.spacing.window_margin, egui::Margin::same(12.0));
}

#[test]
fn test_apply_theme_dark_and_light() {
    let ctx = Context::default();

    // Dark theme
    apply_theme(&ctx, "dark");
    {
        let visuals = &ctx.style().visuals;
        assert!(visuals.dark_mode);
        assert_eq!(
            visuals.window_rounding,
            egui::Rounding::same(ROUNDING_WINDOW)
        );
        assert_eq!(
            visuals.widgets.noninteractive.rounding,
            egui::Rounding::same(ROUNDING_BUTTON)
        );
        assert_eq!(
            visuals.widgets.inactive.rounding,
            egui::Rounding::same(ROUNDING_BUTTON)
        );
        assert_eq!(
            visuals.widgets.hovered.rounding,
            egui::Rounding::same(ROUNDING_BUTTON)
        );
        assert_eq!(
            visuals.widgets.active.rounding,
            egui::Rounding::same(ROUNDING_BUTTON)
        );
        assert_eq!(visuals.panel_fill, COLOR_DARK_BG);
        assert_eq!(visuals.window_fill, COLOR_DARK_SURFACE);
    }

    // Light theme (case-insensitive)
    apply_theme(&ctx, "Light");
    {
        let visuals = &ctx.style().visuals;
        assert!(!visuals.dark_mode);
        assert_eq!(
            visuals.window_rounding,
            egui::Rounding::same(ROUNDING_WINDOW)
        );
        assert_eq!(
            visuals.widgets.noninteractive.rounding,
            egui::Rounding::same(ROUNDING_BUTTON)
        );
        assert_eq!(visuals.panel_fill, COLOR_LIGHT_BG);
        assert_eq!(visuals.window_fill, COLOR_LIGHT_SURFACE);
    }

    // Unknown defaults to dark
    apply_theme(&ctx, "solarized_unknown");
    assert!(ctx.style().visuals.dark_mode);
}

#[test]
fn test_card_frame() {
    let dark_visuals = egui::Visuals::dark();
    let dark_frame = card_frame(&dark_visuals);
    assert_eq!(dark_frame.fill, COLOR_DARK_SURFACE);
    assert_eq!(dark_frame.rounding, egui::Rounding::same(ROUNDING_CARD));

    let light_visuals = egui::Visuals::light();
    let light_frame = card_frame(&light_visuals);
    assert_eq!(light_frame.fill, COLOR_LIGHT_SURFACE);
    assert_eq!(light_frame.rounding, egui::Rounding::same(ROUNDING_CARD));
}

#[test]
fn test_status_badge_rendering() {
    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            status_badge(ui, "Running", COLOR_RUNNING);
            status_badge(ui, "Stopped", COLOR_STOPPED);
            status_badge(ui, "Error", COLOR_ERROR);

            let resp = status_badge_interactive(ui, "Interactive", COLOR_PRIMARY);
            assert!(resp.rect.width() > 0.0);
            assert!(resp.rect.height() > 0.0);
        });
    });
}

#[test]
fn test_ui_components_rendering() {
    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            section_header(ui, "Virtual Machines");
            let p_btn = primary_button(ui, "Start VM");
            let d_btn = danger_button(ui, "Delete VM");

            assert!(p_btn.rect.width() > 0.0);
            assert!(d_btn.rect.width() > 0.0);
        });
    });
}

#[test]
fn test_load_app_logo_and_icon() {
    let logo = load_app_logo();
    assert!(logo.is_some(), "Embedded logo must load successfully");
    let logo_img = logo.unwrap();
    assert!(logo_img.width() > 0);
    assert!(logo_img.height() > 0);
    assert_eq!(logo_img.pixels.len(), logo_img.width() * logo_img.height());

    let icon = load_app_icon();
    assert!(icon.is_some(), "Embedded app icon must load successfully");
    let icon_data = icon.unwrap();
    assert_eq!(icon_data.width, 128);
    assert_eq!(icon_data.height, 128);
    assert_eq!(icon_data.rgba.len(), 128 * 128 * 4);
}
