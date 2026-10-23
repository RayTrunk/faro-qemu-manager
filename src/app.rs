// FARO QEMU MANAGER (C) R.Trunk 2026
// Application State Integration & Main Controller

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use uuid::Uuid;

use crate::config::{
    default_settings_path, default_vms_path, quarantine_file, save_settings, save_vms,
    try_load_settings, try_load_vms, AppSettings, ConfigLoadError, VmConfig,
};
use crate::i18n::I18n;
use crate::qemu::{
    build_qemu_args, find_qemu_binary, find_qemu_img, spawn_qemu, QemuEvent, VmProcess,
};
use crate::ui::console::ConsoleLogBuffer;
use crate::ui::notice::Notice;
use crate::ui::settings::SettingsDialog;
use crate::ui::sidebar::{SidebarAction, SidebarView, VmStatus};
use crate::ui::theme::{apply_theme, configure_modern_fonts, icons, COLOR_PRIMARY};
use crate::ui::vm_detail::{CreateDiskState, DetailTab, VmDetailAction, VmDetailView};

/// FARO QEMU MANAGER central application state
pub struct FaroQemApp {
    pub settings: AppSettings,
    pub settings_path: PathBuf,
    pub vms: Vec<VmConfig>,
    pub vms_path: PathBuf,
    pub selected_vm_id: Option<Uuid>,
    pub i18n: I18n,
    pub search_query: String,
    pub settings_open: bool,
    pub about_open: bool,
    pub active_tab: DetailTab,
    pub create_disk_modal: CreateDiskState,
    pub console_logs: HashMap<Uuid, ConsoleLogBuffer>,
    pub running_processes: HashMap<Uuid, (VmProcess, mpsc::Receiver<QemuEvent>)>,
    pub status_map: HashMap<Uuid, VmStatus>,
    pub cli_previews: HashMap<Uuid, String>,
    pub confirming_delete_id: Option<Uuid>,
    pub logo_texture: Option<egui::TextureHandle>,
    /// Problems the user needs to know about, shown as a band above the menu.
    pub notices: Vec<Notice>,
}

/// Short, human-facing name for a configuration path.
fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Set an unreadable configuration file aside and report what happened.
///
/// The file holds work the user cares about, so it is never replaced. Which of
/// the two outcomes occurred matters to them: either their bytes are safe at a
/// named path, or they are still sitting where they were and nothing was done
/// to them.
fn preservation_notice(i18n: &I18n, path: &Path, why: &ConfigLoadError) -> Notice {
    let file = file_label(path);
    match quarantine_file(path) {
        Ok(kept) => Notice::error(i18n.tf(
            "config_corrupt_quarantined",
            &[
                ("file", file.as_str()),
                ("path", kept.display().to_string().as_str()),
            ],
        )),
        Err(e) => Notice::error(i18n.tf(
            "config_corrupt_not_quarantined",
            &[
                ("file", file.as_str()),
                ("detail", format!("{}: {}", why, e).as_str()),
            ],
        )),
    }
}

/// Report a configuration file that could not be written.
fn save_failure_notice(i18n: &I18n, path: &Path, detail: &str) -> Notice {
    Notice::error(i18n.tf(
        "config_save_failed",
        &[("file", file_label(path).as_str()), ("detail", detail)],
    ))
}

impl FaroQemApp {
    /// Initialize `FaroQemApp` for native GUI execution with `CreationContext`
    pub fn new(cc: &eframe::CreationContext) -> Self {
        configure_modern_fonts(&cc.egui_ctx);

        let settings_path = if Path::new("settings.json").exists() {
            PathBuf::from("settings.json")
        } else {
            default_settings_path()
        };

        let vms_path = if Path::new("vms.json").exists() {
            PathBuf::from("vms.json")
        } else {
            default_vms_path()
        };

        Self::with_paths(settings_path, vms_path, Some(&cc.egui_ctx))
    }

