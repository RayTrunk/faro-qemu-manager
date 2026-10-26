// FARO QEMU MANAGER (C) R.Trunk 2026
// Modern Typography, Icons & Theme Engine

use egui::{
    epaint::Shadow, vec2, Color32, Context, FontFamily, FontId, Frame, Margin, Response, Rounding,
    Stroke, TextStyle, Ui, Vec2, Visuals, WidgetText,
};

// ---------------------------------------------------------------------------
// Status and Theme Color Palette
// ---------------------------------------------------------------------------

// One colour, one meaning. Green says a machine is running, amber says
// something wants a second look, red says something failed or cannot be
// undone, and blue belongs to the action you came here to take. Nothing
// else is coloured, so that these four always mean something.
//
// Every one is measured against the background it is written on: WCAG asks
// 4.5:1 for text, and the blue this project began with managed 1.79:1 --
// present in the file, invisible on the screen.

/// Emerald green: this machine is running (#10B981)
pub const COLOR_RUNNING: Color32 = Color32::from_rgb(16, 185, 129);

/// Quiet grey for a machine that is simply off (#8A8D96).
/// Lighter than the old #71717A, which sat at 3.1:1 -- legible in theory.
pub const COLOR_STOPPED: Color32 = Color32::from_rgb(138, 141, 150);

/// Rose red: something failed, or cannot be undone (#F43F5E)
pub const COLOR_ERROR: Color32 = Color32::from_rgb(244, 63, 94);

/// The blue of the mark itself: the one action, and nothing else (#3884FF).
/// The deep Klein blue it replaces was unreadable as text and heavy as a
/// fill -- the whole screen had to be near-black for it to show at all.
pub const COLOR_PRIMARY: Color32 = Color32::from_rgb(56, 132, 255);

/// A touch brighter under the pointer (#5C9BFF)
pub const COLOR_PRIMARY_HOVER: Color32 = Color32::from_rgb(92, 155, 255);

/// Deep blue, used only as a fill with white text on it (#1D4ED8).
/// White on this is comfortably readable; on the lighter primary it is not.
pub const COLOR_PRIMARY_DEEP: Color32 = Color32::from_rgb(29, 78, 216);

/// Violet / Indigo accent for special highlights and VM tags (#818CF8)
pub const COLOR_ACCENT: Color32 = Color32::from_rgb(129, 140, 248);

/// Amber: worth a second look before continuing (#F59E0B)
pub const COLOR_WARNING: Color32 = Color32::from_rgb(245, 158, 11);

// Dark theme surfaces, lifted out of near-black. #0E0F12 read as a hole in
// the screen, and anything that had to be visible against it was forced to
// shout. Three gentle steps now: panel, card, border.
pub const COLOR_DARK_BG: Color32 = Color32::from_rgb(23, 25, 30);
pub const COLOR_DARK_SURFACE: Color32 = Color32::from_rgb(31, 34, 40);
pub const COLOR_DARK_SURFACE_HOVER: Color32 = Color32::from_rgb(42, 46, 54);
pub const COLOR_DARK_BORDER: Color32 = Color32::from_rgb(55, 60, 70);

/// Sunk below the panel: the console's terminal, the command line under it.
/// Darker than the background on purpose -- a surface you read *into*
/// rather than one that sits on top -- but no longer the near-black the
/// rest of the interface used to be, which left it indistinguishable.
pub const COLOR_DARK_SUNKEN: Color32 = Color32::from_rgb(16, 18, 23);

// Light theme surface colors (Swiss High-Modernist Light)
pub const COLOR_LIGHT_BG: Color32 = Color32::from_rgb(247, 247, 248);
pub const COLOR_LIGHT_SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
pub const COLOR_LIGHT_SURFACE_HOVER: Color32 = Color32::from_rgb(240, 241, 244);
pub const COLOR_LIGHT_BORDER: Color32 = Color32::from_rgb(226, 226, 230);

// ---------------------------------------------------------------------------
// Dimension & Rounding Constants (Swiss Architectural Precision: 2px)
// ---------------------------------------------------------------------------

pub const ROUNDING_BADGE: f32 = 2.0;
pub const ROUNDING_BUTTON: f32 = 2.0;
pub const ROUNDING_CARD: f32 = 2.0;
pub const ROUNDING_WINDOW: f32 = 2.0;

