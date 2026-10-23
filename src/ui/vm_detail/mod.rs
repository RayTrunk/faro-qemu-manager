// FARO QEMU MANAGER (C) R.Trunk 2026
// VM Detail Tabs and Hardware Controls component

// VM detail panel: the type, its tabs, and the actions it reports.

mod display;
mod network;
mod offers;
mod storage;
mod system;

pub use offers::*;

use egui::{Color32, Stroke};
use std::path::Path;
use uuid::Uuid;

use crate::config::{DriveFormat, VmConfig};
use crate::i18n::I18n;
use crate::qemu::QemuCapabilities;
use crate::ui::console::{ConsoleLogBuffer, ConsoleView};
use crate::ui::theme::{
    card_frame, icons, status_badge, COLOR_ERROR, COLOR_PRIMARY, COLOR_RUNNING, COLOR_STOPPED,
    ROUNDING_BUTTON,
};

/// Graphics adapters worth offering for `arch`.
///
/// `-vga` and its named types are an x86 convenience. On every other
/// architecture QEMU has no VGA adapter and answers `Virtio VGA not
/// Actions emitted by the VM detail view toolbar and tabs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmDetailAction {
    Launch(Uuid),
    Stop(Uuid),
    Save(Uuid),
}

impl VmDetailAction {
    /// Return the ID of the target virtual machine
    pub fn vm_id(&self) -> Uuid {
        match self {
            VmDetailAction::Launch(id) | VmDetailAction::Stop(id) | VmDetailAction::Save(id) => *id,
        }
    }
}

/// Navigation tabs in the VM detail inspector
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailTab {
    #[default]
    System,
    Storage,
    Display,
    Network,
    Console,
}

impl DetailTab {
    /// Returns all available tabs
    pub fn all() -> &'static [DetailTab] {
        &[
            DetailTab::System,
            DetailTab::Storage,
            DetailTab::Display,
            DetailTab::Network,
            DetailTab::Console,
        ]
    }
}

/// State tracking for the "Create New Disk Image" modal dialog
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDiskState {
    pub open: bool,
    pub path_buf: String,
    pub format: DriveFormat,
    pub size_gb: u32,
    pub status_msg: Option<Result<String, String>>,
}

impl Default for CreateDiskState {
    fn default() -> Self {
        Self {
            open: false,
            path_buf: String::new(),
            format: DriveFormat::Qcow2,
            size_gb: 20,
            status_msg: None,
        }
    }
}

impl CreateDiskState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open modal dialog with an optional default filename
    pub fn open_dialog(&mut self, default_filename: &str) {
        self.open = true;
        self.status_msg = None;
        if self.path_buf.is_empty() && !default_filename.is_empty() {
            self.path_buf = default_filename.to_string();
        }
    }

    /// Close the modal dialog
    pub fn close(&mut self) {
        self.open = false;
    }
}

/// VM Detail View component inspector
pub struct VmDetailView<'a> {
    pub vm: &'a mut VmConfig,
    pub is_running: bool,
    pub i18n: &'a I18n,
    pub active_tab: &'a mut DetailTab,
    pub qemu_img_binary: Option<&'a Path>,
    pub disk_modal: &'a mut CreateDiskState,
    pub console_logs: Option<&'a mut ConsoleLogBuffer>,
    pub pid: Option<u32>,
    pub cli_command: Option<&'a str>,
    pub logo: Option<&'a egui::TextureHandle>,
    /// What the installed QEMU can do. `None` means nobody asked.
    pub capabilities: Option<&'a QemuCapabilities>,
    /// Whether the app found the firmware a UEFI machine needs. Resolved once
    /// beside the capabilities; searching for it here would mean a handful of
    /// `is_file` calls on every frame this panel draws. Unknown counts as
    /// present, the same way an unprobed capability permits everything: a
    /// warning nobody asked for is worse than none.
    pub uefi_available: bool,
}

