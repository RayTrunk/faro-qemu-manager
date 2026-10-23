// FARO QEMU MANAGER (C) R.Trunk 2026
// The template wizard: one dialog, two clicks in the normal case.

use std::path::{Path, PathBuf};

use crate::config::{Accelerator, Arch, DriveInterface, Firmware, VmConfig};
use crate::i18n::I18n;
use crate::qemu::QemuCapabilities;
use crate::templates::VmTemplate;
use crate::ui::theme::{card_frame, icons, COLOR_ERROR, COLOR_WARNING};
use crate::ui::vm_detail::{
    offered_accelerators, offered_machine_types, offered_netdev_backends, offered_vga_types,
};

/// What the wizard reports back when it closes.
///
/// The two variants differ a lot in size, which clippy notices. Boxing would
/// add an allocation and a dereference to a value that exists for exactly one
/// function return before being moved into the VM list.
#[allow(clippy::large_enum_variant)]
pub enum WizardAction {
    /// Create this machine. Nothing has been written yet: the caller claims
    /// the folder, makes the disk and the variable store, and only then adds
    /// the machine to the list.
    Create {
        vm: VmConfig,
        /// Size of the disk image to make, in GB. The configuration itself
        /// carries no size — a disk is simply a file afterwards — so it
        /// travels with the request that creates one.
        disk_gb: u32,
    },
    /// Closed without creating anything.
    Cancel,
}

/// Disk size for a machine started from the empty tile, which names none.
pub const DEFAULT_DISK_GB: u32 = 32;

/// Everything the wizard remembers between frames.
pub struct WizardState {
    pub open: bool,
    /// Key of the highlighted template tile. `None` means the empty tile, or
    /// that nothing has been picked yet — [`WizardState::draft`] tells those
    /// two apart.
    pub selected: Option<&'static str>,
    pub name: String,
    pub iso: String,
    pub hardware_open: bool,
    pub network_open: bool,
    /// The machine as it stands. Built when a tile is clicked, then edited in
    /// place.
    pub draft: Option<VmConfig>,
    /// Size of the disk image to make. Not part of the configuration, which
    /// only records where the disk is.
    pub disk_gb: u32,
}

impl Default for WizardState {
    fn default() -> Self {
        Self {
            open: false,
            selected: None,
            name: String::new(),
            iso: String::new(),
            hardware_open: false,
            network_open: false,
            draft: None,
            disk_gb: DEFAULT_DISK_GB,
        }
    }
}

impl WizardState {
    /// Start over from this template.
    ///
    /// Picking a template replaces the draft rather than merging into it: two
    /// starting assignments laid over each other are neither.
    pub fn select(&mut self, key: &str) {
        if let Some(template) = VmTemplate::by_key(key) {
            self.selected = Some(template.key);
            self.disk_gb = template.disk_gb;
            self.draft = Some(template.instantiate(&self.name, Path::new("")));
        }
    }

    /// Start from nothing, which is what the direct way gives.
    pub fn select_empty(&mut self, arch: Arch) {
        self.selected = None;
        self.disk_gb = DEFAULT_DISK_GB;
        self.draft = Some(VmConfig::new_default(&self.name, arch));
    }

    /// Catalogue key of the reason creation cannot happen yet, if any.
    pub fn why_not_create(&self) -> Option<&'static str> {
        if self.draft.is_none() {
            return Some("wizard_needs_template");
        }
        if self.name.trim().is_empty() {
            return Some("wizard_needs_name");
        }
        None
    }

    /// Hand out the finished draft and close.
    ///
    /// The name comes from the field as it stands now, not from when the tile
    /// was clicked — which is usually before anyone has typed one.
    pub fn take_draft(&mut self) -> Option<VmConfig> {
        let mut vm = self.draft.take()?;
        vm.name = self.name.trim().to_string();
        if !self.iso.trim().is_empty() {
            vm.cdroms = vec![PathBuf::from(self.iso.trim())];
        }
        self.open = false;
        self.selected = None;
        self.name.clear();
        self.iso.clear();
        Some(vm)
    }

    /// Close without creating anything.
    fn dismiss(&mut self) {
        self.open = false;
        self.selected = None;
        self.draft = None;
        self.name.clear();
        self.iso.clear();
    }

    fn template(&self) -> Option<&'static VmTemplate> {
        self.selected.and_then(VmTemplate::by_key)
    }
}