pub const PADDING_BADGE: Vec2 = Vec2::new(6.0, 2.5);
pub const PADDING_BUTTON: Vec2 = Vec2::new(12.0, 6.0);
pub const PADDING_CARD: f32 = 12.0;

// ---------------------------------------------------------------------------
// Standard Unicode Icons for UI
// ---------------------------------------------------------------------------

pub mod icons {
    pub const PLAY: &str = "\u{25B6}"; // ▶
    pub const STOP: &str = "\u{23F9}"; // ⏹
    pub const SETTINGS: &str = "\u{2699}"; // ⚙
    pub const ADD: &str = "\u{2795}"; // ➕
    pub const DELETE: &str = "\u{1F5D1}"; // 🗑
    pub const EDIT: &str = "\u{270F}"; // ✏
    pub const FOLDER: &str = "\u{1F4C1}"; // 📁
    pub const SAVE: &str = "\u{1F4BE}"; // 💾
    pub const VM: &str = "\u{1F5A5}"; // 🖥
    pub const CPU: &str = "\u{26A1}"; // ⚡
    pub const RAM: &str = "\u{1F4DF}"; // 📟
    pub const DISK: &str = "\u{1F5B4}"; // 🖴
    pub const NETWORK: &str = "\u{1F310}"; // 🌐
    pub const CONSOLE: &str = "\u{1F4BB}"; // 💻
    pub const CHECK: &str = "\u{2714}"; // ✔
    pub const CLOSE: &str = "\u{2716}"; // ✖
    pub const WARNING: &str = "\u{26A0}"; // ⚠
    pub const ERROR: &str = "\u{26D4}"; // ⛔
    pub const SEARCH: &str = "\u{1F50D}"; // 🔍
    pub const LOGS: &str = "\u{1F4DC}"; // 📜
    pub const COPY: &str = "\u{1F4CB}"; // 📋
}

// ---------------------------------------------------------------------------
// Typography & Font Configuration
// ---------------------------------------------------------------------------

/// Configure crisp typography using system font families (Segoe UI, Inter,
/// Cantarell, Roboto) if available, with clean fallback to embedded fonts.
/// Also configures clear typographic hierarchy (Heading: 20pt, Body: 14pt,
/// Button: 14pt, Monospace: 13pt, Small: 11pt).
pub fn configure_modern_fonts(ctx: &Context) {
    // The typefaces themselves, before the scale that sizes them. Called
    // once at startup -- rebuilding the glyph atlas is not frame work.
    ctx.set_fonts(font_definitions());

    // Apply clean typographic scales and layout spacing
    ctx.style_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(20.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Name("Subheading".into()),
                FontId::new(16.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(14.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
            ),
            (
                TextStyle::Small,
                FontId::new(11.0, FontFamily::Proportional),
            ),
        ]
        .into();

        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = PADDING_BUTTON;
        style.spacing.window_margin = Margin::same(12.0);
        style.spacing.indent = 16.0;
        style.spacing.icon_width = 16.0;
        style.spacing.icon_spacing = 6.0;
    });
}

// ---------------------------------------------------------------------------
// Theme Engine & Visual Styling
// ---------------------------------------------------------------------------

/// Apply the specified theme (`"dark"` or `"light"`) with modern frame styling,
/// 6.0px widget corner roundings, 8.0px window roundings, clean margins, and
/// cohesive text contrast.
/// The typefaces the program carries with it.
///
/// IBM Plex rather than the toolkit's default: it was drawn for technical
/// products, its figures are tabular -- 8192 and 2048 line up in a column,
/// which is the whole point of a machine's data sheet -- and the SIL Open
/// Font Licence travels with a GPL build. Three cuts and no more: regular
/// for text, semibold for the few places that carry weight, mono for every
/// value you might compare against another.
///
/// The toolkit's own fonts stay in the list behind ours, so the emoji used
/// as icons still resolve instead of leaving empty boxes.
pub fn font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "plex_sans".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf")),
    );
    fonts.font_data.insert(
        "plex_sans_semibold".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/IBMPlexSans-SemiBold.ttf"
        )),
    );
    fonts.font_data.insert(
        "plex_mono".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/IBMPlexMono-Regular.ttf")),
    );

    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, "plex_sans".to_owned());
    }
    if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
        family.insert(0, "plex_mono".to_owned());
    }
    // A named family for the few headings that carry weight, so a caller
    // asks for the cut by name instead of leaning on fake bold.
    fonts.families.insert(
        FontFamily::Name("plex_semibold".into()),
        vec!["plex_sans_semibold".to_owned(), "plex_sans".to_owned()],
    );

    fonts
}

