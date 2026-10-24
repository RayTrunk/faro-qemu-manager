// FARO QEMU MANAGER (C) R.Trunk 2026
// Settings dialog component

use egui::Context;
use std::path::{Path, PathBuf};

use crate::config::{default_vm_dir, AppSettings, Arch, CreationMode};
use crate::i18n::I18n;
use crate::ui::theme::{apply_theme, icons, COLOR_RUNNING, COLOR_WARNING};

/// Modal dialog for application settings
/// How wide the settings dialog is, in points.
///
/// Fixed on purpose. Everything inside measures against this number, so
/// nothing in the dialog can push the dialog, and a long path cannot make
/// the window creep across the screen.
const SETTINGS_WIDTH: f32 = 520.0;

pub struct SettingsDialog<'a> {
    pub open: &'a mut bool,
    pub settings: &'a mut AppSettings,
    pub i18n: &'a mut I18n,
    /// The emulator the application resolved, or `None` when it found
    /// none. Handed in rather than searched for here: this dialog draws
    /// every frame it is open, and the file system does not change between
    /// two frames (spec section 7.2).
    pub detected_qemu: Option<&'a Path>,
    /// Whether the QEMU directory field has been typed in since it was
    /// last reported. The caller owns it: this dialog is built anew for
    /// every frame, so a flag of its own would forget on the next one.
    pub qemu_dir_edited: Option<&'a mut bool>,
}

impl<'a> SettingsDialog<'a> {
    /// Creates a new `SettingsDialog` instance
    pub fn new(open: &'a mut bool, settings: &'a mut AppSettings, i18n: &'a mut I18n) -> Self {
        Self {
            open,
            settings,
            i18n,
            detected_qemu: None,
            qemu_dir_edited: None,
        }
    }

    /// Tell the dialog which emulator the application found.
    pub fn with_detected_qemu(mut self, detected: Option<&'a Path>) -> Self {
        self.detected_qemu = detected;
        self
    }

    /// Lend the dialog the caller's memory of an unfinished edit.
    pub fn editing_qemu_dir(mut self, edited: &'a mut bool) -> Self {
        self.qemu_dir_edited = Some(edited);
        self
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

        // A fixed width, not a suggested one. This dialog holds a field
        // whose width is derived from the space available to it, and the
        // space available to it used to be derived from the width of the
        // dialog -- which is derived from its content. That circle grew the
        // window by thirteen pixels per frame, for as long as it was open.
        let window = egui::Window::new(format!("{} {}", icons::SETTINGS, self.i18n.t("settings")))
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .min_width(SETTINGS_WIDTH)
            .max_width(SETTINGS_WIDTH)
            .default_height(360.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]);

