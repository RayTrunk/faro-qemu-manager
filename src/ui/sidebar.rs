// FARO QEM (C) R.Trunk 2026
// Sidebar navigation and VM list component

use egui::{Color32, Margin, Stroke};
use std::collections::HashMap;
use uuid::Uuid;

use crate::config::{Arch, VmConfig};
use crate::i18n::I18n;
use crate::ui::theme::{
    card_frame, danger_button, icons, status_badge, status_badge_interactive, COLOR_ACCENT,
    COLOR_DARK_SURFACE_HOVER, COLOR_ERROR, COLOR_LIGHT_SURFACE_HOVER, COLOR_PRIMARY, COLOR_RUNNING,
    COLOR_STOPPED,
};

/// Actions emitted by the sidebar navigation and toolbar
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarAction {
    SelectVm(Uuid),
    NewVm,
    DuplicateVm(Uuid),
    DeleteVm(Uuid),
    OpenSettings,
}

/// Execution status of a virtual machine
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum VmStatus {
    #[default]
    Stopped,
    Running {
        pid: u32,
    },
    Error(String),
}

impl VmStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, VmStatus::Running { .. })
    }

    pub fn pid(&self) -> Option<u32> {
        match self {
            VmStatus::Running { pid } => Some(*pid),
            _ => None,
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, VmStatus::Error(_))
    }
}

impl std::fmt::Display for VmStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmStatus::Stopped => write!(f, "Stopped"),
            VmStatus::Running { pid } => write!(f, "Running (PID: {})", pid),
            VmStatus::Error(err) => write!(f, "Error: {}", err),
        }
    }
}

/// Sidebar view state and rendering logic
pub struct SidebarView<'a> {
    pub vms: &'a [VmConfig],
    pub selected_vm_id: Option<Uuid>,
    pub search_query: &'a mut String,
    pub vm_statuses: &'a HashMap<Uuid, VmStatus>,
    pub i18n: &'a I18n,
    pub confirming_delete_id: Option<&'a mut Option<Uuid>>,
    pub logo: Option<&'a egui::TextureHandle>,
}

impl<'a> SidebarView<'a> {
    /// Creates a new `SidebarView` without delete confirmation tracking
    pub fn new(
        vms: &'a [VmConfig],
        selected_vm_id: Option<Uuid>,
        search_query: &'a mut String,
        vm_statuses: &'a HashMap<Uuid, VmStatus>,
        i18n: &'a I18n,
    ) -> Self {
        Self {
            vms,
            selected_vm_id,
            search_query,
            vm_statuses,
            i18n,
            confirming_delete_id: None,
            logo: None,
        }
    }

    /// Attach an optional logo texture to display in the header
    pub fn with_logo(mut self, logo: Option<&'a egui::TextureHandle>) -> Self {
        self.logo = logo;
        self
    }

    /// Attach a mutable reference to track delete confirmation modal state
    pub fn with_delete_confirmation(mut self, confirm: &'a mut Option<Uuid>) -> Self {
        self.confirming_delete_id = Some(confirm);
        self
    }

