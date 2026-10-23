// FARO QEMU MANAGER (C) R.Trunk 2026
// VM Detail Tabs and Hardware Controls component

// Storage tab, and the create-disk dialog.

use std::path::PathBuf;

use crate::config::{DriveConfig, DriveFormat, DriveInterface};
use crate::qemu::{create_disk_image, find_qemu_img};
use crate::ui::theme::{
    card_frame, danger_button, icons, section_header, COLOR_ERROR, COLOR_PRIMARY, COLOR_RUNNING,
};

use super::VmDetailView;

impl<'a> VmDetailView<'a> {
    /// Tab 2: Storage & Media
    pub(super) fn show_storage_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.i18n.t("tab_storage"));
        ui.add_space(6.0);

        // Section 1: Virtual Hard Disks
        card_frame(ui.visuals()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(self.i18n.t("disks"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // [+ Add Existing Disk]
                    if ui
                        .button(format!("{} {}", icons::ADD, self.i18n.t("add_disk")))
                        .clicked()
                    {
                        let dialog = rfd::FileDialog::new().add_filter(
                            self.i18n.t("filter_disk"),
                            &["qcow2", "img", "raw", "vmdk", "vdi", "vhdx"],
                        );
                        if let Some(file) = dialog.pick_file() {
                            let ext = file
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            let format = match ext.as_str() {
                                "qcow2" => DriveFormat::Qcow2,
                                "vmdk" => DriveFormat::Vmdk,
                                _ => DriveFormat::Raw,
                            };
                            self.vm.drives.push(DriveConfig {
                                path: file,
                                format,
                                interface: DriveInterface::VirtIO,
                                boot_index: None,
                            });
                        }
                    }

                    // [✨ Create New Disk Image]
                    let create_modal_btn =
                        egui::Button::new(format!("{} {}", icons::ADD, self.i18n.t("create_disk")))
                            .fill(COLOR_PRIMARY);
                    if ui.add(create_modal_btn).clicked() {
                        let default_name =
                            format!("{}.qcow2", self.vm.name.replace(' ', "_").to_lowercase());
                        self.disk_modal.open_dialog(self.vm_folder, &default_name);
                    }
                });
            });

            ui.add_space(6.0);

            if self.vm.drives.is_empty() {
                ui.label(
                    egui::RichText::new(self.i18n.t("disks_empty_hint"))
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                        .italics(),
                );
            } else {
                let mut remove_idx = None;
                for (idx, drive) in self.vm.drives.iter_mut().enumerate() {
                    card_frame(ui.visuals()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(icons::DISK)
                                    .size(16.0)
                                    .color(COLOR_PRIMARY),
                            );
                            let file_name = drive
                                .path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("drive");
                            ui.label(
                                egui::RichText::new(self.i18n.tf(
                                    "drive_label",
                                    &[("index", &(idx + 1).to_string()), ("name", file_name)],
                                ))
                                .strong(),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if danger_button(
                                        ui,
                                        format!("{} {}", icons::DELETE, self.i18n.t("remove")),
                                    )
                                    .clicked()
                                    {
                                        remove_idx = Some(idx);
                                    }
                                },
                            );
                        });

                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(drive.path.to_string_lossy())
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            // Format
                            ui.label(self.i18n.t("format"));
                            egui::ComboBox::from_id_salt(format!("drive_fmt_{}", idx))
                                .selected_text(drive.format.as_str())
                                .show_ui(ui, |ui| {
                                    for f in DriveFormat::all() {
                                        ui.selectable_value(&mut drive.format, *f, f.as_str());
                                    }
                                });

                            ui.add_space(8.0);

                            // Interface
                            ui.label(self.i18n.t("interface"));
                            egui::ComboBox::from_id_salt(format!("drive_if_{}", idx))
                                .selected_text(drive.interface.as_str())
                                .show_ui(ui, |ui| {
                                    for iface in DriveInterface::all() {
                                        ui.selectable_value(
                                            &mut drive.interface,
                                            *iface,
                                            iface.as_str(),
                                        );
                                    }
                                });

                            ui.add_space(8.0);

                            // Boot Index
                            let mut has_boot = drive.boot_index.is_some();
                            if ui
                                .checkbox(&mut has_boot, self.i18n.t("boot_index"))
                                .changed()
                            {
                                drive.boot_index = if has_boot { Some(idx as u32) } else { None };
                            }
                            if let Some(ref mut bidx) = drive.boot_index {
                                ui.add(egui::DragValue::new(bidx).range(0..=10));
                            }
                        });
                    });
                    ui.add_space(4.0);
                }

                if let Some(idx) = remove_idx {
                    self.vm.drives.remove(idx);
                }
            }
        });

        ui.add_space(8.0);

        // Section 2: CD-ROM / ISO Images
        card_frame(ui.visuals()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(self.i18n.t("cdroms"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(format!("{} {}", icons::ADD, self.i18n.t("add_cdrom")))
                        .clicked()
                    {
                        let dialog = rfd::FileDialog::new()
                            .add_filter(self.i18n.t("filter_iso"), &["iso", "img"]);
                        if let Some(file) = dialog.pick_file() {
                            self.vm.cdroms.push(file);
                        }
                    }
                });
            });

            ui.add_space(6.0);

            if self.vm.cdroms.is_empty() {
                ui.label(
                    egui::RichText::new(self.i18n.t("cdroms_empty_hint"))
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                        .italics(),
                );
            } else {
                let mut remove_cdrom_idx = None;
                for (idx, cdrom) in self.vm.cdroms.iter_mut().enumerate() {
                    card_frame(ui.visuals()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(icons::DISK)
                                    .size(16.0)
                                    .color(COLOR_PRIMARY),
                            );
                            let file_name = cdrom
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("cdrom.iso");
                            ui.label(
                                egui::RichText::new(self.i18n.tf(
                                    "cdrom_label",
                                    &[("index", &(idx + 1).to_string()), ("name", file_name)],
                                ))
                                .strong(),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if danger_button(
                                        ui,
                                        format!("{} {}", icons::CLOSE, self.i18n.t("eject")),
                                    )
                                    .clicked()
                                    {
                                        remove_cdrom_idx = Some(idx);
                                    }
                                    if ui
                                        .button(format!(
                                            "{} {}",
                                            icons::FOLDER,
                                            self.i18n.t("browse")
                                        ))
                                        .clicked()
                                    {
                                        let mut dialog = rfd::FileDialog::new()
                                            .add_filter(self.i18n.t("filter_iso"), &["iso", "img"]);
                                        if let Some(parent) = cdrom.parent() {
                                            dialog = dialog.set_directory(parent);
                                        }
                                        if let Some(new_iso) = dialog.pick_file() {
                                            *cdrom = new_iso;
                                        }
                                    }
                                },
                            );
                        });

                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(cdrom.to_string_lossy())
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.add_space(4.0);
                }

                if let Some(i) = remove_cdrom_idx {
                    self.vm.cdroms.remove(i);
                }
            }
        });
    }

    /// Modal Dialog: Create New Disk Image
    pub(super) fn show_create_disk_modal(&mut self, ctx: &egui::Context) {
        if !self.disk_modal.open {
            return;
        }

        let mut close_modal = false;
        let mut is_open = self.disk_modal.open;

        egui::Window::new(format!("{} {}", icons::ADD, self.i18n.t("create_disk")))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.add_space(4.0);
                section_header(ui, self.i18n.t("create_disk"));

                egui::Grid::new("create_disk_modal_grid")
                    .num_columns(2)
                    .spacing([16.0, 10.0])
                    .show(ui, |ui| {
                        // Path
                        ui.label(egui::RichText::new(self.i18n.t("disk_path")).strong());
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.disk_modal.path_buf)
                                    .hint_text("C:\\VMs\\disk.qcow2")
                                    .desired_width(220.0),
                            );
                            if ui
                                .button(format!("{} {}", icons::FOLDER, self.i18n.t("browse")))
                                .clicked()
                            {
                                let default_name = format!(
                                    "{}.qcow2",
                                    self.vm.name.replace(' ', "_").to_lowercase()
                                );
                                if let Some(save_path) = rfd::FileDialog::new()
                                    .set_file_name(&default_name)
                                    .add_filter(self.i18n.t("format_qcow2"), &["qcow2"])
                                    .add_filter(self.i18n.t("format_raw"), &["raw", "img"])
                                    .save_file()
                                {
                                    self.disk_modal.path_buf =
                                        save_path.to_string_lossy().to_string();
                                }
                            }
                        });
                        ui.end_row();

                        // Format
                        ui.label(egui::RichText::new(self.i18n.t("format")).strong());
                        egui::ComboBox::from_id_salt("modal_disk_fmt_combo")
                            .selected_text(self.disk_modal.format.as_str())
                            .show_ui(ui, |ui| {
                                for f in DriveFormat::all() {
                                    ui.selectable_value(
                                        &mut self.disk_modal.format,
                                        *f,
                                        f.as_str(),
                                    );
                                }
                            });
                        ui.end_row();

                        // Size (GB)
                        ui.label(egui::RichText::new(self.i18n.t("size_gb")).strong());
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Slider::new(&mut self.disk_modal.size_gb, 1..=1000)
                                        .text("GB"),
                                );
                            });
                            ui.horizontal(|ui| {
                                for &s in &[10, 20, 40, 64, 100, 250] {
                                    if ui.small_button(format!("{} GB", s)).clicked() {
                                        self.disk_modal.size_gb = s;
                                    }
                                }
                            });
                        });
                        ui.end_row();
                    });

                ui.add_space(8.0);

                // Status message feedback
                if let Some(ref res) = self.disk_modal.status_msg {
                    match res {
                        Ok(msg) => {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(icons::CHECK).color(COLOR_RUNNING));
                                ui.label(egui::RichText::new(msg).color(COLOR_RUNNING));
                            });
                        }
                        Err(err) => {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(icons::ERROR).color(COLOR_ERROR));
                                ui.label(egui::RichText::new(err).color(COLOR_ERROR));
                            });
                        }
                    }
                    ui.add_space(6.0);
                }

                ui.separator();
                ui.add_space(6.0);

                // Action buttons
                ui.horizontal(|ui| {
                    let create_btn =
                        egui::Button::new(format!("{} {}", icons::ADD, self.i18n.t("create")))
                            .fill(COLOR_PRIMARY);
                    if ui.add(create_btn).clicked() {
                        let trimmed = self.disk_modal.path_buf.trim();
                        if trimmed.is_empty() {
                            self.disk_modal.status_msg =
                                Some(Err(self.i18n.t("disk_path_empty").to_string()));
                        } else {
                            let path = PathBuf::from(trimmed);
                            let qemu_img = self
                                .qemu_img_binary
                                .map(PathBuf::from)
                                .or_else(|| find_qemu_img(None));

                            match qemu_img {
                                Some(bin) => {
                                    match create_disk_image(
                                        &bin,
                                        &path,
                                        self.disk_modal.format,
                                        self.disk_modal.size_gb,
                                    ) {
                                        Ok(()) => {
                                            self.vm.drives.push(DriveConfig {
                                                path,
                                                format: self.disk_modal.format,
                                                interface: DriveInterface::VirtIO,
                                                boot_index: None,
                                            });
                                            self.disk_modal.status_msg =
                                                Some(Ok(self.i18n.t("disk_created").to_string()));
                                            close_modal = true;
                                        }
                                        Err(e) => {
                                            self.disk_modal.status_msg = Some(Err(e));
                                        }
                                    }
                                }
                                None => {
                                    self.disk_modal.status_msg =
                                        Some(Err(self.i18n.t("qemu_img_missing").to_string()));
                                }
                            }
                        }
                    }

                    if ui.button(self.i18n.t("cancel")).clicked() {
                        close_modal = true;
                    }
                });
            });

        if !is_open || close_modal {
            self.disk_modal.open = false;
        }
    }
}