pub fn apply_theme(ctx: &Context, theme: &str) {
    let is_light = theme.eq_ignore_ascii_case("light");

    let visuals = if is_light {
        create_light_visuals()
    } else {
        create_dark_visuals()
    };

    ctx.set_visuals(visuals);
}

/// Create dark mode visuals (Swiss High-Modernist Dark)
fn create_dark_visuals() -> Visuals {
    let mut visuals = Visuals::dark();

    visuals.dark_mode = true;
    visuals.override_text_color = None;

    // Background surfaces: deep neutral matte, no muddy brown/purple tint
    visuals.panel_fill = COLOR_DARK_BG;
    visuals.window_fill = COLOR_DARK_SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(10, 11, 14);
    visuals.faint_bg_color = Color32::from_rgb(18, 19, 23);
    visuals.code_bg_color = Color32::from_rgb(16, 17, 21);

    // Window & dialogue styling: 2px architectural precision, crisp hairline border
    visuals.window_rounding = Rounding::same(ROUNDING_WINDOW);
    visuals.window_stroke = Stroke::new(1.0, COLOR_DARK_BORDER);
    visuals.window_shadow = Shadow {
        offset: vec2(0.0, 2.0),
        blur: 10.0,
        spread: 0.0,
        color: Color32::from_black_alpha(70),
    };

    // Non-interactive widgets (frames, separators, labels)
    visuals.widgets.noninteractive.bg_fill = COLOR_DARK_SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, COLOR_DARK_BORDER);
    visuals.widgets.noninteractive.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(180, 184, 194));

    // Inactive widgets (buttons, text inputs at rest)
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(26, 28, 34);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COLOR_DARK_BORDER);
    visuals.widgets.inactive.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(220, 224, 232));

    // Hovered widgets: crisp Klein Blue hairline focus
    visuals.widgets.hovered.bg_fill = COLOR_DARK_SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_PRIMARY_HOVER);
    visuals.widgets.hovered.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    // Active / pressed widgets: Yves Klein Blue
    visuals.widgets.active.bg_fill = COLOR_PRIMARY;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, COLOR_PRIMARY_HOVER);
    visuals.widgets.active.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    // Open menus / dropdowns
    visuals.widgets.open.bg_fill = COLOR_DARK_SURFACE_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, COLOR_PRIMARY);
    visuals.widgets.open.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    // What is selected -- a chosen toggle, highlighted text -- is tinted,
    // not painted. A solid accent behind every selectable_label put three
    // or four saturated blocks on screen at once, each as loud as the one
    // action that matters.
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(
        COLOR_PRIMARY.r(),
        COLOR_PRIMARY.g(),
        COLOR_PRIMARY.b(),
        60,
    );
    visuals.selection.stroke = Stroke::new(1.0, COLOR_PRIMARY);

    visuals
}

/// Create light mode visuals (Swiss High-Modernist Light)
fn create_light_visuals() -> Visuals {
    let mut visuals = Visuals::light();

    visuals.dark_mode = false;
    visuals.override_text_color = None;

    // Background surfaces: pure white and stark light gray
    visuals.panel_fill = COLOR_LIGHT_BG;
    visuals.window_fill = COLOR_LIGHT_SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(242, 243, 245);
    visuals.faint_bg_color = Color32::from_rgb(245, 246, 248);
    visuals.code_bg_color = Color32::from_rgb(238, 239, 242);

    // Window & dialogue styling: 2px architectural precision, crisp hairline border
    visuals.window_rounding = Rounding::same(ROUNDING_WINDOW);
    visuals.window_stroke = Stroke::new(1.0, COLOR_LIGHT_BORDER);
    visuals.window_shadow = Shadow {
        offset: vec2(0.0, 2.0),
        blur: 8.0,
        spread: 0.0,
        color: Color32::from_black_alpha(20),
    };

    // Non-interactive widgets
    visuals.widgets.noninteractive.bg_fill = COLOR_LIGHT_SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, COLOR_LIGHT_BORDER);
    visuals.widgets.noninteractive.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(55, 60, 68));

    // Inactive widgets
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(243, 244, 246);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COLOR_LIGHT_BORDER);
    visuals.widgets.inactive.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(25, 30, 36));

    // Hovered widgets
    visuals.widgets.hovered.bg_fill = COLOR_LIGHT_SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, COLOR_PRIMARY);
    visuals.widgets.hovered.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::from_rgb(10, 15, 20));

    // Active / pressed widgets: Yves Klein Blue
    visuals.widgets.active.bg_fill = COLOR_PRIMARY;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, COLOR_PRIMARY_HOVER);
    visuals.widgets.active.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    // Open menus / dropdowns
    visuals.widgets.open.bg_fill = COLOR_LIGHT_SURFACE_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, COLOR_PRIMARY);
    visuals.widgets.open.rounding = Rounding::same(ROUNDING_BUTTON);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::from_rgb(10, 15, 20));

    // Text selection
    visuals.selection.bg_fill = Color32::from_rgb(205, 225, 255);
    visuals.selection.stroke = Stroke::new(1.0, COLOR_PRIMARY);

    visuals
}

