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
use std::time::{Duration, SystemTime};
use uuid::Uuid;

use crate::config::{DriveFormat, VmConfig};
use crate::i18n::I18n;
use crate::preflight::{Finding, Severity};
use crate::qemu::QemuCapabilities;
use crate::ui::console::{ConsoleLogBuffer, ConsoleView};
use crate::ui::theme::{
    card_frame, icons, status_badge, COLOR_ERROR, COLOR_PRIMARY, COLOR_RUNNING, COLOR_STOPPED,
    COLOR_WARNING, ROUNDING_BUTTON,
};

/// The same colour, quieter.
///
/// Used where something should be recognisable as belonging to the accent
/// without competing with the one action that carries it.
fn tinted(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// How long a guest is given before the hard way appears beside the waiting.
///
/// Twenty seconds: long enough that a machine shutting down normally never
/// invites anyone to pull the plug halfway through, short enough that nobody
/// stands in front of a hung guest with nothing to do (Review Focus 2).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(20);

/// Actions emitted by the VM detail view toolbar and tabs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmDetailAction {
    Launch(Uuid),
    /// Ask the guest to power down. Never kills: spec section 7.1.
    Shutdown(Uuid),
    /// Pull the plug. A second, named step the user chooses deliberately,
    /// offered beside a waiting that has gone on too long or where no
    /// graceful path exists at all.
    ForceStop(Uuid),
    Save(Uuid),
    /// Make a disk image for this machine. The work belongs to the
    /// application, not to the frame: qemu-img takes seconds to minutes on
    /// a large image, and the panel would be frozen for all of them.
    CreateDisk {
        id: Uuid,
        path: std::path::PathBuf,
        format: DriveFormat,
        size_gb: u32,
    },
    /// Fetch this machine's installation image, or pick up where an
    /// interrupted transfer left off. Without this, one broken connection
    /// left a machine with a `.part` nothing could resume and a start the
    /// check would hold for ever.
    Download(Uuid),
    /// Stop the transfer and clear what it left behind.
    CancelDownload(Uuid),
}

