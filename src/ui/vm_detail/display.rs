// FARO QEMU MANAGER (C) R.Trunk 2026
// VM Detail Tabs and Hardware Controls component

// Display tab of the VM detail panel.

use crate::ui::theme::card_frame;

use super::offers::*;
use super::VmDetailView;

impl<'a> VmDetailView<'a> {
    /// Tab 3: Display & Graphics
    pub(super) fn show_display_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.i18n.t("tab_display"));
        ui.add_space(6.0);

        card_frame(ui.visuals()).show(ui, |ui| {
            egui::Grid::new("vm_display_grid")
                .num_columns(2)
                .spacing([20.0, 14.0])
                .show(ui, |ui| {
                    // VGA Type
                    ui.label(egui::RichText::new(self.i18n.t("vga_type")).strong());
                    ui.horizontal(|ui| {
                        let vga_options = offered_vga_types(self.vm.arch);
                        egui::ComboBox::from_id_salt("detail_vga_combo")
                            .selected_text(&self.vm.display.vga_type)
                            .show_ui(ui, |ui| {
                                for opt in &vga_options {
                                    ui.selectable_value(
                                        &mut self.vm.display.vga_type,
                                        opt.to_string(),
                                        *opt,
                                    );
                                }
                            });
                        let hint = match self.vm.display.vga_type.as_str() {
                            "virtio-vga" => self.i18n.t("hint_vga_virtio"),
                            "std" => self.i18n.t("hint_vga_std"),
                            "qxl" => self.i18n.t("hint_vga_qxl"),
                            "cirrus" => self.i18n.t("hint_vga_cirrus"),
                            "none" => self.i18n.t("hint_vga_none"),
                            _ => "",
                        };
                        ui.label(
                            egui::RichText::new(hint)
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.end_row();

                    // Display Backend
                    ui.label(egui::RichText::new(self.i18n.t("backend")).strong());
                    ui.horizontal(|ui| {
                        let backend_options = ["default", "gtk", "sdl", "none"];
                        egui::ComboBox::from_id_salt("detail_display_backend_combo")
                            .selected_text(&self.vm.display.display_backend)
                            .show_ui(ui, |ui| {
                                for opt in &backend_options {
                                    ui.selectable_value(
                                        &mut self.vm.display.display_backend,
                                        opt.to_string(),
                                        *opt,
                                    );
                                }
                            });
                        let hint = match self.vm.display.display_backend.as_str() {
                            "default" => self.i18n.t("hint_display_default"),
                            "gtk" => self.i18n.t("hint_display_gtk"),
                            "sdl" => self.i18n.t("hint_display_sdl"),
                            "none" => self.i18n.t("hint_display_none"),
                            _ => "",
                        };
                        ui.label(
                            egui::RichText::new(hint)
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.end_row();

                    // Fullscreen
                    ui.label(egui::RichText::new(self.i18n.t("fullscreen")).strong());
                    ui.checkbox(
                        &mut self.vm.display.fullscreen,
                        self.i18n.t("fullscreen_hint"),
                    );
                    ui.end_row();
                });
        });
    }
}