impl<'a> VmDetailView<'a> {
    /// Creates a new `VmDetailView` instance
    pub fn new(
        vm: &'a mut VmConfig,
        is_running: bool,
        i18n: &'a I18n,
        active_tab: &'a mut DetailTab,
        qemu_img_binary: Option<&'a Path>,
        disk_modal: &'a mut CreateDiskState,
    ) -> Self {
        Self {
            vm,
            is_running,
            i18n,
            active_tab,
            qemu_img_binary,
            disk_modal,
            console_logs: None,
            pid: None,
            cli_command: None,
            logo: None,
            capabilities: None,
            uefi_available: true,
        }
    }

    /// Attach what the installed QEMU can do, so the view offers only that.
    /// Tell the panel whether the UEFI firmware pair was found.
    pub fn with_firmware(mut self, available: bool) -> Self {
        self.uefi_available = available;
        self
    }

    pub fn with_capabilities(mut self, caps: &'a QemuCapabilities) -> Self {
        self.capabilities = Some(caps);
        self
    }

    /// Attach logo texture to detail view header
    pub fn with_logo(mut self, logo: Option<&'a egui::TextureHandle>) -> Self {
        self.logo = logo;
        self
    }

    /// Attach live console log buffer to detail view
    pub fn with_console(mut self, logs: &'a mut ConsoleLogBuffer) -> Self {
        self.console_logs = Some(logs);
        self
    }