/// The wizard dialog. Draws; creates nothing.
pub struct WizardDialog<'a> {
    state: &'a mut WizardState,
    i18n: &'a I18n,
    capabilities: Option<&'a QemuCapabilities>,
    uefi_available: bool,
}

impl<'a> WizardDialog<'a> {
    pub fn new(state: &'a mut WizardState, i18n: &'a I18n) -> Self {
        Self {
            state,
            i18n,
            capabilities: None,
            uefi_available: true,
        }
    }

    pub fn with_capabilities(mut self, caps: &'a QemuCapabilities) -> Self {
        self.capabilities = Some(caps);
        self
    }

    /// Whether the UEFI firmware pair was found. Unknown counts as present,
    /// the same way an unprobed capability permits everything.
    pub fn with_firmware(mut self, available: bool) -> Self {
        self.uefi_available = available;
        self
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<WizardAction> {
        if !self.state.open {
            return None;
        }
        let mut action = None;
        let mut open = true;

        egui::Window::new(self.i18n.t("wizard_title"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(620.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                self.gallery(ui);
                ui.add_space(8.0);
                self.always_visible_fields(ui);
                ui.add_space(8.0);
                self.hardware_block(ui);
                self.network_block(ui);
                self.notices(ui);
                ui.add_space(8.0);
                ui.separator();
                action = self.buttons(ui);
            });

        if !open && action.is_none() {
            self.state.dismiss();
            return Some(WizardAction::Cancel);
        }
        action
    }

    /// The tile gallery: six templates plus one empty tile.
    fn gallery(&mut self, ui: &mut egui::Ui) {
        let empty_label = self.i18n.t("wizard_empty_tile").to_string();
        let state = &mut *self.state;
        card_frame(ui.visuals()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for template in VmTemplate::all() {
                    let picked = state.selected == Some(template.key);
                    if ui.selectable_label(picked, template.name).clicked() {
                        state.select(template.key);
                    }
                }
                let empty_picked = state.selected.is_none() && state.draft.is_some();
                if ui.selectable_label(empty_picked, &empty_label).clicked() {
                    let arch = state.draft.as_ref().map(|d| d.arch).unwrap_or(Arch::X86_64);
                    state.select_empty(arch);
                }
            });
        });
    }

    /// Name and installation ISO: the two fields always on screen.
    fn always_visible_fields(&mut self, ui: &mut egui::Ui) {
        let name_label = self.i18n.t("vm_name").to_string();
        let iso_label = self.i18n.t("iso_path").to_string();
        let browse = format!("{} {}", icons::FOLDER, self.i18n.t("browse"));
        let state = &mut *self.state;

        egui::Grid::new("wizard_fields")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new(&name_label).strong());
                ui.add(
                    egui::TextEdit::singleline(&mut state.name)
                        .desired_width(380.0)
                        .hint_text(&name_label),
                );
                ui.end_row();

                ui.label(egui::RichText::new(&iso_label).strong());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.iso)
                            .desired_width(260.0)
                            .hint_text(&iso_label),
                    );
                    if ui.button(&browse).clicked() {
                        if let Some(file) = rfd::FileDialog::new()
                            .add_filter(self.i18n.t("filter_iso"), &["iso", "img"])
                            .pick_file()
                        {
                            state.iso = file.to_string_lossy().to_string();
                        }
                    }
                });
                ui.end_row();
            });
    }

    /// Hardware and firmware, collapsed, with its core values in the header.
    fn hardware_block(&mut self, ui: &mut egui::Ui) {
        let header = format!(
            "{}  -  {}",
            self.i18n.t("wizard_hardware_block"),
            self.hardware_summary()
        );
        let labels: Vec<String> = [
            "cores",
            "memory_mb",
            "accel",
            "machine",
            "vga_type",
            "interface",
            "size_gb",
        ]
        .iter()
        .map(|k| self.i18n.t(k).to_string())
        .collect();
        let caps = self.capabilities;
        let was_open = self.state.hardware_open;
        let mut disk_gb = self.state.disk_gb;
        let draft = &mut self.state.draft;

        let response = egui::CollapsingHeader::new(header)
            .id_salt("wizard_hardware")
            .open(Some(was_open))
            .show(ui, |ui| {
                let Some(draft) = draft.as_mut() else {
                    return;
                };
                egui::Grid::new("wizard_hw_grid")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(&labels[0]);
                        ui.add(egui::DragValue::new(&mut draft.cpu_cores).range(1..=64));
                        ui.end_row();

                        ui.label(&labels[1]);
                        ui.add(
                            egui::DragValue::new(&mut draft.memory_mb)
                                .range(128..=1_048_576)
                                .speed(64.0),
                        );
                        ui.end_row();

                        ui.label(&labels[2]);
                        egui::ComboBox::from_id_salt("wizard_accel")
                            .selected_text(draft.accel.as_str().to_uppercase())
                            .show_ui(ui, |ui| {
                                for acc in offered_accelerators() {
                                    ui.selectable_value(
                                        &mut draft.accel,
                                        acc,
                                        acc.as_str().to_uppercase(),
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label(&labels[3]);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut draft.machine_type)
                                    .desired_width(120.0),
                            );
                            for machine in offered_machine_types(draft.arch, caps) {
                                if ui.small_button(machine).clicked() {
                                    draft.machine_type = machine.to_string();
                                }
                            }
                        });
                        ui.end_row();

                        ui.label(&labels[4]);
                        egui::ComboBox::from_id_salt("wizard_vga")
                            .selected_text(&draft.display.vga_type)
                            .show_ui(ui, |ui| {
                                for opt in offered_vga_types(draft.arch) {
                                    ui.selectable_value(
                                        &mut draft.display.vga_type,
                                        opt.to_string(),
                                        opt,
                                    );
                                }
                            });
                        ui.end_row();

                        if let Some(drive) = draft.drives.first_mut() {
                            ui.label(&labels[5]);
                            egui::ComboBox::from_id_salt("wizard_iface")
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
                            ui.end_row();

                            ui.label(&labels[6]);
                            ui.add(egui::DragValue::new(&mut disk_gb).range(1..=4096));
                            ui.end_row();
                        }
                    });
            });

        self.state.disk_gb = disk_gb;
        if response.header_response.clicked() {
            self.state.hardware_open = !was_open;
        }
    }

    /// Network and port forwarding, collapsed, with its core values shown.
    fn network_block(&mut self, ui: &mut egui::Ui) {
        let header = format!(
            "{}  -  {}",
            self.i18n.t("wizard_network_block"),
            self.network_summary()
        );
        let backend_label = self.i18n.t("backend").to_string();
        let nic_label = self.i18n.t("nic_model").to_string();
        let forwards_warning = self.i18n.t("forwards_backend_only").to_string();
        let caps = self.capabilities;
        let was_open = self.state.network_open;
        let draft = &mut self.state.draft;

        let response = egui::CollapsingHeader::new(header)
            .id_salt("wizard_network")
            .open(Some(was_open))
            .show(ui, |ui| {
                let Some(draft) = draft.as_mut() else {
                    return;
                };
                egui::Grid::new("wizard_net_grid")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(&backend_label);
                        egui::ComboBox::from_id_salt("wizard_backend")
                            .selected_text(&draft.network.backend)
                            .show_ui(ui, |ui| {
                                for opt in offered_netdev_backends(caps) {
                                    ui.selectable_value(
                                        &mut draft.network.backend,
                                        opt.to_string(),
                                        opt,
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label(&nic_label);
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.network.nic_model)
                                .desired_width(180.0),
                        );
                        ui.end_row();
                    });

                if !draft.network.port_forwards.is_empty()
                    && !draft.network.backend.eq_ignore_ascii_case("user")
                {
                    ui.label(
                        egui::RichText::new(&forwards_warning)
                            .size(11.0)
                            .color(COLOR_WARNING),
                    );
                }
            });

        if response.header_response.clicked() {
            self.state.network_open = !was_open;
        }
    }

    /// Template-specific warnings, and anything that would stop a start.
    fn notices(&mut self, ui: &mut egui::Ui) {
        if let Some(key) = self.state.template().and_then(|t| t.notice_key) {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(self.i18n.t(key))
                    .size(11.0)
                    .color(COLOR_WARNING),
            );
        }
        let wants_uefi = self
            .state
            .draft
            .as_ref()
            .is_some_and(|d| matches!(d.firmware, Firmware::Uefi { .. }));
        if wants_uefi && !self.uefi_available {
            ui.label(
                egui::RichText::new(self.i18n.t("firmware_missing"))
                    .size(11.0)
                    .color(COLOR_ERROR),
            );
        }
    }

    /// Create, with the reason beside it when it cannot fire yet, and Cancel.
    fn buttons(&mut self, ui: &mut egui::Ui) -> Option<WizardAction> {
        let mut action = None;
        let blocked = self.state.why_not_create();
        let create_label = format!("{} {}", icons::ADD, self.i18n.t("create"));
        let cancel_label = self.i18n.t("cancel").to_string();
        let reason = blocked.map(|key| self.i18n.t(key).to_string());
        let disk_gb = self.state.disk_gb;
        let state = &mut *self.state;

        ui.horizontal(|ui| {
            if ui
                .add_enabled(blocked.is_none(), egui::Button::new(&create_label))
                .clicked()
            {
                if let Some(vm) = state.take_draft() {
                    action = Some(WizardAction::Create { vm, disk_gb });
                }
            }
            if ui.button(&cancel_label).clicked() {
                state.dismiss();
                action = Some(WizardAction::Cancel);
            }
            // A disabled button without a reason is a dead end.
            if let Some(reason) = reason {
                ui.label(
                    egui::RichText::new(reason)
                        .size(11.0)
                        .color(ui.visuals().weak_text_color()),
                );
            }
        });
        action
    }

    /// The hardware values a glance should show without opening the block.
    fn hardware_summary(&self) -> String {
        let Some(draft) = self.state.draft.as_ref() else {
            return String::new();
        };
        let firmware = match draft.firmware {
            Firmware::Uefi { .. } => "UEFI",
            Firmware::Bios { .. } => "BIOS",
        };
        let disk = if draft.drives.is_empty() {
            String::new()
        } else {
            format!(", {} GB", self.state.disk_gb)
        };
        let interface = draft
            .drives
            .first()
            .map(|d| format!(", {}", d.interface.as_str()))
            .unwrap_or_default();
        let accel = if draft.accel == Accelerator::Auto {
            String::new()
        } else {
            format!(", {}", draft.accel.as_str())
        };
        format!(
            "{} x {}, {} MB{}{}, {}{}",
            draft.cpu_cores, draft.cpu_model, draft.memory_mb, disk, interface, firmware, accel
        )
    }

    /// The network values a glance should show without opening the block.
    fn network_summary(&self) -> String {
        let Some(draft) = self.state.draft.as_ref() else {
            return String::new();
        };
        let forwards = draft.network.port_forwards.len();
        if forwards == 0 {
            format!("{}, {}", draft.network.backend, draft.network.nic_model)
        } else {
            format!(
                "{}, {}, {} x {}",
                draft.network.backend,
                draft.network.nic_model,
                forwards,
                self.i18n.t("port_forwards")
            )
        }
    }
}