// ---------------------------------------------------------------------------
// Reusable UI Helpers & Components
// ---------------------------------------------------------------------------

/// Helper function to calculate readable text color against a background color.
fn badge_text_color(bg: Color32) -> Color32 {
    let lum = 0.299 * (bg.r() as f32) + 0.587 * (bg.g() as f32) + 0.114 * (bg.b() as f32);
    if lum > 165.0 {
        Color32::from_rgb(20, 24, 30)
    } else {
        Color32::WHITE
    }
}

/// Draw a styled status badge (e.g. "RUNNING", "STOPPED", "ERROR") with rounded
/// corners and contrasting text.
pub fn status_badge(ui: &mut Ui, text: &str, color: Color32) {
    let _ = status_badge_interactive(ui, text, color);
}

/// Draw a styled status badge and return the interaction response (for tooltips or clicks).
pub fn status_badge_interactive(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font_id = FontId::new(11.0, FontFamily::Proportional);
    let text_color = badge_text_color(color);

    // The caller's own words, as written. Shouting them in capitals made
    // every badge look like a warning, and cost the ones that are.
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font_id, text_color);
    let size = galley.size() + PADDING_BADGE * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, Rounding::same(ROUNDING_BADGE), color);
        let text_pos = rect.min + PADDING_BADGE;
        ui.painter().galley(text_pos, galley, text_color);
    }

    response
}

/// Reusable modern card frame container with rounded borders and padding
pub fn card_frame(visuals: &Visuals) -> Frame {
    let fill = if visuals.dark_mode {
        COLOR_DARK_SURFACE
    } else {
        COLOR_LIGHT_SURFACE
    };
    let stroke = if visuals.dark_mode {
        Stroke::new(1.0, COLOR_DARK_BORDER)
    } else {
        Stroke::new(1.0, COLOR_LIGHT_BORDER)
    };

    Frame::none()
        .fill(fill)
        .stroke(stroke)
        .rounding(Rounding::same(ROUNDING_CARD))
        .inner_margin(Margin::same(PADDING_CARD))
}

/// Render a styled primary action button
pub fn primary_button(ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
    let button = egui::Button::new(text)
        .fill(COLOR_PRIMARY)
        .min_size(vec2(80.0, 28.0))
        .rounding(Rounding::same(ROUNDING_BUTTON));
    ui.add(button)
}

/// Render a danger / destructive action button
pub fn danger_button(ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
    let button = egui::Button::new(text)
        .fill(COLOR_ERROR)
        .min_size(vec2(80.0, 28.0))
        .rounding(Rounding::same(ROUNDING_BUTTON));
    ui.add(button)
}

/// Render a section header with clean vertical margins
pub fn section_header(ui: &mut Ui, title: &str) {
    ui.vertical(|ui| {
        ui.add_space(6.0);
        ui.heading(title);
        ui.add_space(2.0);
    });
}

/// Load the embedded FARO QEMU MANAGER logo image.
pub fn load_app_logo() -> Option<egui::ColorImage> {
    let bytes = include_bytes!("../../assets/logo.png");
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels = img.into_raw();
    Some(egui::ColorImage::from_rgba_unmultiplied(size, &pixels))
}

/// Load the embedded FARO QEMU MANAGER logo as window icon data (scaled to 128x128 for Win32/X11 compatibility).
pub fn load_app_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../assets/logo.png");
    let img = image::load_from_memory(bytes).ok()?;
    let resized = img
        .resize(128, 128, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (width, height) = (resized.width(), resized.height());
    let rgba = resized.into_raw();
    Some(egui::IconData {
        rgba,
        width,
        height,
    })
}