    /// Initialize `FaroQemApp` with explicit file paths and optional egui Context
    pub fn with_paths(
        settings_path: PathBuf,
        vms_path: PathBuf,
        ctx: Option<&egui::Context>,
    ) -> Self {
        let mut notices: Vec<Notice> = Vec::new();

        // Settings come first: the chosen language decides what language the
        // notices below are written in.
        let settings = match try_load_settings(&settings_path) {
            Ok(loaded) => loaded,
            Err(ConfigLoadError::Missing) => AppSettings::default(),
            Err(why) => {
                let defaults = AppSettings::default();
                notices.push(preservation_notice(
                    &I18n::new(&defaults.language),
                    &settings_path,
                    &why,
                ));
                defaults
            }
        };

        let i18n = I18n::new(&settings.language);

        // An unreadable VM list is the user's data. Preserve it, say so, and
        // start empty — writing a starter VM here is what used to destroy it.
        let (mut vms, vms_unreadable) = match try_load_vms(&vms_path) {
            Ok(loaded) => (loaded, false),
            Err(ConfigLoadError::Missing) => (Vec::new(), false),
            Err(why) => {
                notices.push(preservation_notice(&i18n, &vms_path, &why));
                (Vec::new(), true)
            }
        };

        if vms.is_empty() && !vms_unreadable {
            let mut default_vm = VmConfig::default();
            default_vm.id = Uuid::new_v4();
            default_vm.name = "Debian 12".to_string();
            default_vm.arch = settings.default_arch;
            default_vm.machine_type = default_vm.arch.default_machine().to_string();
            default_vm.cpu_model = default_vm.arch.default_cpu().to_string();
            if let Err(e) = save_vms(&vms_path, &[default_vm.clone()]) {
                notices.push(save_failure_notice(&i18n, &vms_path, &e));
            }
            vms.push(default_vm);
        }

        let selected_vm_id = vms.first().map(|v| v.id);

        let logo_texture = ctx.and_then(|c| {
            crate::ui::theme::load_app_logo()
                .map(|img| c.load_texture("faro_logo", img, egui::TextureOptions::LINEAR))
        });

        if let Some(c) = ctx {
            configure_modern_fonts(c);
            apply_theme(c, &settings.theme);
        }

        Self {
            settings,
            settings_path,
            vms,
            vms_path,
            selected_vm_id,
            i18n,
            search_query: String::new(),
            settings_open: false,
            about_open: false,
            active_tab: DetailTab::System,
            create_disk_modal: CreateDiskState::default(),
            console_logs: HashMap::new(),
            running_processes: HashMap::new(),
            status_map: HashMap::new(),
            cli_previews: HashMap::new(),
            confirming_delete_id: None,
            logo_texture,
            notices,
        }
    }

    /// Persist the VM list, reporting a failure instead of discarding it.
    ///
    /// A silently dropped write means the user's next start quietly loses the
    /// change they just made.
    fn save_vms_now(&mut self) {
        if let Err(e) = save_vms(&self.vms_path, &self.vms) {
            let notice = save_failure_notice(&self.i18n, &self.vms_path, &e);
            self.notices.push(notice);
        }
    }

    /// Persist application settings, reporting a failure instead of discarding it.
    fn save_settings_now(&mut self) {
        if let Err(e) = save_settings(&self.settings_path, &self.settings) {
            let notice = save_failure_notice(&self.i18n, &self.settings_path, &e);
            self.notices.push(notice);
        }
    }

    /// Non-blocking drain of all asynchronous QEMU events from running process channels
    pub fn process_qemu_events(&mut self, ctx: Option<&egui::Context>) {
        let mut exited_ids = Vec::new();
        let mut repaint_requested = false;

        for (&id, (proc, rx)) in self.running_processes.iter_mut() {
            let log_buf = self.console_logs.entry(id).or_default();
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        repaint_requested = true;
                        match &event {
                            QemuEvent::Started(pid) => {
                                self.status_map.insert(id, VmStatus::Running { pid: *pid });
                                log_buf.handle_event(&event);
                            }
                            QemuEvent::Stdout(_) | QemuEvent::Stderr(_) => {
                                log_buf.handle_event(&event);
                            }
                            QemuEvent::Exited(code) => {
                                log_buf.handle_event(&event);
                                match code {
                                    Some(0) | None => {
                                        self.status_map.insert(id, VmStatus::Stopped);
                                    }
                                    Some(c) => {
                                        self.status_map.insert(
                                            id,
                                            VmStatus::Error(format!(
                                                "QEMU process exited with code {}",
                                                c
                                            )),
                                        );
                                    }
                                }
                                exited_ids.push(id);
                            }
                            QemuEvent::Error(err) => {
                                log_buf.handle_event(&event);
                                self.status_map.insert(id, VmStatus::Error(err.clone()));
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if !proc.is_alive() {
                            exited_ids.push(id);
                            self.status_map.insert(id, VmStatus::Stopped);
                        }
                        break;
                    }
                }
            }
        }

        for id in exited_ids {
            self.running_processes.remove(&id);
        }

