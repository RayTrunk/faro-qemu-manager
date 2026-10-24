// FARO QEMU MANAGER (C) R.Trunk 2026
// The template wizard: one dialog, two clicks in the normal case.

use std::path::{Path, PathBuf};

use crate::config::{Accelerator, Arch, DriveInterface, Firmware, PortForwardRule, VmConfig};
use crate::i18n::I18n;
use crate::qemu::{nvram_path_for, QemuCapabilities};
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

/// Stands in for an image whose filename a mirror has not yet given us.
///
/// It is deliberately a path that does not exist: the start check then
/// reports a missing ISO and holds the start, which is exactly the state the
/// machine is in. Replaced the moment the download reports its target.
pub const PENDING_ISO: &str = "(waiting for the download)";

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
    /// Whether the user asked for the ISO to be fetched. Private, because
    /// the answer is not this flag alone -- see
    /// [`WizardState::wants_download`]. Two sources for one question is how
    /// a withdrawn request came to be honoured anyway.
    download_requested: bool,
    /// Why opening the official page did not work, if it did not. Kept here
    /// rather than in a local so the user can read it.
    pub page_problem: Option<String>,
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
            download_requested: false,
            page_problem: None,
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
            // Starting over includes the download: the new template may not
            // even have a mirror.
            self.download_requested = false;
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
    ///
    /// The dialog closes but keeps everything the user chose and typed,
    /// because the creation can still fail — a first start with no QEMU
    /// directory set is the ordinary case, not an exotic one. The caller
    /// reports the outcome: [`WizardState::creation_failed`] puts the dialog
    /// back as it was, [`WizardState::created`] clears it for the next time.
    pub fn take_draft(&mut self) -> Option<VmConfig> {
        let mut vm = self.draft.clone()?;
        vm.name = self.name.trim().to_string();
        if !self.iso.trim().is_empty() {
            vm.cdroms = vec![PathBuf::from(self.iso.trim())];
        } else if self.download_requested {
            // A machine waiting for an image must not look complete. Until a
            // mirror answers, the real filename is unknown, so this stands
            // in: the start check sees a missing ISO and holds the start,
            // which is the truth. Without it there is a window -- and, if
            // the mirror is unreachable, a permanent state -- where the
            // machine looks ready and would install nothing.
            vm.cdroms = vec![PathBuf::from(PENDING_ISO)];
        }
        self.open = false;
        Some(vm)
    }

    /// The machine was created. Nothing is left to come back to.
    pub fn created(&mut self) {
        self.dismiss();
    }

    /// Never mind, do not fetch it.
    pub fn withdraw_download(&mut self) {
        self.download_requested = false;
    }

    /// Fetch the ISO once the machine exists.
    ///
    /// Only remembered here. Resolving today's filename means talking to a
    /// mirror, and that does not happen while a frame is being drawn.
    pub fn request_download(&mut self) {
        self.download_requested = true;
        // The field is where the answer goes; asking for it to be fetched
        // means it is not named yet.
        self.iso.clear();
    }

    /// Whether the user has asked for the image to be fetched.
    ///
    /// Naming a file withdraws the request: someone who clicks Download and
    /// then browses for the ISO they already have has changed their mind,
    /// and silently fetching one anyway -- over their own choice -- is the
    /// anti-goal from spec section 1.
    pub fn wants_download(&mut self) -> bool {
        if !self.iso.trim().is_empty() {
            self.download_requested = false;
        }
        self.download_requested
    }

    /// Switch the draft between UEFI and BIOS.
    ///
    /// The variable store's path is derived from the disk's folder, which the
    /// draft does not have yet — creation moves the paths and derives it
    /// then. A placeholder here would be a path nothing writes to, so the
    /// store is named after the machine and rebased on creation.
    pub fn set_uefi(&mut self, uefi: bool) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        draft.firmware = if uefi {
            Firmware::Uefi {
                nvram: nvram_path_for(draft),
            }
        } else {
            Firmware::Bios { rom: None }
        };
    }

    /// Whether the draft is set to UEFI.
    pub fn is_uefi(&self) -> bool {
        self.draft
            .as_ref()
            .is_some_and(|d| matches!(d.firmware, Firmware::Uefi { .. }))
    }

    /// Add a forwarding rule to the draft.
    ///
    /// The defaults are SSH, which is what the overwhelming majority of first
    /// rules are, and both numbers are editable right where they appear.
    pub fn add_port_forward(&mut self) {
        if let Some(draft) = self.draft.as_mut() {
            draft.network.port_forwards.push(PortForwardRule {
                host_port: 2222,
                guest_port: 22,
                proto: "tcp".to_string(),
            });
        }
    }

    /// The creation the last [`WizardState::take_draft`] was for did not work.
    ///
    /// Everything the user chose and typed is still here, so the dialog comes
    /// back as they left it and they can fix the one thing that was wrong —
    /// rather than re-picking a template, retyping a name and finding a 6 GB
    /// ISO again, only to meet the same message.
    pub fn creation_failed(&mut self) {
        if self.draft.is_some() {
            self.open = true;
        }
    }

    /// Open the dialog for a fresh machine.
    ///
    /// A constructor rather than a struct literal, so the private request
    /// flag stays private and callers cannot forget to clear it.
    pub fn opened() -> Self {
        Self {
            open: true,
            ..Self::default()
        }
    }

    /// Close without creating anything.
    fn dismiss(&mut self) {
        self.open = false;
        self.selected = None;
        self.draft = None;
        self.name.clear();
        self.iso.clear();
        self.download_requested = false;
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
    /// What the settings call the default architecture. The empty tile is
    /// "what the direct way gives", and the direct way uses this.
    default_arch: Arch,
}