        window.show(ctx, |ui| {
            ui.add_space(8.0);

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
                            ui.selectable_value(
                                &mut selected_theme,
                                "dark".to_string(),
                                self.i18n.t("dark"),
                            );
                            ui.selectable_value(
                                &mut selected_theme,
                                "light".to_string(),
                                self.i18n.t("light"),
                            );
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

                    // Which way the New button takes. Both ways stay: the
                    // wizard is a starting point, not a replacement.
                    ui.label(egui::RichText::new(self.i18n.t("creation_mode")).strong());
                    let mut mode = self.settings.creation_mode;
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut mode,
                            CreationMode::Wizard,
                            self.i18n.t("creation_mode_wizard"),
                        );
                        ui.selectable_value(
                            &mut mode,
                            CreationMode::Direct,
                            self.i18n.t("creation_mode_direct"),
                        );
                    });
                    if mode != self.settings.creation_mode {
                        self.settings.creation_mode = mode;
                        modified = true;
                    }
                    ui.end_row();

                    // Where VM folders go. Empty means the default, which is
                    // not the same as freezing today's default into the file.
                    ui.label(egui::RichText::new(self.i18n.t("vm_dir")).strong());
                    ui.horizontal(|ui| {
                        let mut shown = self
                            .settings
                            .vm_dir
                            .as_ref()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let default_hint = default_vm_dir().to_string_lossy().to_string();
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut shown)
                                .desired_width(260.0)
                                .hint_text(default_hint),
                        );
                        if edit.changed() {
                            let trimmed = shown.trim();
                            self.settings.vm_dir = if trimmed.is_empty() {
                                None
                            } else {
                                Some(PathBuf::from(trimmed))
                            };
                            modified = true;
                        }
                        if ui
                            .button(format!("{} {}", icons::FOLDER, self.i18n.t("browse")))
                            .clicked()
                        {
                            let mut dialog = rfd::FileDialog::new();
                            if let Some(cur) = &self.settings.vm_dir {
                                dialog = dialog.set_directory(cur);
                            }
                            if let Some(folder) = dialog.pick_folder() {
                                self.settings.vm_dir = Some(folder);
                                modified = true;
                            }
                        }
                        if self.settings.vm_dir.is_some()
                            && ui
                                .button(icons::CLOSE)
                                .on_hover_text(self.i18n.t("clear"))
                                .clicked()
                        {
                            self.settings.vm_dir = None;
                            modified = true;
                        }
                    });
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
                // Measured against the dialog's own fixed width, never
                // against what happens to be left over: the leftover is
                // what the window is sized from.
                let desired_width = (SETTINGS_WIDTH - 150.0 - clear_width).max(120.0);

                let resp = ui.add(
                    egui::TextEdit::singleline(&mut dir_str)
                        .hint_text(self.i18n.t("qemu_dir_auto"))
                        .desired_width(desired_width),
                );

                // Kept in the settings on every keystroke, but reported as
                // modified only when the field is done: the application
                // answers `modified` by probing QEMU, which starts five
                // processes and takes the better part of a second. Doing
                // that per keystroke froze the window for as long as it
                // took to type a path.
                if resp.changed() {
                    let trimmed = dir_str.trim();
                    self.settings.qemu_dir = if trimmed.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(trimmed))
                    };
                    if let Some(edited) = self.qemu_dir_edited.as_deref_mut() {
                        *edited = true;
                    } else {
                        // No memory lent: report at once rather than lose it.
                        modified = true;
                    }
                }
                let edited_before = self.qemu_dir_edited.as_deref().copied().unwrap_or(false);
                if resp.lost_focus() && edited_before {
                    if let Some(edited) = self.qemu_dir_edited.as_deref_mut() {
                        *edited = false;
                    }
                    modified = true;
                }

                if ui
                    .button(format!("{} {}", icons::FOLDER, self.i18n.t("browse")))
                    .clicked()
                {
                    let mut dialog = rfd::FileDialog::new();
                    if let Some(cur) = &self.settings.qemu_dir {
                        dialog = dialog.set_directory(cur);
                    }
                    if let Some(folder) = dialog.pick_folder() {
                        self.settings.qemu_dir = Some(folder);
                        modified = true;
                    }
                }

                if has_path
                    && ui
                        .small_button(icons::CLOSE)
                        .on_hover_text(self.i18n.t("clear"))
                        .clicked()
                {
                    self.settings.qemu_dir = None;
                    modified = true;
                }
            });

            ui.add_space(6.0);

            // Status indicator: what the application resolved, not a fresh
            // search. A directory the user just picked is reflected because
            // the application re-resolves on that change.
            ui.horizontal(|ui| match self.detected_qemu {
                Some(bin_path) => {
                    ui.label(egui::RichText::new(icons::CHECK).color(COLOR_RUNNING));
                    // Wrapped, because the dialog's width is fixed and a
                    // path like C:\Program Files\qemu\qemu-system-x86_64.exe
                    // is longer than it. Two lines beat a clipped one.
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                self.i18n
                                    .tf("qemu_found", &[("path", &bin_path.display().to_string())]),
                            )
                            .size(12.0)
                            .color(COLOR_RUNNING),
                        )
                        .wrap(),
                    );
                }
                None => {
                    ui.label(egui::RichText::new(icons::WARNING).color(COLOR_WARNING));
                    ui.label(
                        egui::RichText::new(self.i18n.t("qemu_not_found"))
                            .size(12.0)
                            .color(COLOR_WARNING),
                    );
                }
            });

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(6.0);

            // 5. Version and close button. The version belongs here
            // because every fault report starts with "which build?", and
            // nobody should have to read the file properties of the
            // executable to answer that. It comes from the package, so a
            // release cannot leave this spot behind.
            let weak = ui.visuals().weak_text_color();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(
                        self.i18n
                            .tf("version", &[("version", env!("CARGO_PKG_VERSION"))]),
                    )
                    .size(11.0)
                    .color(weak),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(self.i18n.t("close")).clicked() {
                        close_requested = true;
                    }
                });
            });
        });

        if close_requested {
            is_open = false;
        }

        *self.open = is_open;
        modified
    }
}
