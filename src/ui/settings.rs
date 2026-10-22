// FARO QEM (C) R.Trunk 2026
// Settings dialog component

use std::path::PathBuf;
use egui::Context;

use crate::config::{Arch, AppSettings};
use crate::i18n::I18n;
use crate::qemu::find_qemu_binary;
use crate::ui::theme::{
    apply_theme, icons, section_header, COLOR_RUNNING, COLOR_WARNING,
};

/// Modal dialog for application settings
pub struct SettingsDialog<'a> {
    pub open: &'a mut bool,
    pub settings: &'a mut AppSettings,
    pub i18n: &'a mut I18n,
}

impl<'a> SettingsDialog<'a> {
    /// Creates a new `SettingsDialog` instance
    pub fn new(open: &'a mut bool, settings: &'a mut AppSettings, i18n: &'a mut I18n) -> Self {
        Self {
            open,
            settings,
            i18n,
        }
    }

    /// Renders the settings dialog window.
    /// Returns `true` if any setting was modified.
    pub fn show(&mut self, ctx: &Context) -> bool {
        if !*self.open {
            return false;
        }

        let mut modified = false;
        let mut is_open = *self.open;
        let mut close_requested = false;

        let window = egui::Window::new(format!("{} {}", icons::SETTINGS, self.i18n.t("settings")))
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .default_width(460.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);

        window.show(ctx, |ui| {
            ui.add_space(4.0);
            section_header(ui, self.i18n.t("settings"));

            egui::Grid::new("settings_form_grid")
                .num_columns(2)
                .spacing([18.0, 12.0])
                .show(ui, |ui| {
                    // 1. Language selector
                    ui.label(egui::RichText::new(self.i18n.t("language")).strong());

                    let current_lang = self.settings.language.clone();
                    let current_name = self
                        .i18n
                        .available_locales()
                        .into_iter()
                        .find(|(c, _)| *c == current_lang)
                        .map(|(_, n)| n)
                        .unwrap_or(current_lang.as_str());

                    let mut selected_lang = current_lang.clone();
                    egui::ComboBox::from_id_salt("settings_lang_combo")
                        .selected_text(current_name)
                        .show_ui(ui, |ui| {
                            for (code, name) in self.i18n.available_locales() {
                                ui.selectable_value(&mut selected_lang, code.to_string(), name);
                            }
                        });

                    if selected_lang != current_lang {
                        self.i18n.set_locale(&selected_lang);
                        self.settings.language = selected_lang;
                        modified = true;
                    }
                    ui.end_row();

                    // 2. Theme selector
                    ui.label(egui::RichText::new(self.i18n.t("theme")).strong());

                    let current_theme = self.settings.theme.to_lowercase();
                    let theme_display = if current_theme == "light" {
                        self.i18n.t("light")
                    } else {
                        self.i18n.t("dark")
                    };

                    let mut selected_theme = current_theme.clone();
                    egui::ComboBox::from_id_salt("settings_theme_combo")
                        .selected_text(theme_display)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut selected_theme, "dark".to_string(), self.i18n.t("dark"));
                            ui.selectable_value(&mut selected_theme, "light".to_string(), self.i18n.t("light"));
                        });

                    if selected_theme != current_theme {
                        apply_theme(ctx, &selected_theme);
                        self.settings.theme = selected_theme;
                        modified = true;
                    }
                    ui.end_row();

                    // 3. Default architecture
                    ui.label(egui::RichText::new(self.i18n.t("arch")).strong());

                    let mut selected_arch = self.settings.default_arch;
                    egui::ComboBox::from_id_salt("settings_arch_combo")
                        .selected_text(selected_arch.as_str())
                        .show_ui(ui, |ui| {
                            for arch in Arch::all() {
                                ui.selectable_value(&mut selected_arch, *arch, arch.as_str());
                            }
                        });

                    if selected_arch != self.settings.default_arch {
                        self.settings.default_arch = selected_arch;
                        modified = true;
                    }
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 4. QEMU Executable Directory
            ui.label(egui::RichText::new(self.i18n.t("qemu_dir")).strong());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                let mut dir_str = self
                    .settings
                    .qemu_dir
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                let has_path = self.settings.qemu_dir.is_some();
                let clear_width = if has_path { 28.0 } else { 0.0 };
                let desired_width = (ui.available_width() - 110.0 - clear_width).max(120.0);

                let resp = ui.add(
                    egui::TextEdit::singleline(&mut dir_str)
                        .hint_text("Auto-detected (System PATH & defaults)")
                        .desired_width(desired_width),
                );

                if resp.changed() {
                    let trimmed = dir_str.trim();
                    self.settings.qemu_dir = if trimmed.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(trimmed))
                    };
                    modified = true;
                }

                if ui.button(format!("{} {}", icons::FOLDER, self.i18n.t("browse"))).clicked() {
                    let mut dialog = rfd::FileDialog::new();
                    if let Some(cur) = &self.settings.qemu_dir {
                        dialog = dialog.set_directory(cur);
                    }
                    if let Some(folder) = dialog.pick_folder() {
                        self.settings.qemu_dir = Some(folder);
                        modified = true;
                    }
                }

                if has_path && ui.small_button(icons::CLOSE).on_hover_text(self.i18n.t("clear")).clicked() {
                    self.settings.qemu_dir = None;
                    modified = true;
                }
            });

            ui.add_space(6.0);

            // Status indicator: check if find_qemu_binary finds a binary
            let detected = find_qemu_binary(self.settings.qemu_dir.as_deref(), Arch::X86_64);
            ui.horizontal(|ui| {
                match detected {
                    Some(ref bin_path) => {
                        ui.label(egui::RichText::new(icons::CHECK).color(COLOR_RUNNING));
                        ui.label(
                            egui::RichText::new(format!("QEMU binary found: {}", bin_path.display()))
                                .size(12.0)
                                .color(COLOR_RUNNING),
                        );
                    }
                    None => {
                        ui.label(egui::RichText::new(icons::WARNING).color(COLOR_WARNING));
                        ui.label(
                            egui::RichText::new("No QEMU binary found in selected directory or system PATH")
                                .size(12.0)
                                .color(COLOR_WARNING),
                        );
                    }
                }
            });

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(6.0);

            // 5. Close Button
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(self.i18n.t("close")).clicked() {
                    close_requested = true;
                }
            });
        });

        if close_requested {
            is_open = false;
        }

        *self.open = is_open;
        modified
    }
}

/// Helper function to render the settings dialog
pub fn render_settings_dialog(
    ctx: &Context,
    open: &mut bool,
    settings: &mut AppSettings,
    i18n: &mut I18n,
) -> bool {
    SettingsDialog::new(open, settings, i18n).show(ctx)
}