impl<'a> WizardDialog<'a> {
    pub fn new(state: &'a mut WizardState, i18n: &'a I18n) -> Self {
        Self {
            state,
            i18n,
            capabilities: None,
            uefi_available: true,
            default_arch: Arch::X86_64,
        }
    }

    /// Use this architecture for the empty tile, as the direct way does.
    pub fn with_default_arch(mut self, arch: Arch) -> Self {
        self.default_arch = arch;
        self
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
                // The window is anchored and cannot be resized, so content
                // taller than the viewport cannot be reached by dragging or
                // stretching — and its height is the user's to set, through
                // the two blocks. Everything above the buttons scrolls; the
                // buttons stay outside the scroll area, so Create and Cancel
                // are on screen whatever is expanded and however small the
                // window is.
                let room = ctx.screen_rect().height();
                egui::ScrollArea::vertical()
                    .max_height((room - 120.0).max(120.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        self.gallery(ui);
                        ui.add_space(8.0);
                        self.always_visible_fields(ui);
                        ui.add_space(8.0);
                        self.hardware_block(ui);
                        self.network_block(ui);
                        self.notices(ui);
                    });
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
        let default_arch = self.default_arch;
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
                    // The empty tile is what the direct way gives, and the
                    // direct way starts from the configured architecture.
                    let arch = state.draft.as_ref().map(|d| d.arch).unwrap_or(default_arch);
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
        let download = self.i18n.t("download").to_string();
        let open_page = self.i18n.t("open_official_page").to_string();
        let limit_notice = self.i18n.t("download_limit_notice").to_string();
        let page_failed = self.i18n.t("open_page_failed").to_string();
        let filter_iso = self.i18n.t("filter_iso").to_string();
        // What the chosen template offers: a mirror we can check, or a page
        // the user fetches from themselves.
        let template_key = self.state.selected;
        let has_source = template_key
            .and_then(VmTemplate::by_key)
            .and_then(|t| t.iso_source)
            .and_then(crate::download::source_for)
            .is_some();
        let page = template_key.and_then(crate::download::official_page);
        let state = &mut *self.state;
        let mut open_page_failed: Option<String> = None;

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
                            .add_filter(&filter_iso, &["iso", "img"])
                            .pick_file()
                        {
                            state.iso = file.to_string_lossy().to_string();
                        }
                    }
                    // Only where there is a stable, documented, checkable
                    // address. Everything else gets its page instead, which
                    // is honest rather than a download that cannot be
                    // verified.
                    if has_source {
                        // A toggle, not a one-shot: clicking it changes what
                        // the dialog shows, so the deferral is visible rather
                        // than inferred. The transfer begins after Create --
                        // a button that silently does nothing until a second
                        // button is pressed reads as broken.
                        let asked = state.wants_download();
                        if ui.selectable_label(asked, &download).clicked() {
                            if asked {
                                state.withdraw_download();
                            } else {
                                state.request_download();
                            }
                        }
                    } else if let Some(url) = page {
                        if ui.button(&open_page).clicked() {
                            if let Err(e) = crate::download::open_in_browser(url) {
                                open_page_failed = Some(page_failed.replace("{error}", &e));
                            }
                        }
                    }
                });
                ui.end_row();
            });

        // Spec section 6: the limit belongs in the interface, not only in the
        // design document.
        if has_source {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(&limit_notice)
                    .size(11.0)
                    .color(ui.visuals().weak_text_color()),
            );
        }
        // Kept in the state, not in a local: a local lives for one frame,
        // which for a 16 ms flash is the same as saying nothing. For the
        // three images the application deliberately will not download, this
        // button failing in silence is the whole feature failing.
        if let Some(problem) = open_page_failed {
            self.state.page_problem = Some(problem);
        }
        if let Some(problem) = &self.state.page_problem {
            ui.label(egui::RichText::new(problem).size(11.0).color(COLOR_ERROR));
        }
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
            "firmware",
            "firmware_bios",
            "firmware_uefi",
            "firmware_missing",
        ]
        .iter()
        .map(|k| self.i18n.t(k).to_string())
        .collect();
        let caps = self.capabilities;
        let uefi_available = self.uefi_available;
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
                        // Firmware first: it is the half of this block's
                        // name that the summary line shows, and the one
                        // choice a guest cannot survive being changed after
                        // installation.
                        ui.label(&labels[7]);
                        ui.horizontal(|ui| {
                            let is_uefi = matches!(draft.firmware, Firmware::Uefi { .. });
                            if ui.selectable_label(!is_uefi, &labels[8]).clicked() && is_uefi {
                                draft.firmware = Firmware::Bios { rom: None };
                            }
                            if ui.selectable_label(is_uefi, &labels[9]).clicked() && !is_uefi {
                                draft.firmware = Firmware::Uefi {
                                    nvram: nvram_path_for(draft),
                                };
                            }
                            if matches!(draft.firmware, Firmware::Uefi { .. }) && !uefi_available {
                                ui.label(
                                    egui::RichText::new(&labels[10])
                                        .size(11.0)
                                        .color(COLOR_ERROR),
                                );
                            }
                        });
                        ui.end_row();

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
        let host_label = self.i18n.t("host_port").to_string();
        let guest_label = self.i18n.t("guest_port").to_string();
        let add_label = format!("{} {}", icons::ADD, self.i18n.t("add_port_forward"));
        let remove_label = format!("{} {}", icons::CLOSE, self.i18n.t("remove"));
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

                // Port forwarding is the other half of this block's name, and
                // the summary line counts the rules. Without an editor here
                // that count could never be anything but zero, and the user
                // had to create the machine first and find the detail panel.
                ui.add_space(6.0);
                let mut remove: Option<usize> = None;
                for (index, rule) in draft.network.port_forwards.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(&host_label);
                        ui.add(egui::DragValue::new(&mut rule.host_port).range(1..=65535));
                        ui.label("->");
                        ui.label(&guest_label);
                        ui.add(egui::DragValue::new(&mut rule.guest_port).range(1..=65535));
                        egui::ComboBox::from_id_salt(format!("wizard_fw_proto_{}", index))
                            .selected_text(&rule.proto)
                            .width(64.0)
                            .show_ui(ui, |ui| {
                                for proto in ["tcp", "udp"] {
                                    ui.selectable_value(&mut rule.proto, proto.to_string(), proto);
                                }
                            });
                        if ui.small_button(&remove_label).clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some(index) = remove {
                    draft.network.port_forwards.remove(index);
                }
                if ui.button(&add_label).clicked() {
                    draft.network.port_forwards.push(PortForwardRule {
                        host_port: 2222,
                        guest_port: 22,
                        proto: "tcp".to_string(),
                    });
                }

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