impl VmDetailAction {
    /// Return the ID of the target virtual machine
    pub fn vm_id(&self) -> Uuid {
        match self {
            VmDetailAction::Launch(id)
            | VmDetailAction::Shutdown(id)
            | VmDetailAction::ForceStop(id)
            | VmDetailAction::Save(id)
            | VmDetailAction::Download(id)
            | VmDetailAction::CancelDownload(id) => *id,
            VmDetailAction::CreateDisk { id, .. } => *id,
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
    /// Whether qemu-img is working on this request right now. The dialog
    /// stays open and says so, rather than vanishing and leaving the answer
    /// -- "created", or the reason it failed -- with nobody to read it.
    pub busy: bool,
}

impl Default for CreateDiskState {
    fn default() -> Self {
        Self {
            open: false,
            path_buf: String::new(),
            format: DriveFormat::Qcow2,
            size_gb: 20,
            status_msg: None,
            busy: false,
        }
    }
}

impl CreateDiskState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the dialog, proposing a path for the new image.
    ///
    /// `base` is the machine's own folder. Without it a bare filename is
    /// proposed, and a bare filename is written wherever the application was
    /// launched from -- which is how a stray disk image ends up in the
    /// project directory (spec section 5.7). With it, the proposal lands
    /// beside the machine's other files.
    pub fn open_dialog(&mut self, base: Option<&Path>, default_filename: &str) {
        self.open = true;
        self.status_msg = None;
        if self.path_buf.is_empty() && !default_filename.is_empty() {
            self.path_buf = match base {
                Some(dir) => dir.join(default_filename).display().to_string(),
                None => default_filename.to_string(),
            };
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
    /// The machine's own folder under the VM directory, when it has one.
    /// New files -- a second disk, a variable store -- are proposed inside
    /// it rather than next to the executable.
    pub vm_folder: Option<&'a Path>,
    /// What the installed QEMU can do. `None` means nobody asked.
    pub capabilities: Option<&'a QemuCapabilities>,
    /// Whether the app found the firmware a UEFI machine needs. Resolved once
    /// beside the capabilities; searching for it here would mean a handful of
    /// `is_file` calls on every frame this panel draws. Unknown counts as
    /// present, the same way an unprobed capability permits everything: a
    /// warning nobody asked for is worse than none.
    pub uefi_available: bool,
    /// What the start check found for this machine. Shown above the start
    /// button, so a refused start is explained before it is attempted.
    pub findings: &'a [Finding],
    /// One sentence about a download in flight, when there is one.
    pub download_status: Option<&'a str>,
    /// Whether this machine has an image worth fetching: a template with a
    /// mirror, and no finished file yet.
    pub can_resume_download: bool,
    /// Whether a graceful shutdown is possible at all: a QMP channel to this
    /// guest. `false` by default — a panel that assumed one would offer a
    /// button whose only possible outcome is an error message.
    pub qmp_available: bool,
    /// Since when the guest has been asked to power down, when it has.
    pub shutting_down_since: Option<SystemTime>,
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
            vm_folder: None,
            capabilities: None,
            uefi_available: true,
            findings: &[],
            download_status: None,
            can_resume_download: false,
            qmp_available: false,
            shutting_down_since: None,
        }
    }

    /// What the stop controls may offer: whether a QMP channel exists, and
    /// since when this guest has been asked to power down.
    pub fn with_shutdown(mut self, qmp_available: bool, since: Option<SystemTime>) -> Self {
        self.qmp_available = qmp_available;
        self.shutting_down_since = since;
        self
    }

    /// Attach what the installed QEMU can do, so the view offers only that.
    /// Hand the panel what the start check found.
    pub fn with_findings(mut self, findings: &'a [Finding]) -> Self {
        self.findings = findings;
        self
    }

    /// Tell the panel whether the UEFI firmware pair was found.
    pub fn with_firmware(mut self, available: bool) -> Self {
        self.uefi_available = available;
        self
    }

    pub fn with_capabilities(mut self, caps: &'a QemuCapabilities) -> Self {
        self.capabilities = Some(caps);
        self
    }

    /// Tell the panel what a running download is doing.
    pub fn with_download_status(mut self, status: Option<&'a str>) -> Self {
        self.download_status = status;
        self
    }

    /// Tell the panel whether fetching this machine's image is on offer.
    pub fn with_resumable_download(mut self, resumable: bool) -> Self {
        self.can_resume_download = resumable;
        self
    }

    /// Tell the panel where this machine's files belong.
    pub fn with_vm_folder(mut self, folder: Option<&'a Path>) -> Self {
        self.vm_folder = folder;
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
    /// The stop controls, in the one place both layouts read them from.
    ///
    /// Three states, and none of them hides what it does: a running guest is
    /// asked to power down, a guest already shutting down shows how long it
    /// has been at it and — once the grace period is up — the hard way
    /// beside it, and a guest with no QMP channel is told so plainly rather
    /// than offered a "shutdown" that would kill it.
    fn stop_controls(
        &self,
        ui: &mut egui::Ui,
        min_size: Option<egui::Vec2>,
    ) -> Option<VmDetailAction> {
        let mut action = None;
        let button = |ui: &mut egui::Ui, label: String, fill: Color32, enabled: bool| -> bool {
            let mut btn =
                egui::Button::new(egui::RichText::new(label).strong().color(Color32::WHITE))
                    .fill(fill);
            if let Some(size) = min_size {
                btn = btn.min_size(size);
            }
            ui.add_enabled(enabled, btn).clicked()
        };
        let force_label = format!("{} {}", icons::STOP, self.i18n.t("force_stop"));
        let shutdown_label = format!("{} {}", icons::STOP, self.i18n.t("shutdown"));

        if let Some(since) = self.shutting_down_since {
            // A guest that never finishes must not leave the user staring at
            // a spinner with no way out.
            let waited = since.elapsed().unwrap_or_default();
            ui.label(
                egui::RichText::new(self.i18n.t("shutting_down"))
                    .color(COLOR_WARNING)
                    .size(12.0),
            );
            ui.label(
                egui::RichText::new(self.i18n.tf(
                    "shutdown_elapsed",
                    &[("seconds", &waited.as_secs().to_string())],
                ))
                .size(11.0)
                .color(ui.visuals().weak_text_color()),
            );
            if waited >= SHUTDOWN_GRACE && button(ui, force_label, COLOR_ERROR, true) {
                action = Some(VmDetailAction::ForceStop(self.vm.id));
            }
        } else if self.is_running && self.qmp_available {
            if button(ui, shutdown_label, COLOR_WARNING, true) {
                action = Some(VmDetailAction::Shutdown(self.vm.id));
            }
        } else if self.is_running {
            ui.label(
                egui::RichText::new(self.i18n.t("shutdown_unavailable"))
                    .color(COLOR_WARNING)
                    .size(11.0),
            );
            if button(ui, force_label, COLOR_ERROR, true) {
                action = Some(VmDetailAction::ForceStop(self.vm.id));
            }
        } else {
            // Nothing is running: the control keeps its place, greyed out.
            let _ = button(ui, shutdown_label, COLOR_STOPPED, false);
        }
        action
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<VmDetailAction> {
        let mut action: Option<VmDetailAction> = None;

        // 1. Top Header Card (Swiss Architectural Folio Header)
        card_frame(ui.visuals()).show(ui, |ui| {
            // Said before the start, not read in the console afterwards. This
            // sits above the two layout branches so neither can omit it.
            for finding in self.findings {
                let colour = match finding.severity {
                    Severity::Blocking => COLOR_ERROR,
                    Severity::Warning => COLOR_WARNING,
                };
                ui.label(egui::RichText::new(&finding.text).size(11.0).color(colour));
            }
            // A download in flight says what it is doing and offers the one
            // thing the user might want: to stop it. One that is not running
            // but left a partial behind offers to pick it up — without that,
            // a single broken connection left the machine unstartable with
            // nothing in the interface to do about it.
            if let Some(status) = self.download_status {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(status).size(11.0).color(COLOR_PRIMARY));
                    if ui.small_button(self.i18n.t("cancel")).clicked() {
                        action = Some(VmDetailAction::CancelDownload(self.vm.id));
                    }
                });
            } else if self.can_resume_download {
                ui.horizontal(|ui| {
                    if ui
                        .button(format!("{} {}", icons::ADD, self.i18n.t("download")))
                        .clicked()
                    {
                        action = Some(VmDetailAction::Download(self.vm.id));
                    }
                    if ui.small_button(self.i18n.t("clear")).clicked() {
                        action = Some(VmDetailAction::CancelDownload(self.vm.id));
                    }
                });
            }
            if !self.findings.is_empty() || self.download_status.is_some() {
                ui.add_space(4.0);
            }

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

                    // [⏹ Shut Down] / the waiting state / [⏹ Force Stop]
                    if let Some(a) = self.stop_controls(ui, None) {
                        action = Some(a);
                    }

                    // [💾 Save]
                    let save_btn = egui::Button::new(
                        egui::RichText::new(format!("{} {}", icons::SAVE, self.i18n.t("save")))
                            .strong()
                            .color(COLOR_PRIMARY),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, tinted(COLOR_PRIMARY, 150)));
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

                    // Running / Shutting down / Stopped Status Badge
                    if self.shutting_down_since.is_some() {
                        status_badge(ui, self.i18n.t("status_shutting_down"), COLOR_WARNING);
                    } else if self.is_running {
                        let status_text = if let Some(pid) = self.pid {
                            self.i18n
                                .tf("badge_running_pid", &[("pid", &pid.to_string())])
                        } else {
                            self.i18n.t("badge_running").to_string()
                        };
                        status_badge(ui, &status_text, COLOR_RUNNING);
                    } else {
                        status_badge(ui, "STOPPED", COLOR_STOPPED);
                    }

                    // Wrapping, not overflowing. Right-aligned groups that
                    // do not fit used to be laid out past the right edge of
                    // the window -- and egui does not paint what lies
                    // outside, so the save button was simply not there. With
                    // a wrap, a group that runs out of room moves to the
                    // next line and stays reachable.
                    let actions =
                        egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true);
                    ui.with_layout(actions, |ui| {
                        // 1. [💾 Save] (rightmost)
                        // Outlined, not filled: starting a machine is the
                        // one thing this row is for, and three filled
                        // buttons side by side name no first among them.
                        let save_btn = egui::Button::new(
                            egui::RichText::new(format!("{} {}", icons::SAVE, self.i18n.t("save")))
                                .strong()
                                .color(COLOR_PRIMARY),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::new(1.0, tinted(COLOR_PRIMARY, 150)))
                        .min_size(egui::vec2(80.0, 28.0));
                        if ui.add(save_btn).clicked() {
                            action = Some(VmDetailAction::Save(self.vm.id));
                        }

                        // 2. [⏹ Shut Down] / the waiting state (middle)
                        if let Some(a) = self.stop_controls(ui, Some(egui::vec2(80.0, 28.0))) {
                            action = Some(a);
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
                            egui::RichText::new(self.i18n.t("card_architecture"))
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
                            egui::RichText::new(self.i18n.t("card_processing"))
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
                            egui::RichText::new(self.i18n.t("card_memory"))
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
                            egui::RichText::new(self.i18n.t("card_accelerator"))
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
                // The chosen tab is marked, not shouted: a solid accent
                // fill made it as loud as the one real action on the
                // screen, and two things shouting is one thing too many.
                let btn = if is_selected {
                    egui::Button::new(egui::RichText::new(text).strong().color(COLOR_PRIMARY))
                        .fill(tinted(COLOR_PRIMARY, 46))
                        .stroke(Stroke::new(1.0, tinted(COLOR_PRIMARY, 150)))
                        .min_size(egui::vec2(95.0, 28.0))
                } else {
                    egui::Button::new(egui::RichText::new(text).color(ui.visuals().text_color()))
                        .fill(Color32::TRANSPARENT)
                        .min_size(egui::vec2(95.0, 28.0))
                };

                if ui.add(btn).clicked() {
                    *self.active_tab = tab;
                }
            }
        });

        ui.add_space(8.0);
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
        if let Some(a) = self.show_create_disk_modal(ui.ctx()) {
            action = Some(a);
        }

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
                    ui.label(
                        egui::RichText::new(icons::CONSOLE)
                            .size(24.0)
                            .color(COLOR_PRIMARY),
                    );
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(self.i18n.t("console_title")).strong());
                        if self.is_running {
                            ui.label(
                                egui::RichText::new(self.i18n.t("console_running_hint"))
                                    .color(COLOR_RUNNING)
                                    .size(12.0),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(self.i18n.t("console_stopped_hint"))
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
                    .stroke(Stroke::new(
                        1.0,
                        ui.visuals().widgets.noninteractive.bg_stroke.color,
                    ))
                    .rounding(egui::Rounding::same(ROUNDING_BUTTON))
                    .inner_margin(egui::Margin::same(10.0));

                frame.show(ui, |ui| {
                    ui.set_min_height(140.0);
                    if self.is_running {
                        ui.label(
                            egui::RichText::new(
                                self.i18n
                                    .tf("console_placeholder_running", &[("name", &self.vm.name)]),
                            )
                            .monospace()
                            .size(12.0)
                            .color(COLOR_RUNNING),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(
                                self.i18n
                                    .tf("console_placeholder_stopped", &[("name", &self.vm.name)]),
                            )
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
