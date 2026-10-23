// FARO QEMU MANAGER (C) R.Trunk 2026
// VM Detail Tabs and Hardware Controls component

// System tab of the VM detail panel.

use std::path::PathBuf;

use crate::config::{Accelerator, Arch, Firmware};
use crate::qemu::nvram_path_for;
use crate::ui::theme::{card_frame, icons, COLOR_ERROR, COLOR_PRIMARY, COLOR_WARNING};

/// Graphics adapters worth offering for `arch`.
///
/// `-vga` and its named types are an x86 convenience. On every other
/// architecture QEMU has no VGA adapter and answers `Virtio VGA not
use super::offers::*;
use super::VmDetailView;

impl<'a> VmDetailView<'a> {
    /// Tab 1: System & Hardware
    pub(super) fn show_system_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.i18n.t("tab_system"));
        ui.add_space(6.0);

        card_frame(ui.visuals()).show(ui, |ui| {
            egui::Grid::new("vm_system_hardware_grid")
                .num_columns(2)
                .spacing([20.0, 14.0])
                .show(ui, |ui| {
                    // 1. Architecture
                    ui.label(egui::RichText::new(self.i18n.t("arch")).strong());
                    let old_arch = self.vm.arch;
                    egui::ComboBox::from_id_salt("detail_arch_combo")
                        .selected_text(self.vm.arch.as_str())
                        .show_ui(ui, |ui| {
                            for a in Arch::all() {
                                ui.selectable_value(&mut self.vm.arch, *a, a.as_str());
                            }
                        });
                    if self.vm.arch != old_arch {
                        if self.vm.machine_type.is_empty()
                            || self.vm.machine_type == old_arch.default_machine()
                        {
                            self.vm.machine_type = self.vm.arch.default_machine().to_string();
                        }
                        if self.vm.cpu_model.is_empty()
                            || self.vm.cpu_model == old_arch.default_cpu()
                        {
                            self.vm.cpu_model = self.vm.arch.default_cpu().to_string();
                        }
                    }
                    ui.end_row();

                    // 2. Machine Type
                    ui.label(egui::RichText::new(self.i18n.t("machine")).strong());
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.vm.machine_type)
                                .hint_text(self.vm.arch.default_machine())
                                .desired_width(140.0),
                        );
                        let suggestions = offered_machine_types(self.vm.arch, self.capabilities);
                        for machine in suggestions {
                            if ui.small_button(machine).clicked() {
                                self.vm.machine_type = machine.to_string();
                            }
                        }
                        if !self.capabilities.is_some_and(|c| c.was_probed()) {
                            // Nothing was filtered, so nothing here is known
                            // to work. Better said than silently implied.
                            ui.label(
                                egui::RichText::new(self.i18n.t("option_unverified"))
                                    .size(11.0)
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
                    });
                    ui.end_row();

                    // 3. Accelerator
                    ui.label(egui::RichText::new(self.i18n.t("accel")).strong());
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("detail_accel_combo")
                            .selected_text(self.vm.accel.as_str().to_uppercase())
                            .show_ui(ui, |ui| {
                                for acc in offered_accelerators() {
                                    ui.selectable_value(
                                        &mut self.vm.accel,
                                        acc,
                                        acc.as_str().to_uppercase(),
                                    );
                                }
                            });
                        if !offered_accelerators().contains(&self.vm.accel) {
                            // A stored value this build cannot use is kept,
                            // not quietly rewritten — so it has to be marked.
                            ui.label(
                                egui::RichText::new(self.i18n.t("option_substituted"))
                                    .size(11.0)
                                    .color(COLOR_WARNING),
                            );
                        }
                        let hint = match self.vm.accel {
                            Accelerator::Auto => {
                                if cfg!(target_os = "windows") {
                                    self.i18n.t("hint_accel_auto_whpx")
                                } else if cfg!(target_os = "linux") {
                                    self.i18n.t("hint_accel_auto_kvm")
                                } else {
                                    self.i18n.t("hint_accel_auto_tcg")
                                }
                            }
                            Accelerator::WHPX => self.i18n.t("hint_accel_whpx"),
                            Accelerator::HAXM => self.i18n.t("hint_accel_haxm"),
                            Accelerator::KVM => self.i18n.t("hint_accel_kvm"),
                            Accelerator::TCG => self.i18n.t("hint_accel_tcg"),
                        };
                        ui.label(
                            egui::RichText::new(hint)
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.end_row();

                    // 4. CPU Model
                    ui.label(egui::RichText::new(self.i18n.t("cpu")).strong());
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.vm.cpu_model)
                                .hint_text(self.vm.arch.default_cpu())
                                .desired_width(140.0),
                        );
                        let cpu_suggestions: &[&str] = match self.vm.arch {
                            Arch::X86_64 | Arch::I386 => &["host", "max", "qemu64"],
                            Arch::Aarch64 => &["host", "cortex-a72", "cortex-a57", "max"],
                            Arch::Arm => &["cortex-a15", "cortex-a9", "max"],
                            Arch::Riscv64 => &["rv64", "max"],
                        };
                        for &c in cpu_suggestions {
                            if ui.small_button(c).clicked() {
                                self.vm.cpu_model = c.to_string();
                            }
                        }
                    });
                    ui.end_row();

                    // 5. CPU Cores Slider
                    ui.label(egui::RichText::new(self.i18n.t("cores")).strong());
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut self.vm.cpu_cores, 1..=32)
                                .text(self.i18n.t("cores")),
                        );
                        for &c in &[1, 2, 4, 8, 16] {
                            if ui.small_button(format!("{}", c)).clicked() {
                                self.vm.cpu_cores = c;
                            }
                        }
                    });
                    ui.end_row();

                    // 6. Memory Allocation
                    ui.label(egui::RichText::new(self.i18n.t("memory_mb")).strong());
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Slider::new(&mut self.vm.memory_mb, 128..=65536)
                                    .text("MB")
                                    .step_by(128.0),
                            );
                            let gb = self.vm.memory_mb as f64 / 1024.0;
                            ui.label(
                                egui::RichText::new(format!("({:.2} GB)", gb))
                                    .color(COLOR_PRIMARY)
                                    .strong(),
                            );
                        });
                        ui.horizontal(|ui| {
                            let presets: &[(u32, &str)] = &[
                                (1024, "1 GB"),
                                (2048, "2 GB"),
                                (4096, "4 GB"),
                                (8192, "8 GB"),
                                (16384, "16 GB"),
                                (32768, "32 GB"),
                            ];
                            for &(mb, label) in presets {
                                if ui.small_button(label).clicked() {
                                    self.vm.memory_mb = mb;
                                }
                            }
                        });
                    });
                    ui.end_row();

                    // 7. Firmware
                    ui.label(egui::RichText::new(self.i18n.t("firmware")).strong());
                    ui.vertical(|ui| {
                        let is_uefi = matches!(self.vm.firmware, Firmware::Uefi { .. });

                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(!is_uefi, self.i18n.t("firmware_bios"))
                                .clicked()
                                && is_uefi
                            {
                                self.vm.firmware = Firmware::Bios { rom: None };
                            }
                            if ui
                                .selectable_label(is_uefi, self.i18n.t("firmware_uefi"))
                                .clicked()
                                && !is_uefi
                            {
                                self.vm.firmware = Firmware::Uefi {
                                    nvram: nvram_path_for(self.vm),
                                };
                            }
                        });

                        match &mut self.vm.firmware {
                            Firmware::Bios { rom } => {
                                ui.horizontal(|ui| {
                                    let mut path_str = rom
                                        .as_ref()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_default();
                                    let resp = ui.add(
                                        egui::TextEdit::singleline(&mut path_str)
                                            .hint_text(self.i18n.t("firmware_rom"))
                                            .desired_width(220.0),
                                    );
                                    if resp.changed() {
                                        let trimmed = path_str.trim();
                                        *rom = if trimmed.is_empty() {
                                            None
                                        } else {
                                            Some(PathBuf::from(trimmed))
                                        };
                                    }
                                    if ui
                                        .button(format!(
                                            "{} {}",
                                            icons::FOLDER,
                                            self.i18n.t("browse")
                                        ))
                                        .clicked()
                                    {
                                        if let Some(file) = rfd::FileDialog::new()
                                            .add_filter(
                                                self.i18n.t("filter_firmware"),
                                                &["fd", "bin", "rom"],
                                            )
                                            .pick_file()
                                        {
                                            *rom = Some(file);
                                        }
                                    }
                                });
                            }
                            Firmware::Uefi { nvram } => {
                                ui.label(
                                    egui::RichText::new(self.i18n.t("firmware_uefi_hint"))
                                        .size(11.0)
                                        .color(ui.visuals().weak_text_color()),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}: {}",
                                        self.i18n.t("firmware_nvram"),
                                        nvram.display()
                                    ))
                                    .size(11.0)
                                    .monospace()
                                    .color(ui.visuals().weak_text_color()),
                                );
                                if !self.uefi_available {
                                    ui.label(
                                        egui::RichText::new(self.i18n.t("firmware_missing"))
                                            .size(11.0)
                                            .color(COLOR_ERROR),
                                    );
                                }
                            }
                        }
                    });
                    ui.end_row();
                });
        });
    }
}