    /// Render the sidebar and return any action triggered by user interaction
    pub fn show(mut self, ui: &mut egui::Ui) -> Option<SidebarAction> {
        let mut action: Option<SidebarAction> = None;

        // 1. Header: "FARO QEM" with subtitle "(C) R.Trunk 2026" and Logo
        ui.horizontal(|ui| {
            if let Some(logo) = self.logo {
                ui.image((logo.id(), egui::vec2(36.0, 36.0)));
            }
            ui.vertical(|ui| {
                ui.heading(
                    egui::RichText::new("FARO QEM")
                        .size(18.0)
                        .strong()
                        .color(COLOR_PRIMARY),
                );
                ui.label(
                    egui::RichText::new("(C) R.Trunk 2026")
                        .size(11.0)
                        .color(ui.visuals().weak_text_color()),
                );
            });
        });

        ui.add_space(6.0);

        // 2. Search filter input with clear button
        ui.horizontal(|ui| {
            ui.label(icons::SEARCH);
            let has_query = !self.search_query.is_empty();
            let clear_btn_width = if has_query { 24.0 } else { 0.0 };
            let desired_width = (ui.available_width() - clear_btn_width).max(80.0);

            ui.add(
                egui::TextEdit::singleline(self.search_query)
                    .hint_text(self.i18n.t("search_placeholder"))
                    .desired_width(desired_width),
            );

            if has_query
                && ui
                    .small_button(icons::CLOSE)
                    .on_hover_text(self.i18n.t("clear"))
                    .clicked()
            {
                self.search_query.clear();
            }
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        // 3. Bottom Toolbar & Scrollable VM list
        // Dock toolbar firmly at the bottom using bottom_up layout so it never overlaps
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(4.0);

            // Bottom Toolbar
            ui.horizontal_wrapped(|ui| {
                // [+ New VM]
                let new_btn =
                    egui::Button::new(format!("{} {}", icons::ADD, self.i18n.t("new_vm")))
                        .fill(COLOR_PRIMARY);
                if ui.add(new_btn).clicked() {
                    action = Some(SidebarAction::NewVm);
                }

                // [Duplicate] (enabled if VM selected)
                let can_dup = self.selected_vm_id.is_some();
                let dup_btn =
                    egui::Button::new(format!("{} {}", icons::EDIT, self.i18n.t("duplicate")));
                if ui.add_enabled(can_dup, dup_btn).clicked() {
                    if let Some(id) = self.selected_vm_id {
                        action = Some(SidebarAction::DuplicateVm(id));
                    }
                }

                // [Delete] (enabled if VM selected)
                let can_del = self.selected_vm_id.is_some();
                let del_btn =
                    egui::Button::new(format!("{} {}", icons::DELETE, self.i18n.t("delete"))).fill(
                        if can_del {
                            COLOR_ERROR.linear_multiply(0.2)
                        } else {
                            Color32::TRANSPARENT
                        },
                    );
                if ui.add_enabled(can_del, del_btn).clicked() {
                    if let Some(id) = self.selected_vm_id {
                        if let Some(ref mut confirm_slot) = self.confirming_delete_id {
                            **confirm_slot = Some(id);
                        } else {
                            action = Some(SidebarAction::DeleteVm(id));
                        }
                    }
                }

                // [⚙ Settings]
                let set_btn =
                    egui::Button::new(format!("{} {}", icons::SETTINGS, self.i18n.t("settings")));
                if ui.add(set_btn).clicked() {
                    action = Some(SidebarAction::OpenSettings);
                }
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Scrollable VM list fills all remaining height without overlapping
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let query = self.search_query.trim().to_lowercase();
                        let filtered_vms: Vec<&VmConfig> = self
                            .vms
                            .iter()
                            .filter(|vm| {
                                if query.is_empty() {
                                    true
                                } else {
                                    vm.name.to_lowercase().contains(&query)
                                        || vm.arch.as_str().to_lowercase().contains(&query)
                                }
                            })
                            .collect();

                        if filtered_vms.is_empty() {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new(self.i18n.t("no_vms_found"))
                                        .color(ui.visuals().weak_text_color())
                                        .italics(),
                                );
                            });
                        } else {
                            for vm in filtered_vms {
                                let is_selected = self.selected_vm_id == Some(vm.id);
                                let status = self
                                    .vm_statuses
                                    .get(&vm.id)
                                    .cloned()
                                    .unwrap_or(VmStatus::Stopped);

                                let mut frame = card_frame(ui.visuals())
                                    .inner_margin(Margin::symmetric(10.0, 8.0));

                                if is_selected {
                                    frame = frame.stroke(Stroke::new(2.0, COLOR_PRIMARY));
                                    if ui.visuals().dark_mode {
                                        frame = frame.fill(COLOR_DARK_SURFACE_HOVER);
                                    } else {
                                        frame = frame.fill(COLOR_LIGHT_SURFACE_HOVER);
                                    }
                                }

                                let card_resp = frame
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            // Architecture / OS icon
                                            let (arch_icon, icon_color) = match vm.arch {
                                                Arch::X86_64 => (icons::VM, COLOR_PRIMARY),
                                                Arch::I386 => (icons::VM, COLOR_ACCENT),
                                                Arch::Aarch64 => (icons::CPU, COLOR_PRIMARY),
                                                Arch::Arm => (icons::CPU, COLOR_ACCENT),
                                                Arch::Riscv64 => (icons::CONSOLE, COLOR_ACCENT),
                                            };
                                            ui.label(
                                                egui::RichText::new(arch_icon)
                                                    .size(18.0)
                                                    .color(icon_color),
                                            );

                                            ui.vertical(|ui| {
                                                // Row 1: VM name & status badge
                                                ui.horizontal(|ui| {
                                                    let name_text = if is_selected {
                                                        egui::RichText::new(&vm.name)
                                                            .strong()
                                                            .color(COLOR_PRIMARY)
                                                    } else {
                                                        egui::RichText::new(&vm.name).strong()
                                                    };
                                                    ui.label(name_text);

                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| match &status {
                                                            VmStatus::Running { pid } => {
                                                                status_badge(
                                                                    ui,
                                                                    &format!(
                                                                        "{} {}",
                                                                        icons::PLAY,
                                                                        pid
                                                                    ),
                                                                    COLOR_RUNNING,
                                                                );
                                                            }
                                                            VmStatus::Stopped => {
                                                                status_badge(
                                                                    ui,
                                                                    self.i18n.t("status_stopped"),
                                                                    COLOR_STOPPED,
                                                                );
                                                            }
                                                            VmStatus::Error(err) => {
                                                                let badge_resp =
                                                                    status_badge_interactive(
                                                                        ui,
                                                                        self.i18n.t("status_error"),
                                                                        COLOR_ERROR,
                                                                    );
                                                                if !err.is_empty() {
                                                                    badge_resp.on_hover_text(
                                                                        err.as_str(),
                                                                    );
                                                                }
                                                            }
                                                        },
                                                    );
                                                });

                                                // Row 2: Specs preview (e.g. "x86_64 • 4 cores, 4096 MB")
                                                let core_label = if vm.cpu_cores == 1 {
                                                    "core"
                                                } else {
                                                    "cores"
                                                };
                                                let specs_text = format!(
                                                    "{} • {} {}, {} MB",
                                                    vm.arch.as_str(),
                                                    vm.cpu_cores,
                                                    core_label,
                                                    vm.memory_mb
                                                );
                                                ui.label(
                                                    egui::RichText::new(specs_text)
                                                        .size(11.0)
                                                        .color(ui.visuals().weak_text_color()),
                                                );
                                            });
                                        });
                                    })
                                    .response;

                                let interact = ui.interact(
                                    card_resp.rect,
                                    ui.make_persistent_id(vm.id),
                                    egui::Sense::click(),
                                );
                                if interact.clicked() {
                                    action = Some(SidebarAction::SelectVm(vm.id));
                                }
                                if interact.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                ui.add_space(4.0);
                            }
                        }
                    });
            });
        });
        // 5. Delete Confirmation Modal Dialog
        if let Some(ref mut confirm_slot) = self.confirming_delete_id {
            if let Some(del_id) = **confirm_slot {
                let vm_name = self
                    .vms
                    .iter()
                    .find(|v| v.id == del_id)
                    .map(|v| v.name.clone())
                    .unwrap_or_else(|| "Virtual Machine".to_string());

                let mut close_modal = false;
                egui::Window::new(format!("{} {}", icons::WARNING, self.i18n.t("delete")))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        ui.set_min_width(320.0);
                        ui.add_space(6.0);
                        ui.label(format!(
                            "{} '{}'?",
                            self.i18n.t("delete_confirm_msg"),
                            vm_name
                        ));
                        ui.label(
                            egui::RichText::new("This action will remove the VM configuration.")
                                .size(11.0)
                                .color(COLOR_ERROR),
                        );
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if danger_button(
                                ui,
                                format!("{} {}", icons::DELETE, self.i18n.t("delete")),
                            )
                            .clicked()
                            {
                                action = Some(SidebarAction::DeleteVm(del_id));
                                close_modal = true;
                            }
                            if ui.button(self.i18n.t("cancel")).clicked() {
                                close_modal = true;
                            }
                        });
                        ui.add_space(4.0);
                    });

                if close_modal {
                    **confirm_slot = None;
                }
            }
        }

        action
    }
}