        if repaint_requested {
            if let Some(c) = ctx {
                c.request_repaint();
            }
        }
    }

    /// Launch a virtual machine by ID
    pub fn launch_vm(&mut self, id: Uuid, ctx: Option<&egui::Context>) -> Result<(), String> {
        let vm = self
            .vms
            .iter()
            .find(|v| v.id == id)
            .cloned()
            .ok_or_else(|| format!("VM with id {} not found", id))?;

        if let Some(status) = self.status_map.get(&id) {
            if status.is_running() {
                return Ok(());
            }
        }

        let qemu_bin = find_qemu_binary(self.settings.qemu_dir.as_deref(), vm.arch);
        let bin_path = match qemu_bin {
            Some(path) => path,
            None => {
                let err_msg = format!("QEMU binary not found for {}", vm.arch.as_str());
                self.status_map.insert(id, VmStatus::Error(err_msg.clone()));
                let log_buf = self.console_logs.entry(id).or_default();
                log_buf.push_error(&err_msg);
                return Err(err_msg);
            }
        };

        let args = build_qemu_args(&vm);
        let cli_str = format!("{} {}", bin_path.display(), shell_words::join(&args));
        self.cli_previews.insert(id, cli_str);

        let (tx, rx) = mpsc::channel();
        match spawn_qemu(&bin_path, &args, tx) {
            Ok(proc) => {
                let pid = proc.pid();
                self.running_processes.insert(id, (proc, rx));
                self.status_map.insert(id, VmStatus::Running { pid });
                let log_buf = self.console_logs.entry(id).or_default();
                log_buf.push_system(format!("[PROCESS] QEMU started with PID {}", pid));
                self.active_tab = DetailTab::Console;
                if let Some(c) = ctx {
                    c.request_repaint();
                }
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("Failed to spawn QEMU: {}", e);
                self.status_map.insert(id, VmStatus::Error(err_msg.clone()));
                let log_buf = self.console_logs.entry(id).or_default();
                log_buf.push_error(&err_msg);
                Err(err_msg)
            }
        }
    }

    /// Stop a virtual machine by ID
    pub fn stop_vm(&mut self, id: Uuid) -> Result<(), String> {
        if let Some((mut proc, _)) = self.running_processes.remove(&id) {
            let res = proc.kill();
            let log_buf = self.console_logs.entry(id).or_default();
            log_buf.push_system("[PROCESS] QEMU process terminated");
            self.status_map.insert(id, VmStatus::Stopped);
            res
        } else {
            self.status_map.insert(id, VmStatus::Stopped);
            Ok(())
        }
    }

    /// Handle actions triggered by the sidebar
    pub fn handle_sidebar_action(&mut self, action: SidebarAction, _ctx: Option<&egui::Context>) {
        match action {
            SidebarAction::SelectVm(id) => {
                self.selected_vm_id = Some(id);
            }
            SidebarAction::NewVm => {
                let mut new_vm = VmConfig::default();
                new_vm.id = Uuid::new_v4();
                new_vm.name = format!("VM {}", self.vms.len() + 1);
                new_vm.arch = self.settings.default_arch;
                new_vm.machine_type = new_vm.arch.default_machine().to_string();
                new_vm.cpu_model = new_vm.arch.default_cpu().to_string();
                let new_id = new_vm.id;
                self.vms.push(new_vm);
                self.selected_vm_id = Some(new_id);
                self.save_vms_now();
            }
            SidebarAction::DuplicateVm(id) => {
                if let Some(pos) = self.vms.iter().position(|v| v.id == id) {
                    let mut cloned = self.vms[pos].clone();
                    cloned.id = Uuid::new_v4();
                    cloned.name = format!("{} (Copy)", cloned.name);
                    let new_id = cloned.id;
                    self.vms.push(cloned);
                    self.selected_vm_id = Some(new_id);
                    self.save_vms_now();
                }
            }
            SidebarAction::DeleteVm(id) => {
                let _ = self.stop_vm(id);
                self.status_map.remove(&id);
                self.console_logs.remove(&id);
                self.cli_previews.remove(&id);
                if let Some(pos) = self.vms.iter().position(|v| v.id == id) {
                    self.vms.remove(pos);
                    self.save_vms_now();
                    if self.selected_vm_id == Some(id) {
                        if self.vms.is_empty() {
                            self.selected_vm_id = None;
                        } else if pos < self.vms.len() {
                            self.selected_vm_id = Some(self.vms[pos].id);
                        } else {
                            self.selected_vm_id = Some(self.vms[pos - 1].id);
                        }
                    }
                }
            }
            SidebarAction::OpenSettings => {
                self.settings_open = true;
            }
        }
    }

    /// Handle actions triggered by the VM detail view
    pub fn handle_vm_detail_action(&mut self, action: VmDetailAction, ctx: Option<&egui::Context>) {
        match action {
            VmDetailAction::Launch(id) => {
                let _ = self.launch_vm(id, ctx);
            }
            VmDetailAction::Stop(id) => {
                let _ = self.stop_vm(id);
            }
            VmDetailAction::Save(_id) => {
                self.save_vms_now();
            }
        }
    }

    /// Render empty-state greeting card when no VM is selected
    fn render_empty_state(&self, ui: &mut egui::Ui) -> bool {
        let mut create_clicked = false;
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            if let Some(ref logo) = self.logo_texture {
                ui.image((logo.id(), egui::vec2(120.0, 120.0)));
            } else {
                ui.label(egui::RichText::new("🖥").size(48.0));
            }
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(
                    "Select a virtual machine from the sidebar or create a new one.",
                )
                .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(20.0);
            let create_btn = egui::Button::new(format!("{} {}", icons::ADD, self.i18n.t("new_vm")))
                .fill(COLOR_PRIMARY)
                .min_size(egui::vec2(160.0, 36.0));
            if ui.add(create_btn).clicked() {
                create_clicked = true;
            }
        });
        create_clicked
    }

    /// Render one UI frame and process asynchronous supervisor events
    pub fn render_ui(&mut self, ctx: &egui::Context) {
        // 1. Process asynchronous QEMU events
        self.process_qemu_events(Some(ctx));

        let mut menu_sidebar_action: Option<SidebarAction> = None;
        let mut menu_detail_action: Option<VmDetailAction> = None;

        // 2. Top Menu Bar (Global application commands & Settings)
        egui::TopBottomPanel::top("top_menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // File Menu
                ui.menu_button(self.i18n.t("menu_file"), |ui| {
                    if ui
                        .button(format!("{} {}", icons::ADD, self.i18n.t("new_vm")))
                        .clicked()
                    {
                        menu_sidebar_action = Some(SidebarAction::NewVm);
                        ui.close_menu();
                    }
                    let can_dup = self.selected_vm_id.is_some();
                    if ui
                        .add_enabled(
                            can_dup,
                            egui::Button::new(format!(
                                "{} {}",
                                icons::EDIT,
                                self.i18n.t("duplicate")
                            )),
                        )
                        .clicked()
                    {
                        if let Some(id) = self.selected_vm_id {
                            menu_sidebar_action = Some(SidebarAction::DuplicateVm(id));
                        }
                        ui.close_menu();
                    }
                    let can_del = self.selected_vm_id.is_some();
                    if ui
                        .add_enabled(
                            can_del,
                            egui::Button::new(format!(
                                "{} {}",
                                icons::DELETE,
                                self.i18n.t("delete")
                            )),
                        )
                        .clicked()
                    {
                        if let Some(id) = self.selected_vm_id {
                            self.confirming_delete_id = Some(id);
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .button(format!("{} {}", icons::CLOSE, self.i18n.t("exit")))
                        .clicked()
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close_menu();
                    }
                });

                // Virtual Machine Menu
                ui.menu_button(self.i18n.t("menu_vm"), |ui| {
                    let has_vm = self.selected_vm_id.is_some();
                    let is_running = self
                        .selected_vm_id
                        .and_then(|id| self.status_map.get(&id))
                        .map(|s| s.is_running())
                        .unwrap_or(false);

                    if ui
                        .add_enabled(
                            has_vm && !is_running,
                            egui::Button::new(format!("{} {}", icons::PLAY, self.i18n.t("launch"))),
                        )
                        .clicked()
                    {
                        if let Some(id) = self.selected_vm_id {
                            menu_detail_action = Some(VmDetailAction::Launch(id));
                        }
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(
                            has_vm && is_running,
                            egui::Button::new(format!("{} {}", icons::STOP, self.i18n.t("stop"))),
                        )
                        .clicked()
                    {
                        if let Some(id) = self.selected_vm_id {
                            menu_detail_action = Some(VmDetailAction::Stop(id));
                        }
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(
                            has_vm,
                            egui::Button::new(format!("{} {}", icons::SAVE, self.i18n.t("save"))),
                        )
                        .clicked()
                    {
                        if let Some(id) = self.selected_vm_id {
                            menu_detail_action = Some(VmDetailAction::Save(id));
                        }
                        ui.close_menu();
                    }
                });

                // Help Menu
                ui.menu_button(self.i18n.t("menu_help"), |ui| {
                    if ui.button(self.i18n.t("about")).clicked() {
                        self.about_open = true;
                        ui.close_menu();
                    }
                });

                // Right Upper Corner: Only location for Settings (Gear Symbol + "Settings")
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(format!("{} {}", icons::SETTINGS, self.i18n.t("settings")))
                        .clicked()
                    {
                        self.settings_open = true;
                    }
                });
            });
        });

        if let Some(action) = menu_sidebar_action {
            self.handle_sidebar_action(action, Some(ctx));
        }
        if let Some(action) = menu_detail_action {
            self.handle_vm_detail_action(action, Some(ctx));
        }

        // 3. Sidebar Navigation Panel
        let mut sidebar_action = None;
        egui::SidePanel::left("sidebar_panel")
            .resizable(true)
            .default_width(280.0)
            .min_width(220.0)
            .max_width(400.0)
            .show(ctx, |ui| {
                sidebar_action = SidebarView::new(
                    &self.vms,
                    self.selected_vm_id,
                    &mut self.search_query,
                    &self.status_map,
                    &self.i18n,
                )
                .with_delete_confirmation(&mut self.confirming_delete_id)
                .with_logo(self.logo_texture.as_ref())
                .show(ui);
            });

        if let Some(action) = sidebar_action {
            self.handle_sidebar_action(action, Some(ctx));
        }

        // 4. Central Content Panel
        let mut detail_action = None;
        let mut empty_state_create = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(id) = self.selected_vm_id {
                if let Some(pos) = self.vms.iter().position(|v| v.id == id) {
                    let is_running = self
                        .status_map
                        .get(&id)
                        .map(|s| s.is_running())
                        .unwrap_or(false);
                    let pid = self.status_map.get(&id).and_then(|s| s.pid());
                    let cli_cmd = self.cli_previews.get(&id).map(|s| s.as_str());
                    let qemu_img = find_qemu_img(self.settings.qemu_dir.as_deref());

                    let console_buf = self.console_logs.entry(id).or_default();
                    let mut view = VmDetailView::new(
                        &mut self.vms[pos],
                        is_running,
                        &self.i18n,
                        &mut self.active_tab,
                        qemu_img.as_deref(),
                        &mut self.create_disk_modal,
                    )
                    .with_console(console_buf)
                    .with_logo(self.logo_texture.as_ref());

                    if let Some(p) = pid {
                        view = view.with_pid(p);
                    }
                    if let Some(cmd) = cli_cmd {
                        view = view.with_cli_command(cmd);
                    }

                    detail_action = view.show(ui);
                } else {
                    empty_state_create = self.render_empty_state(ui);
                }
            } else {
                empty_state_create = self.render_empty_state(ui);
            }
        });

        if empty_state_create {
            self.handle_sidebar_action(SidebarAction::NewVm, Some(ctx));
        }

        if let Some(action) = detail_action {
            self.handle_vm_detail_action(action, Some(ctx));
        }

        // 5. Modal Settings Dialog
        if self.settings_open {
            let mut dialog =
                SettingsDialog::new(&mut self.settings_open, &mut self.settings, &mut self.i18n);
            if dialog.show(ctx) {
                self.save_settings_now();
                apply_theme(ctx, &self.settings.theme);
                self.i18n.set_locale(&self.settings.language);
            }
        }

        // 6. About Dialog
        if self.about_open {
            let mut is_open = self.about_open;
            egui::Window::new(self.i18n.t("about"))
                .open(&mut is_open)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(320.0);
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        if let Some(ref logo) = self.logo_texture {
                            ui.image((logo.id(), egui::vec2(64.0, 64.0)));
                            ui.add_space(6.0);
                        }
                        ui.heading("FARO QEMU MANAGER");
                        ui.label(
                            egui::RichText::new("(C) R.Trunk 2026")
                                .strong()
                                .size(12.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.add_space(8.0);
                        ui.label("Modern QEMU Virtual Machine Manager");
                        ui.label("Swiss High-Modernist Desktop Interface");
                        ui.add_space(12.0);
                        if ui.button(self.i18n.t("close")).clicked() {
                            self.about_open = false;
                        }
                        ui.add_space(4.0);
                    });
                });
            self.about_open = is_open;
        }
    }
}

impl Drop for FaroQemApp {
    fn drop(&mut self) {
        for (_id, (mut proc, _)) in self.running_processes.drain() {
            let _ = proc.kill();
        }
        // Discarded deliberately: the window is going away, so there is no
        // interface left that could show a notice about a failed write.
        let _ = save_vms(&self.vms_path, &self.vms);
        let _ = save_settings(&self.settings_path, &self.settings);
    }
}

impl eframe::App for FaroQemApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_ui(ctx);
    }
}
