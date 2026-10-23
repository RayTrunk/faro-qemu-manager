// FARO QEMU MANAGER (C) R.Trunk 2026
// VM Detail Tabs and Hardware Controls component

use egui::{Color32, Stroke};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::config::{
    Accelerator, Arch, DriveConfig, DriveFormat, DriveInterface, Firmware, PortForwardRule,
    VmConfig,
};
use crate::i18n::I18n;
use crate::qemu::{create_disk_image, find_qemu_img, find_uefi_firmware, QemuCapabilities};
use crate::ui::console::{ConsoleLogBuffer, ConsoleView};
use crate::ui::theme::{
    card_frame, danger_button, icons, section_header, status_badge, COLOR_ERROR, COLOR_PRIMARY,
    COLOR_RUNNING, COLOR_STOPPED, ROUNDING_BUTTON,
};

/// Network backends worth offering for these capabilities.
///
/// `None` means QEMU was never asked, and then everything stays on offer:
/// hiding an option because we failed to ask would claim it is unavailable
/// when we do not know. `none` is ours rather than QEMU's, so it is always in.
pub fn offered_netdev_backends(caps: Option<&QemuCapabilities>) -> Vec<&'static str> {
    ["user", "tap", "bridge"]
        .into_iter()
        .filter(|b| caps.is_none_or(|c| c.has_netdev(b)))
        .chain(std::iter::once("none"))
        .collect()
}

/// Machine types worth suggesting for this architecture.
///
/// The old fixed list offered `pc-i440fx`, which QEMU rejects outright — only
/// versioned names exist. Anything a probe does not confirm is dropped.
pub fn offered_machine_types(arch: Arch, caps: Option<&QemuCapabilities>) -> Vec<&'static str> {
    let candidates: &[&'static str] = match arch {
        Arch::X86_64 | Arch::I386 => &["q35", "microvm"],
        Arch::Aarch64 | Arch::Arm => &["virt"],
        Arch::Riscv64 => &["virt", "sifive_u"],
    };
    candidates
        .iter()
        .copied()
        .filter(|m| caps.is_none_or(|c| c.has_machine(m)))
        .collect()
}