    /// Set operating system process ID
    pub fn with_pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }

    /// Set QEMU command line string
    pub fn with_cli_command(mut self, cmd: &'a str) -> Self {
        self.cli_command = Some(cmd);
        self
    }

    /// Render the complete VM detail view, returning any action triggered
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<VmDetailAction> {
        let mut action: Option<VmDetailAction> = None;

        // 1. Top Header Card (Swiss Architectural Folio Header)
        card_frame(ui.visuals()).show(ui, |ui| {
            let compact = ui.available_width() < 560.0;
            if compact {
                // Stacked layout for compact / narrow windows
                ui.horizontal(|ui| {
                    if let Some(logo) = self.logo {
                        ui.image((logo.id(), egui::vec2(26.0, 26.0)));
                    } else {
                        ui.label(
                            egui::RichText::new(icons::VM)
                                .size(20.0)
                                .color(COLOR_PRIMARY),
                        );
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.vm.name)
                            .font(egui::TextStyle::Heading)
                            .desired_width((ui.available_width() - 30.0).max(100.0)),
                    );
                });

                ui.add_space(6.0);

                ui.horizontal_wrapped(|ui| {
                    // [▶ Launch VM]
                    let launch_btn = egui::Button::new(
                        egui::RichText::new(format!("{} {}", icons::PLAY, self.i18n.t("launch")))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(if !self.is_running {
                        COLOR_RUNNING
                    } else {
                        COLOR_STOPPED
                    });
                    if ui.add_enabled(!self.is_running, launch_btn).clicked() {
                        action = Some(VmDetailAction::Launch(self.vm.id));
                    }

                    // [⏹ Force Stop]
                    let stop_btn = egui::Button::new(
                        egui::RichText::new(format!("{} {}", icons::STOP, self.i18n.t("stop")))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(if self.is_running {
                        COLOR_ERROR
                    } else {
                        COLOR_STOPPED
                    });
                    if ui.add_enabled(self.is_running, stop_btn).clicked() {
                        action = Some(VmDetailAction::Stop(self.vm.id));
                    }

                    // [💾 Save]
                    let save_btn = egui::Button::new(
                        egui::RichText::new(format!("{} {}", icons::SAVE, self.i18n.t("save")))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(COLOR_PRIMARY);
                    if ui.add(save_btn).clicked() {
                        action = Some(VmDetailAction::Save(self.vm.id));
                    }
                });
            } else {
                // Standard wide layout: Name & status on left, Actions right-aligned
                ui.horizontal(|ui| {
                    if let Some(logo) = self.logo {
                        ui.image((logo.id(), egui::vec2(28.0, 28.0)));
                    } else {
                        ui.label(
                            egui::RichText::new(icons::VM)
                                .size(22.0)
                                .color(COLOR_PRIMARY),
                        );
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.vm.name)
                            .font(egui::TextStyle::Heading)
                            .desired_width(220.0),
                    );

                    // Running / Stopped Status Badge
                    if self.is_running {
                        let status_text = if let Some(pid) = self.pid {
                            format!("RUNNING · PID {}", pid)
                        } else {
                            "RUNNING".to_string()
                        };
                        status_badge(ui, &status_text, COLOR_RUNNING);
                    } else {
                        status_badge(ui, "STOPPED", COLOR_STOPPED);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // 1. [💾 Save] (rightmost)
                        let save_btn = egui::Button::new(
                            egui::RichText::new(format!("{} {}", icons::SAVE, self.i18n.t("save")))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(COLOR_PRIMARY)
                        .min_size(egui::vec2(80.0, 28.0));
                        if ui.add(save_btn).clicked() {
                            action = Some(VmDetailAction::Save(self.vm.id));
                        }

                        // 2. [⏹ Force Stop] (middle)
                        let stop_btn = egui::Button::new(
                            egui::RichText::new(format!("{} {}", icons::STOP, self.i18n.t("stop")))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(if self.is_running {
                            COLOR_ERROR
                        } else {
                            COLOR_STOPPED
                        })
                        .min_size(egui::vec2(80.0, 28.0));
                        if ui.add_enabled(self.is_running, stop_btn).clicked() {
                            action = Some(VmDetailAction::Stop(self.vm.id));
                        }

                        // 3. [▶ Launch VM] (leftmost of actions)
                        let launch_btn = egui::Button::new(
                            egui::RichText::new(format!(
                                "{} {}",
                                icons::PLAY,
                                self.i18n.t("launch")
                            ))
                            .strong()
                            .color(Color32::WHITE),
                        )
                        .fill(if !self.is_running {
                            COLOR_RUNNING
                        } else {
                            COLOR_STOPPED
                        })
                        .min_size(egui::vec2(95.0, 28.0));
                        if ui.add_enabled(!self.is_running, launch_btn).clicked() {
                            action = Some(VmDetailAction::Launch(self.vm.id));
                        }
                    });
                });
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            // Row 2: Swiss 4-Column Mathematical Metric Folio Header
            // [ ARCHITECTURE ] | [ PROCESSING ] | [ MEMORY ] | [ ACCELERATOR ]
            ui.horizontal(|ui| {
                let col_width = ((ui.available_width() - 3.0 * 16.0) / 4.0).max(80.0);

                // Col 1: Architecture
                ui.allocate_ui(egui::vec2(col_width, 36.0), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("ARCHITECTURE")
                                .size(10.0)
                                .color(ui.visuals().weak_text_color())
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(self.vm.arch.as_str())
                                .size(13.0)
                                .monospace()
                                .strong(),
                        );
                    });
                });

                ui.separator();

                // Col 2: Processing
                ui.allocate_ui(egui::vec2(col_width, 36.0), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("PROCESSING")
                                .size(10.0)
                                .color(ui.visuals().weak_text_color())
                                .strong(),
                        );
                        let cpu_str = if self.vm.cpu_cores == 1 {
                            "1 vCPU".to_string()
                        } else {
                            format!("{} vCPUs", self.vm.cpu_cores)
                        };
                        ui.label(egui::RichText::new(cpu_str).size(13.0).monospace().strong());
                    });
                });

                ui.separator();

                // Col 3: Memory
                ui.allocate_ui(egui::vec2(col_width, 36.0), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("MEMORY")
                                .size(10.0)
                                .color(ui.visuals().weak_text_color())
                                .strong(),
                        );
                        let mem_str = format!(
                            "{} MB ({:.1} GB)",
                            self.vm.memory_mb,
                            self.vm.memory_mb as f64 / 1024.0
                        );
                        ui.label(egui::RichText::new(mem_str).size(13.0).monospace().strong());
                    });
                });

                ui.separator();

                // Col 4: Accelerator
                ui.allocate_ui(egui::vec2(col_width, 36.0), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("ACCELERATOR")
                                .size(10.0)
                                .color(ui.visuals().weak_text_color())
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(self.vm.accel.as_str().to_uppercase())
                                .size(13.0)
                                .monospace()
                                .strong()
                                .color(COLOR_PRIMARY),
                        );
                    });
                });
            });
        });

        ui.add_space(8.0);

        // 2. Tab Navigation Bar (Swiss Underline / High-Contrast Precision Tabs)
        ui.horizontal_wrapped(|ui| {
            let tab_items = [
                (DetailTab::System, icons::CPU, self.i18n.t("tab_system")),
                (DetailTab::Storage, icons::DISK, self.i18n.t("tab_storage")),
                (DetailTab::Display, icons::VM, self.i18n.t("tab_display")),
                (
                    DetailTab::Network,
                    icons::NETWORK,
                    self.i18n.t("tab_network"),
                ),
                (
                    DetailTab::Console,
                    icons::CONSOLE,
                    self.i18n.t("tab_console"),
                ),
            ];

            for (tab, icon, title) in tab_items {
                let is_selected = *self.active_tab == tab;
                let text = format!("{} {}", icon, title);
                let btn = if is_selected {
                    egui::Button::new(egui::RichText::new(text).strong().color(Color32::WHITE))
                        .fill(COLOR_PRIMARY)
                        .min_size(egui::vec2(95.0, 28.0))
                } else {
                    egui::Button::new(egui::RichText::new(text).color(ui.visuals().text_color()))
                        .min_size(egui::vec2(95.0, 28.0))
                };

                if ui.add(btn).clicked() {
                    *self.active_tab = tab;
                }
            }
        });

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(8.0);

        // 3. Active Tab Content
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match *self.active_tab {
                DetailTab::System => self.show_system_tab(ui),
                DetailTab::Storage => self.show_storage_tab(ui),
                DetailTab::Display => self.show_display_tab(ui),
                DetailTab::Network => self.show_network_tab(ui),
                DetailTab::Console => self.show_console_tab(ui),
            });

        // 4. Modal Dialog: Create New Disk Image
        self.show_create_disk_modal(ui.ctx());

        action
    }

    /// Tab 5: Live Console Output Stream
    fn show_console_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.i18n.t("tab_console"));
        ui.add_space(6.0);

        if let Some(logs) = &mut self.console_logs {
            let mut view = ConsoleView::new(logs, self.i18n)
                .is_running(self.is_running)
                .pid(self.pid)
                .cli_command(self.cli_command);
            view.show(ui);
        } else {
            card_frame(ui.visuals()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(icons::CONSOLE).size(24.0).color(COLOR_PRIMARY));
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("QEMU Live Monitor & Console Output").strong());
                    if self.is_running {
                        ui.label(
                            egui::RichText::new("VM is actively running. Real-time stdout and stderr logs are captured.")
                                .color(COLOR_RUNNING)
                                .size(12.0),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("VM is stopped. Launch the VM to view real-time QEMU output streams.")
                                .color(ui.visuals().weak_text_color())
                                .size(12.0),
                        );
                    }
                });
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            let frame = egui::Frame::none()
                .fill(ui.visuals().extreme_bg_color)
                .stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                .rounding(egui::Rounding::same(ROUNDING_BUTTON))
                .inner_margin(egui::Margin::same(10.0));

            frame.show(ui, |ui| {
                ui.set_min_height(140.0);
                if self.is_running {
                    ui.label(
                        egui::RichText::new(format!(
                            "[FARO QEMU MANAGER] Virtual Machine '{}' is running.\nProcess supervisor is streaming output in background...",
                            self.vm.name
                        ))
                        .monospace()
                        .size(12.0)
                        .color(COLOR_RUNNING),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(format!(
                            "[FARO QEMU MANAGER] Virtual Machine '{}' is stopped.\nClick 'Launch VM' to initialize hardware emulation.",
                            self.vm.name
                        ))
                        .monospace()
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                    );
                }
            });
        });
        }
    }
}
