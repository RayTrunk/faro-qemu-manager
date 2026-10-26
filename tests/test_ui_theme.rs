// FARO QEMU MANAGER (C) R.Trunk 2026
// Unit tests for UI Theme and typography

use egui::{Color32, Context};
use faro_qem::ui::*;

/// The colours that carry meaning have to be told apart at a glance.
///
/// "One colour, one meaning" only works if a person can see which is which.
/// This used to be a list of the same RGB values written twice, which
/// proved only that they had been copied correctly.
#[test]
fn test_the_meaningful_colours_are_distinguishable_from_each_other() {
    let meanings = [
        ("running", COLOR_RUNNING),
        ("error", COLOR_ERROR),
        ("warning", COLOR_WARNING),
        ("primary", COLOR_PRIMARY),
    ];

    for (i, (name_a, a)) in meanings.iter().enumerate() {
        for (name_b, b) in meanings.iter().skip(i + 1) {
            let apart = (a.r() as i32 - b.r() as i32).abs()
                + (a.g() as i32 - b.g() as i32).abs()
                + (a.b() as i32 - b.b() as i32).abs();
            assert!(
                apart > 150,
                "{name_a} and {name_b} are only {apart} apart across the \
                 channels -- too close for one to mean something the other \
                 does not"
            );
        }
    }

    // Hover is the same colour, brighter -- not a different one.
    let hue_shift = (COLOR_PRIMARY.b() as i32 - COLOR_PRIMARY_HOVER.b() as i32).abs();
    assert!(
        hue_shift < 60,
        "the hover tint is a different blue, not a brighter one"
    );
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

/// The program ships its own typefaces rather than borrowing whatever the
/// toolkit defaults to. IBM Plex is chosen on purpose: it was drawn for
/// technical products, its figures are tabular -- so 8192 and 2048 line up
/// in a column -- and its SIL Open Font Licence travels with a GPL build.
#[test]
fn test_the_program_brings_its_own_typefaces() {
    let fonts = faro_qem::ui::theme::font_definitions();

    for face in ["plex_sans", "plex_sans_semibold", "plex_mono"] {
        assert!(
            fonts.font_data.contains_key(face),
            "{face} is missing from the font definitions"
        );
    }

    let proportional = fonts
        .families
        .get(&egui::FontFamily::Proportional)
        .expect("a proportional family");
    assert_eq!(
        proportional.first().map(String::as_str),
        Some("plex_sans"),
        "body text must be set in Plex Sans, not the toolkit default"
    );

    let mono = fonts
        .families
        .get(&egui::FontFamily::Monospace)
        .expect("a monospace family");
    assert_eq!(
        mono.first().map(String::as_str),
        Some("plex_mono"),
        "values must be set in Plex Mono so columns of figures align"
    );

    // Emoji still have to resolve: the interface uses them as icons, and a
    // typeface without them would leave empty boxes behind.
    assert!(
        proportional.len() > 1,
        "the proportional family needs a fallback for glyphs Plex lacks"
    );
}

/// Defining the typefaces is not the same as installing them. This checks
/// the glyphs actually change: a fresh context measures the toolkit's own
/// font, a configured one measures Plex, and the two cannot be identical.
#[test]
fn test_configuring_the_interface_installs_those_typefaces() {
    let plain = Context::default();
    let configured = Context::default();
    faro_qem::ui::theme::configure_modern_fonts(&configured);

    // Inside a frame: egui refuses to hand out font metrics outside one.
    let measure = |ctx: &Context| {
        let mut width = 0.0;
        let _ = ctx.run(Default::default(), |ctx| {
            width = ctx.fonts(|f| {
                f.glyph_width(&egui::FontId::proportional(14.0), 'm')
                    + f.glyph_width(&egui::FontId::monospace(13.0), '8')
            });
        });
        width
    };

    let before = measure(&plain);
    let after = measure(&configured);

    assert!(
        (before - after).abs() > 0.01,
        "the glyphs are unchanged ({before} vs {after}): the typefaces were \
         defined but never handed to the context"
    );
}

/// Contrast ratio between two colours, per WCAG 2.1.
fn contrast(a: Color32, b: Color32) -> f32 {
    fn channel(v: u8) -> f32 {
        let c = v as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    fn luminance(c: Color32) -> f32 {
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }
    let (l1, l2) = (luminance(a), luminance(b));
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

/// Every colour the dark theme writes text in has to be readable on it.
///
/// This replaces an older test that asserted the exact RGB values, which
/// only checked that the numbers had been copied twice. What matters is
/// whether a person can read the result: WCAG asks 4.5:1 for body text.
/// The deep blue this project started with managed 1.8:1 on its own
/// background -- technically present, practically invisible.
#[test]
fn test_every_colour_the_dark_theme_writes_in_is_readable_on_it() {
    let bg = COLOR_DARK_BG;

    for (name, colour, floor) in [
        ("primary", COLOR_PRIMARY, 4.5),
        ("running", COLOR_RUNNING, 4.5),
        ("error", COLOR_ERROR, 4.5),
        ("warning", COLOR_WARNING, 4.5),
        // Stopped is the quiet state: it labels rather than warns, so the
        // 3:1 floor for non-essential interface text applies.
        ("stopped", COLOR_STOPPED, 3.0),
    ] {
        let ratio = contrast(colour, bg);
        assert!(
            ratio >= floor,
            "{name} reaches only {ratio:.2}:1 against the dark background, \
             needs {floor}:1"
        );
    }
}

/// The surfaces have to be distinguishable from one another, but gently.
///
/// A panel, a card and a border that are all within a hair of each other
/// read as one flat slab; one that jumps reads as a hole in the screen.
#[test]
fn test_the_dark_surfaces_separate_without_shouting() {
    let steps = [
        (
            "background to surface",
            contrast(COLOR_DARK_BG, COLOR_DARK_SURFACE),
        ),
        (
            "surface to border",
            contrast(COLOR_DARK_SURFACE, COLOR_DARK_BORDER),
        ),
    ];
    for (what, ratio) in steps {
        assert!(
            ratio > 1.08,
            "{what} is only {ratio:.3}:1 -- the two read as one surface"
        );
        assert!(
            ratio < 2.2,
            "{what} is {ratio:.3}:1 -- that is a hard edge, not a step"
        );
    }
}