/// Accelerators worth offering on this platform.
///
/// HAXM is absent from modern Windows QEMU builds and used to be bent silently
/// onto WHPX. An option that does something other than it says is worse than
/// no option at all.
pub fn offered_accelerators() -> Vec<Accelerator> {
    Accelerator::all()
        .iter()
        .copied()
        .filter(|a| !(cfg!(target_os = "windows") && *a == Accelerator::HAXM))
        .collect()
}

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
        }
    }

    /// Attach what the installed QEMU can do, so the view offers only that.
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

    /// Tab 1: System & Hardware
    fn show_system_tab(&mut self, ui: &mut egui::Ui) {
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
                        let hint = match self.vm.accel {
                            Accelerator::Auto => {
                                if cfg!(target_os = "windows") {
                                    "(Auto: WHPX on Windows)"
                                } else if cfg!(target_os = "linux") {
                                    "(Auto: KVM on Linux)"
                                } else {
                                    "(Auto: TCG Emulation)"
                                }
                            }
                            Accelerator::WHPX => "(Windows Hypervisor Platform)",
                            Accelerator::HAXM => "(Intel Hardware Acceleration)",
                            Accelerator::KVM => "(Kernel-based Virtual Machine)",
                            Accelerator::TCG => "(Tiny Code Generator software emulation)",
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
                                // The variable store lives beside the VM's first
                                // disk, so it travels with the machine.
                                let nvram = self
                                    .vm
                                    .drives
                                    .first()
                                    .and_then(|d| d.path.parent().map(|p| p.to_path_buf()))
                                    .unwrap_or_default()
                                    .join(format!("{}-nvram.fd", self.vm.id));
                                self.vm.firmware = Firmware::Uefi { nvram };
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
                                            .add_filter("Firmware Images", &["fd", "bin", "rom"])
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
                                if find_uefi_firmware(None).is_none() {
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

    /// Tab 2: Storage & Media
    fn show_storage_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.i18n.t("tab_storage"));
        ui.add_space(6.0);

        // Section 1: Virtual Hard Disks
        card_frame(ui.visuals()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(self.i18n.t("disks"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // [+ Add Existing Disk]
                    if ui.button(format!("{} {}", icons::ADD, self.i18n.t("add_disk"))).clicked() {
                        let dialog = rfd::FileDialog::new()
                            .add_filter("Virtual Disk Images", &["qcow2", "img", "raw", "vmdk", "vdi", "vhdx"]);
                        if let Some(file) = dialog.pick_file() {
                            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
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
                    let create_modal_btn = egui::Button::new(format!("{} {}", icons::ADD, self.i18n.t("create_disk")))
                        .fill(COLOR_PRIMARY);
                    if ui.add(create_modal_btn).clicked() {
                        let default_name = format!("{}.qcow2", self.vm.name.replace(' ', "_").to_lowercase());
                        self.disk_modal.open_dialog(&default_name);
                    }
                });
            });

            ui.add_space(6.0);

            if self.vm.drives.is_empty() {
                ui.label(
                    egui::RichText::new("No virtual hard disks attached. Click 'Add Disk' or 'Create Disk' to attach storage.")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                        .italics(),
                );
            } else {
                let mut remove_idx = None;
                for (idx, drive) in self.vm.drives.iter_mut().enumerate() {
                    card_frame(ui.visuals()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icons::DISK).size(16.0).color(COLOR_PRIMARY));
                            let file_name = drive
                                .path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("drive");
                            ui.label(egui::RichText::new(format!("Drive #{}: {}", idx + 1, file_name)).strong());

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if danger_button(ui, format!("{} {}", icons::DELETE, self.i18n.t("remove"))).clicked() {
                                    remove_idx = Some(idx);
                                }
                            });
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
                                        ui.selectable_value(&mut drive.interface, *iface, iface.as_str());
                                    }
                                });

                            ui.add_space(8.0);

                            // Boot Index
                            let mut has_boot = drive.boot_index.is_some();
                            if ui.checkbox(&mut has_boot, self.i18n.t("boot_index")).changed() {
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
                    if ui.button(format!("{} {}", icons::ADD, self.i18n.t("add_cdrom"))).clicked() {
                        let dialog = rfd::FileDialog::new()
                            .add_filter("ISO Images", &["iso", "img"]);
                        if let Some(file) = dialog.pick_file() {
                            self.vm.cdroms.push(file);
                        }
                    }
                });
            });

            ui.add_space(6.0);

            if self.vm.cdroms.is_empty() {
                ui.label(
                    egui::RichText::new("No CD/DVD ISO image attached. Click 'Add CD/DVD ISO' to mount install media.")
                        .size(12.0)
                        .color(ui.visuals().weak_text_color())
                        .italics(),
                );
            } else {
                let mut remove_cdrom_idx = None;
                for (idx, cdrom) in self.vm.cdroms.iter_mut().enumerate() {
                    card_frame(ui.visuals()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(icons::DISK).size(16.0).color(COLOR_PRIMARY));
                            let file_name = cdrom
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("cdrom.iso");
                            ui.label(egui::RichText::new(format!("CD/DVD #{}: {}", idx + 1, file_name)).strong());

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if danger_button(ui, format!("{} {}", icons::CLOSE, self.i18n.t("eject"))).clicked() {
                                    remove_cdrom_idx = Some(idx);
                                }
                                if ui.button(format!("{} {}", icons::FOLDER, self.i18n.t("browse"))).clicked() {
                                    let mut dialog = rfd::FileDialog::new()
                                        .add_filter("ISO Images", &["iso", "img"]);
                                    if let Some(parent) = cdrom.parent() {
                                        dialog = dialog.set_directory(parent);
                                    }
                                    if let Some(new_iso) = dialog.pick_file() {
                                        *cdrom = new_iso;
                                    }
                                }
                            });
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

    /// Tab 3: Display & Graphics
    fn show_display_tab(&mut self, ui: &mut egui::Ui) {
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
                        let vga_options = ["virtio-vga", "std", "qxl", "cirrus", "none"];
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
                            "virtio-vga" => "(VirtIO 2D/3D hardware-accelerated graphics)",
                            "std" => "(Standard VESA 2.0 compatible graphic card)",
                            "qxl" => "(SPICE QXL para-virtualized graphics card)",
                            "cirrus" => "(Legacy Cirrus CL-GD5446 VGA)",
                            "none" => "(Headless / No graphic card)",
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
                            "default" => "(Default native graphical window)",
                            "gtk" => "(GTK graphical user interface)",
                            "sdl" => "(Simple DirectMedia Layer window)",
                            "none" => "(Headless mode without window)",
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
                        "Start VM window in fullscreen mode",
                    );
                    ui.end_row();
                });
        });
    }

    /// Tab 4: Network & Advanced
    fn show_network_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.i18n.t("tab_network"));
        ui.add_space(6.0);

        // Network Hardware Config
        card_frame(ui.visuals()).show(ui, |ui| {
            egui::Grid::new("vm_network_grid")
                .num_columns(2)
                .spacing([20.0, 14.0])
                .show(ui, |ui| {
                    // Backend
                    ui.label(egui::RichText::new(self.i18n.t("backend")).strong());
                    ui.horizontal(|ui| {
                        // "none" is ours, not QEMU's, so it is always on offer.
                        let net_backends = offered_netdev_backends(self.capabilities);
                        egui::ComboBox::from_id_salt("detail_net_backend_combo")
                            .selected_text(&self.vm.network.backend)
                            .show_ui(ui, |ui| {
                                for opt in &net_backends {
                                    ui.selectable_value(
                                        &mut self.vm.network.backend,
                                        opt.to_string(),
                                        *opt,
                                    );
                                }
                            });
                        let hint = match self.vm.network.backend.as_str() {
                            "user" => "(SLIRP NAT user-mode network with Internet access)",
                            "tap" => "(Host TAP virtual network interface)",
                            "bridge" => "(Host bridged interface)",
                            "none" => "(No network connectivity)",
                            _ => "",
                        };
                        ui.label(
                            egui::RichText::new(hint)
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.end_row();

                    // NIC model
                    ui.label(egui::RichText::new(self.i18n.t("nic_model")).strong());
                    ui.horizontal(|ui| {
                        let nic_options =
                            ["virtio-net-pci", "e1000", "rtl8139", "e1000e", "vmxnet3"];
                        egui::ComboBox::from_id_salt("detail_nic_model_combo")
                            .selected_text(&self.vm.network.nic_model)
                            .show_ui(ui, |ui| {
                                for opt in &nic_options {
                                    ui.selectable_value(
                                        &mut self.vm.network.nic_model,
                                        opt.to_string(),
                                        *opt,
                                    );
                                }
                            });
                        let hint = match self.vm.network.nic_model.as_str() {
                            "virtio-net-pci" => "(High performance VirtIO virtual NIC)",
                            "e1000" => "(Intel PRO/1000 Gigabit Ethernet - highly compatible)",
                            "rtl8139" => "(Realtek 8139 Fast Ethernet)",
                            _ => "",
                        };
                        ui.label(
                            egui::RichText::new(hint)
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    ui.end_row();
                });
        });

        ui.add_space(8.0);

        // Port Forwarding Table
        card_frame(ui.visuals()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(self.i18n.t("port_forwards"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(format!("{} {}", icons::ADD, self.i18n.t("add_port_forward"))).clicked() {
                        self.vm.network.port_forwards.push(PortForwardRule {
                            host_port: 2222,
                            guest_port: 22,
                            proto: "tcp".to_string(),
                        });
                    }
                });
            });

            ui.add_space(4.0);

            if self.vm.network.port_forwards.is_empty() {
                ui.label(
                    egui::RichText::new("No port forwarding rules configured. Add rules to forward host ports into VM services (e.g. 2222 -> 22 for SSH, 8080 -> 80 for HTTP).")
                        .size(11.0)
                        .color(ui.visuals().weak_text_color())
                        .italics(),
                );
            } else {
                let mut del_idx = None;
                egui::Grid::new("port_forwards_table_grid")
                    .num_columns(5)
                    .spacing([14.0, 8.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(self.i18n.t("host_port")).strong());
                        ui.label("->");
                        ui.label(egui::RichText::new(self.i18n.t("guest_port")).strong());
                        ui.label(egui::RichText::new(self.i18n.t("proto")).strong());
                        ui.label(egui::RichText::new(self.i18n.t("action")).strong());
                        ui.end_row();

                        for (idx, rule) in self.vm.network.port_forwards.iter_mut().enumerate() {
                            ui.add(egui::DragValue::new(&mut rule.host_port).range(1..=65535));
                            ui.label("->");
                            ui.add(egui::DragValue::new(&mut rule.guest_port).range(1..=65535));

                            let cur_proto = rule.proto.to_lowercase();
                            egui::ComboBox::from_id_salt(format!("proto_combo_{}", idx))
                                .selected_text(cur_proto.to_uppercase())
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut rule.proto, "tcp".to_string(), "TCP");
                                    ui.selectable_value(&mut rule.proto, "udp".to_string(), "UDP");
                                });

                            if danger_button(ui, icons::DELETE).on_hover_text(self.i18n.t("delete")).clicked() {
                                del_idx = Some(idx);
                            }
                            ui.end_row();
                        }
                    });

                if let Some(i) = del_idx {
                    self.vm.network.port_forwards.remove(i);
                }
            }
        });

        ui.add_space(8.0);

        // Extra Arguments Section
        card_frame(ui.visuals()).show(ui, |ui| {
            ui.heading(self.i18n.t("extra_args"));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Additional command-line flags passed directly to QEMU (e.g. -usb -device usb-tablet -audiodev dsound,id=snd0)")
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::multiline(&mut self.vm.extra_args)
                    .hint_text("-usb -device usb-tablet")
                    .desired_rows(3)
                    .desired_width(ui.available_width()),
            );
        });
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

    /// Modal Dialog: Create New Disk Image
    fn show_create_disk_modal(&mut self, ctx: &egui::Context) {
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
                            if ui.button(format!("{} {}", icons::FOLDER, self.i18n.t("browse"))).clicked() {
                                let default_name = format!("{}.qcow2", self.vm.name.replace(' ', "_").to_lowercase());
                                if let Some(save_path) = rfd::FileDialog::new()
                                    .set_file_name(&default_name)
                                    .add_filter("QEMU QCOW2 Disk", &["qcow2"])
                                    .add_filter("Raw Disk Image", &["raw", "img"])
                                    .save_file()
                                {
                                    self.disk_modal.path_buf = save_path.to_string_lossy().to_string();
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
                                    ui.selectable_value(&mut self.disk_modal.format, *f, f.as_str());
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
                    let create_btn = egui::Button::new(format!("{} {}", icons::ADD, self.i18n.t("create")))
                        .fill(COLOR_PRIMARY);
                    if ui.add(create_btn).clicked() {
                        let trimmed = self.disk_modal.path_buf.trim();
                        if trimmed.is_empty() {
                            self.disk_modal.status_msg = Some(Err("Target disk path cannot be empty.".to_string()));
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
                                            self.disk_modal.status_msg = Some(Ok("Disk created successfully!".to_string()));
                                            close_modal = true;
                                        }
                                        Err(e) => {
                                            self.disk_modal.status_msg = Some(Err(e));
                                        }
                                    }
                                }
                                None => {
                                    self.disk_modal.status_msg = Some(Err(
                                        "qemu-img binary not found. Please configure QEMU directory in Settings.".to_string(),
                                    ));
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
