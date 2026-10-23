// FARO QEMU MANAGER (C) R.Trunk 2026
// Application State Integration & Main Controller

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use uuid::Uuid;

use crate::config::{
    claim_vm_folder, containment, default_settings_path, default_vms_path, find_vm_folders,
    is_inside, quarantine_file, recovery_for, save_settings, save_vms, try_load_settings,
    try_load_vms, AppSettings, ConfigLoadError, ConfigRecovery, Containment, CreationMode,
    Firmware, VmConfig,
};
use crate::i18n::I18n;
use crate::preflight::{blocks, check_vm, Finding, HostFacts};
use crate::qemu::{
    build_qemu_args, create_disk_image, find_qemu_binary, find_qemu_img, find_uefi_firmware,
    nvram_path_for, prepare_nvram, probe_capabilities, spawn_qemu, QemuCapabilities, QemuEvent,
    UefiFirmware, VmProcess,
};
use crate::ui::console::ConsoleLogBuffer;
use crate::ui::notice::{Notice, NoticeBar};
use crate::ui::settings::SettingsDialog;
use crate::ui::sidebar::{SidebarAction, SidebarView, VmStatus};
use crate::ui::theme::{apply_theme, configure_modern_fonts, icons, COLOR_PRIMARY};
use crate::ui::vm_detail::{CreateDiskState, DetailTab, VmDetailAction, VmDetailView};
use crate::ui::wizard::{WizardAction, WizardDialog, WizardState};

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
    /// The VM list on disk could not be read and is still sitting there.
    /// Writing to it would destroy it, so every save to that path is refused.
    pub vms_write_blocked: bool,
    /// Same barrier for the settings file.
    pub settings_write_blocked: bool,
    /// What the installed QEMU can do. Probed when the directory is chosen or
    /// changed and then kept — five processes is not something to spawn while
    /// drawing a frame.
    pub capabilities: QemuCapabilities,
    /// The UEFI firmware pair beside the QEMU in use, resolved at the same
    /// time as the capabilities. Searching for it is a handful of `is_file`
    /// calls, which is fine once and wrong once per frame.
    pub uefi_firmware: Option<UefiFirmware>,
    /// The template wizard, when the settings take that way.
    pub wizard: WizardState,
    /// Whether the user ticked "take the folder and files" in the delete box.
    /// Never pre-ticked, and reset every time the box opens.
    pub delete_with_folder: bool,
    /// The folder the delete box offers to take, worked out once when the box
    /// opens: finding it reads a directory, which is not a per-frame job.
    pub delete_folder: Option<PathBuf>,
    /// Which VM `delete_folder` was worked out for.
    pub delete_folder_for: Option<Uuid>,
    /// The selected machine's own folder, so the detail panel can propose
    /// new files inside it. Worked out when the selection changes, because
    /// finding it reads a directory.
    pub selected_folder: Option<PathBuf>,
    /// Which VM `selected_folder` was worked out for.
    pub selected_folder_for: Option<Uuid>,
    /// What this host has, asked once. The start check compares against it.
    pub host: HostFacts,
}

/// Short, human-facing name for a configuration path.
fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Act on a configuration that could not be loaded, and report what was done.
///
/// Returns the notice to show, and whether writes to this path must be refused
/// for the rest of the session. That second value is the whole point: if the
/// user's bytes are still sitting at the path, every later write — including
/// the one on shutdown — would destroy exactly what was just preserved.
fn recover_config(i18n: &I18n, path: &Path, why: &ConfigLoadError) -> (Option<Notice>, bool) {
    let file = file_label(path);

    match recovery_for(why) {
        // Nothing was there. Defaults are right and writing is safe.
        ConfigRecovery::StartFresh => (None, false),

        ConfigRecovery::Preserve => match quarantine_file(path) {
            // The bytes are safe elsewhere and the path is free: write away.
            Ok(kept) => (
                Some(Notice::error(i18n.tf(
                    "config_corrupt_quarantined",
                    &[
                        ("file", file.as_str()),
                        ("path", kept.display().to_string().as_str()),
                    ],
                ))),
                false,
            ),
            // Could not move it. The bytes are still there, so nothing may
            // write over them.
            Err(e) => (
                Some(Notice::error(i18n.tf(
                    "config_corrupt_not_quarantined",
                    &[
                        ("file", file.as_str()),
                        ("detail", format!("{}: {}", why, e).as_str()),
                    ],
                ))),
                true,
            ),
        },

        // We never saw the bytes, so the file may be perfectly healthy and
        // merely locked. Renaming it to `.corrupt-` would be a lie and would
        // bury a good configuration; writing over it would lose it outright.
        ConfigRecovery::LeaveAlone => (
            Some(Notice::error(i18n.tf(
                "config_unreadable_left_alone",
                &[
                    ("file", file.as_str()),
                    ("detail", why.to_string().as_str()),
                ],
            ))),
            true,
        ),
    }
}

/// Report a save that was deliberately not attempted.
fn write_blocked_notice(i18n: &I18n, path: &Path) -> Notice {
    Notice::error(i18n.tf(
        "config_write_blocked",
        &[("file", file_label(path).as_str())],
    ))
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
        let (settings, settings_write_blocked) = match try_load_settings(&settings_path) {
            Ok(loaded) => (loaded, false),
            Err(why) => {
                let defaults = AppSettings::default();
                let (notice, blocked) =
                    recover_config(&I18n::new(&defaults.language), &settings_path, &why);
                notices.extend(notice);
                (defaults, blocked)
            }
        };

        // Spec 11.7: no schema marker beside an existing machine list means
        // this installation predates the wizard, and it keeps the way it
        // knows. A plain serde default could not tell the two apart.
        let mut settings = settings;
        if settings.schema.is_none() && vms_path.exists() {
            settings.creation_mode = CreationMode::Direct;
        }
        let settings = settings;

        let i18n = I18n::new(&settings.language);

        // A VM list that exists but could not be loaded is the user's data.
        // Writing a starter VM over it is precisely what used to destroy it, so
        // `may_seed` stays false for anything but an absent or readable file.
        let (mut vms, vms_write_blocked, may_seed) = match try_load_vms(&vms_path) {
            Ok(loaded) => (loaded, false, true),
            Err(why) => {
                let may_seed = recovery_for(&why) == ConfigRecovery::StartFresh;
                let (notice, blocked) = recover_config(&i18n, &vms_path, &why);
                notices.extend(notice);
                (Vec::new(), blocked, may_seed)
            }
        };

        if vms.is_empty() && may_seed {
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

        let capabilities = find_qemu_binary(settings.qemu_dir.as_deref(), settings.default_arch)
            .map(|bin| probe_capabilities(&bin))
            .unwrap_or_default();
        let uefi_firmware = find_uefi_firmware(settings.qemu_dir.as_deref());

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
            vms_write_blocked,
            settings_write_blocked,
            capabilities,
            uefi_firmware,
            wizard: WizardState::default(),
            delete_with_folder: false,
            delete_folder: None,
            delete_folder_for: None,
            selected_folder: None,
            selected_folder_for: None,
            host: HostFacts::probe(),
        }
    }

    /// Re-ask QEMU what it can do, after the directory or architecture changed.
    ///
    /// The firmware search travels with it: both answers depend on which QEMU
    /// is in use, and both are too expensive to repeat while drawing.
    pub fn refresh_capabilities(&mut self) {
        self.capabilities = find_qemu_binary(
            self.settings.qemu_dir.as_deref(),
            self.settings.default_arch,
        )
        .map(|bin| probe_capabilities(&bin))
        .unwrap_or_default();
        self.uefi_firmware = find_uefi_firmware(self.settings.qemu_dir.as_deref());
    }

    /// Point a duplicated machine at its own UEFI variable store.
    ///
    /// The store's name carries the machine's id, so a plain clone leaves both
    /// machines sharing one file and overwriting each other's boot entries.
    /// The copy gets a fresh path and, where the original store exists, its
    /// current contents: the guest was installed with those entries.
    ///
    /// Returns a message when the contents could not be carried over. The
    /// copy is still usable then — the next start creates a pristine store —
    /// but it will not boot the installed system, which the user should hear.
    fn rebase_nvram(&self, cloned: &mut VmConfig) -> Option<String> {
        let Firmware::Uefi { nvram } = &cloned.firmware else {
            return None;
        };
        let original = nvram.clone();
        let fresh = nvram_path_for(cloned);
        if fresh == original {
            return None;
        }
        cloned.firmware = Firmware::Uefi {
            nvram: fresh.clone(),
        };

        if !original.is_file() {
            return None;
        }
        if let Some(parent) = fresh.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        std::fs::copy(&original, &fresh).err().map(|e| {
            format!(
                "Could not copy the UEFI variable store to {}: {}",
                fresh.display(),
                e
            )
        })
    }

    /// Create the machine the wizard drew up.
    ///
    /// The order matters, so that a failure leaves nothing behind: claim the
    /// folder, move the paths into it, make the disk, make the variable
    /// store, and only then put the machine in the list. Every step before
    /// the last returns an error instead, and the list is untouched.
    ///
    /// The variable store comes after the disk because its path is derived
    /// from the disk's folder; the other way round the store would land
    /// beside the old disk and the machine would not start.
    pub fn create_from_wizard(&mut self, draft: VmConfig, disk_gb: u32) -> Result<Uuid, String> {
        let mut vm = draft;
        let root = self.settings.vm_dir_or_default();
        let folder = claim_vm_folder(&root, &vm.name, vm.id)?;

        if let Some(drive) = vm.drives.first_mut() {
            let file_name = drive
                .path
                .file_name()
                .map(|n| n.to_os_string())
                .unwrap_or_else(|| std::ffi::OsString::from("disk.qcow2"));
            drive.path = folder.join(file_name);
        }
        if matches!(vm.firmware, Firmware::Uefi { .. }) {
            // `nvram_path_for` derives the store's path from the first
            // drive's folder. A machine with no drive -- the empty tile, or
            // one switched to UEFI before a disk exists -- would get a
            // relative path, and a relative path is written next to the
            // executable. The claimed folder is where it belongs.
            let derived = nvram_path_for(&vm);
            vm.firmware = Firmware::Uefi {
                nvram: if derived.is_absolute() {
                    derived
                } else {
                    folder.join(derived.file_name().unwrap_or(derived.as_os_str()))
                },
            };
        }

        if let Some(drive) = vm.drives.first() {
            if !drive.path.is_file() {
                let qemu_img = find_qemu_img(self.settings.qemu_dir.as_deref())
                    .ok_or_else(|| "qemu-img was not found; cannot create a disk".to_string())?;
                create_disk_image(&qemu_img, &drive.path, drive.format, disk_gb)?;
            }
        }
        self.ensure_nvram(&vm)?;

        let id = vm.id;
        self.vms.push(vm);
        self.selected_vm_id = Some(id);
        self.save_vms_now();
        Ok(id)
    }

    /// Duplicate a machine so that the copy owns everything it writes to.
    ///
    /// Two QEMU processes writing one qcow2 destroy it, so a copy that shares
    /// the original's image is not a copy but a trap. Every image that lies
    /// inside the VM directory is copied into the new machine's own folder.
    /// An image outside it is left where it is and the two machines do share
    /// it — that file is the user's, not ours to duplicate into a place they
    /// never chose — and the sharing is said out loud instead of being
    /// discovered later as corruption.
    fn duplicate_vm(&mut self, id: Uuid) {
        let Some(pos) = self.vms.iter().position(|v| v.id == id) else {
            return;
        };
        let mut cloned = self.vms[pos].clone();
        cloned.id = Uuid::new_v4();
        cloned.name = format!("{} (Copy)", cloned.name);
        let new_id = cloned.id;

        let root = self.settings.vm_dir_or_default();

        // What becomes of each image is decided before anything is copied,
        // so that no control-flow path can skip a drive and leave it shared
        // in silence. Every drive gets exactly one fate, and every fate that
        // is not a copy is reported by name.
        let mut shared: Vec<(PathBuf, &'static str, String)> = Vec::new();
        let mut to_copy: Vec<usize> = Vec::new();
        for (index, drive) in cloned.drives.iter().enumerate() {
            match containment(&root, &drive.path) {
                Containment::Inside => to_copy.push(index),
                Containment::Outside => {
                    shared.push((drive.path.clone(), "duplicate_image_shared", String::new()))
                }
                Containment::Unresolvable => {
                    shared.push((drive.path.clone(), "duplicate_image_missing", String::new()))
                }
            }
        }

        // The folder is claimed once, and only when there is something to put
        // in it: a copy whose images all live elsewhere gets no empty folder.
        // If the claim fails, every image that was going to be copied becomes
        // a shared one — which is what it is — and is named as such.
        let folder = if to_copy.is_empty() {
            None
        } else {
            match claim_vm_folder(&root, &cloned.name, new_id) {
                Ok(dir) => Some(dir),
                Err(e) => {
                    for index in std::mem::take(&mut to_copy) {
                        shared.push((
                            cloned.drives[index].path.clone(),
                            "duplicate_folder_failed",
                            e.clone(),
                        ));
                    }
                    None
                }
            }
        };

        if let Some(dir) = folder {
            for index in to_copy {
                let source = cloned.drives[index].path.clone();
                let file_name = source
                    .file_name()
                    .map(|n| n.to_os_string())
                    .unwrap_or_else(|| std::ffi::OsString::from("disk.qcow2"));
                let target = dir.join(file_name);
                match std::fs::copy(&source, &target) {
                    Ok(_) => cloned.drives[index].path = target,
                    Err(e) => shared.push((source, "duplicate_image_copy_failed", e.to_string())),
                }
            }
        }

        let mut problems: Vec<String> = shared
            .into_iter()
            .map(|(path, key, detail)| {
                self.i18n.tf(
                    key,
                    &[
                        ("path", &path.display().to_string()),
                        ("error", detail.as_str()),
                    ],
                )
            })
            .collect();
        if let Some(problem) = self.rebase_nvram(&mut cloned) {
            problems.push(problem);
        }
        for problem in problems {
            self.push_notice(Notice::warning(problem));
        }
        self.vms.push(cloned);
        self.selected_vm_id = Some(new_id);
        self.save_vms_now();
    }

    /// The folder deleting this machine may take, if there is exactly one
    /// answer that is not a guess.
    ///
    /// Three things can go wrong, and each ends in `None` rather than in a
    /// choice made on the user's behalf:
    ///
    /// - **No folder claims the machine.** Nothing to offer.
    /// - **Several do.** A marker is a file, and files get copied: someone who
    ///   duplicated the folder in Explorer before a risky change has two.
    ///   The folder holding the machine's first disk breaks the tie, because
    ///   that is the one the machine actually runs from. With no disk to go
    ///   by, nothing is offered — destroying the wrong directory is worse
    ///   than leaving both.
    /// - **Another machine keeps files there.** The user pointed a second
    ///   VM's disk into this folder. That machine's dialog never mentioned
    ///   this path, so taking it would delete a disk behind the user's back.
    pub fn folder_to_delete_with(&self, id: Uuid) -> Option<PathBuf> {
        let vm = self.vms.iter().find(|v| v.id == id)?;
        let root = self.settings.vm_dir_or_default();
        let candidates = find_vm_folders(&root, id);

        let folder = match candidates.len() {
            0 => return None,
            1 => candidates.into_iter().next()?,
            _ => {
                let own = vm.drives.first()?.path.parent()?.canonicalize().ok()?;
                candidates
                    .into_iter()
                    .find(|c| c.canonicalize().ok().as_deref() == Some(own.as_path()))?
            }
        };

        let lodger = self.vms.iter().filter(|v| v.id != id).any(|other| {
            other
                .drives
                .iter()
                .map(|d| d.path.as_path())
                .chain(other.cdroms.iter().map(|p| p.as_path()))
                .chain(match &other.firmware {
                    Firmware::Uefi { nvram } => Some(nvram.as_path()),
                    Firmware::Bios { rom } => rom.as_deref(),
                })
                .any(|path| is_inside(&folder, path))
        });

        (!lodger).then_some(folder)
    }

    /// Remove a machine, and its folder only if the user asked for that.
    ///
    /// `with_folder` is the box in the delete dialog, which is never
    /// pre-ticked. Even with it ticked, only the folder whose marker names
    /// this VM and that is a direct child of the VM directory is removed: a
    /// disk the user pointed at their own documents must not drag that
    /// directory away with it.
    pub fn delete_vm(&mut self, id: Uuid, with_folder: bool) {
        let folder = with_folder
            .then(|| self.folder_to_delete_with(id))
            .flatten();

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

        // The folder goes last: a failure here must not leave a machine in
        // the list that points at files already gone.
        if let Some(folder) = folder {
            if let Err(e) = std::fs::remove_dir_all(&folder) {
                let text = self.i18n.tf(
                    "delete_folder_failed",
                    &[
                        ("path", &folder.display().to_string()),
                        ("error", &e.to_string()),
                    ],
                );
                self.push_notice(Notice::warning(text));
            }
        }
    }

    /// Everything worth saying about this machine before it starts.
    pub fn findings_for(&self, vm: &VmConfig) -> Vec<Finding> {
        check_vm(
            vm,
            &self.host,
            Some(&self.capabilities),
            self.uefi_firmware.as_ref(),
            &self.i18n,
        )
    }

    /// Make sure a UEFI machine has the variable store QEMU demands.
    ///
    /// The UI only records where the store belongs; this is what puts it
    /// there. It runs on every start, not just when UEFI is chosen, because
    /// the file can go missing between sessions — and QEMU's answer to a
    /// missing one is a refusal to start, not a fresh store.
    ///
    /// An existing store is left exactly as it is: it holds the boot entries
    /// the guest wrote, and overwriting them loses the installed system.
    pub fn ensure_nvram(&self, vm: &VmConfig) -> Result<(), String> {
        let Firmware::Uefi { nvram } = &vm.firmware else {
            return Ok(());
        };
        let Some(firmware) = self.uefi_firmware.as_ref() else {
            return Err(format!(
                "No UEFI firmware found for {}. Starting this machine as BIOS would                  install a guest that can never boot again, so the launch is refused.                  Set the QEMU directory in the settings.",
                vm.name
            ));
        };
        prepare_nvram(&firmware.vars_template, nvram)
    }

    /// Add a notice, unless the very same message is already on screen.
    ///
    /// One identical failure per click — which is what an unwritable
    /// configuration directory produces — would otherwise grow a band tall
    /// enough to push the menu bar, Exit included, off the window.
    fn push_notice(&mut self, notice: Notice) {
        const MAX_NOTICES: usize = 5;

        if self.notices.iter().any(|n| n.text == notice.text) {
            return;
        }
        if self.notices.len() >= MAX_NOTICES {
            self.notices.remove(0);
        }
        self.notices.push(notice);
    }

    /// Persist the VM list, reporting a failure instead of discarding it.
    ///
    /// A silently dropped write means the user's next start quietly loses the
    /// change they just made.
    fn save_vms_now(&mut self) {
        if self.vms_write_blocked {
            let notice = write_blocked_notice(&self.i18n, &self.vms_path);
            self.push_notice(notice);
            return;
        }
        if let Err(e) = save_vms(&self.vms_path, &self.vms) {
            let notice = save_failure_notice(&self.i18n, &self.vms_path, &e);
            self.push_notice(notice);
        }
    }

    /// Persist application settings, reporting a failure instead of discarding it.
    fn save_settings_now(&mut self) {
        if self.settings_write_blocked {
            let notice = write_blocked_notice(&self.i18n, &self.settings_path);
            self.push_notice(notice);
            return;
        }
        if let Err(e) = save_settings(&self.settings_path, &self.settings) {
            let notice = save_failure_notice(&self.i18n, &self.settings_path, &e);
            self.push_notice(notice);
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

        if let Err(err_msg) = self.ensure_nvram(&vm) {
            self.status_map.insert(id, VmStatus::Error(err_msg.clone()));
            let log_buf = self.console_logs.entry(id).or_default();
            log_buf.push_error(&err_msg);
            return Err(err_msg);
        }

        // The check runs after ensure_nvram, not before: the variable store
        // is ours to create, and a machine must not be held for a file the
        // app was about to write. What remains missing is someone else's.
        let findings = self.findings_for(&vm);
        if blocks(&findings) {
            let err_msg = findings
                .iter()
                .map(|f| f.text.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            self.status_map.insert(id, VmStatus::Error(err_msg.clone()));
            let log_buf = self.console_logs.entry(id).or_default();
            for finding in &findings {
                log_buf.push_error(&finding.text);
            }
            return Err(err_msg);
        }
        if !findings.is_empty() {
            let log_buf = self.console_logs.entry(id).or_default();
            for finding in &findings {
                log_buf.push_system(finding.text.clone());
            }
        }

        let args = match build_qemu_args(&vm, self.uefi_firmware.as_ref()) {
            Ok(args) => args,
            Err(err_msg) => {
                self.status_map.insert(id, VmStatus::Error(err_msg.clone()));
                let log_buf = self.console_logs.entry(id).or_default();
                log_buf.push_error(&err_msg);
                return Err(err_msg);
            }
        };
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
            SidebarAction::NewVm if self.settings.creation_mode == CreationMode::Wizard => {
                self.wizard = WizardState {
                    open: true,
                    ..Default::default()
                };
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
                self.duplicate_vm(id);
            }
            SidebarAction::DeleteVm { id, with_folder } => {
                self.delete_vm(id, with_folder);
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
                egui::RichText::new(self.i18n.t("empty_state_hint"))
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

        // 2. Notice band: problems with the stored configuration, above
        // everything else so they cannot be missed. Taken out of `self` for the
        // duration, because the band needs `notices` mutably and `i18n` for its
        // labels at the same time.
        if !self.notices.is_empty() {
            let mut notices = std::mem::take(&mut self.notices);
            egui::TopBottomPanel::top("notice_bar").show(ctx, |ui| {
                NoticeBar::new(&mut notices).show(ui, &self.i18n);
            });
            self.notices = notices;
        }

        // 3. Top Menu Bar (Global application commands & Settings)
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
        //
        // The folder the delete box would offer is worked out when the box
        // opens, not on every frame: finding it reads a directory. Opening
        // the box also clears the tick, so consent is never inherited from
        // the last machine the user deleted.
        if self.delete_folder_for != self.confirming_delete_id {
            self.delete_folder_for = self.confirming_delete_id;
            self.delete_with_folder = false;
            self.delete_folder = self
                .confirming_delete_id
                .and_then(|id| self.folder_to_delete_with(id));
        }
        let delete_folder = self.delete_folder.clone();
        let delete_folder_for = self.delete_folder_for;

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
                .with_delete_folder(
                    &mut self.delete_with_folder,
                    delete_folder.as_deref(),
                    delete_folder_for,
                )
                .with_logo(self.logo_texture.as_ref())
                .show(ui);
            });

        if let Some(action) = sidebar_action {
            self.handle_sidebar_action(action, Some(ctx));
        }

        // 4. Central Content Panel
        //
        // The selected machine's folder is worked out when the selection
        // changes, not on every frame: finding it reads a directory. The
        // detail panel needs it so a second disk is proposed beside the
        // machine's other files rather than next to the executable.
        if self.selected_folder_for != self.selected_vm_id {
            self.selected_folder_for = self.selected_vm_id;
            self.selected_folder = self
                .selected_vm_id
                .and_then(|id| find_vm_folders(&self.settings.vm_dir_or_default(), id).pop());
        }
        let selected_folder = self.selected_folder.clone();

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
                    let findings = self.findings_for(&self.vms[pos]);

                    let console_buf = self.console_logs.entry(id).or_default();
                    let mut view = VmDetailView::new(
                        &mut self.vms[pos],
                        is_running,
                        &self.i18n,
                        &mut self.active_tab,
                        qemu_img.as_deref(),
                        &mut self.create_disk_modal,
                    )
                    .with_vm_folder(selected_folder.as_deref())
                    .with_console(console_buf)
                    .with_logo(self.logo_texture.as_ref())
                    .with_capabilities(&self.capabilities)
                    .with_firmware(self.uefi_firmware.is_some())
                    .with_findings(&findings);

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
                self.refresh_capabilities();
                self.save_settings_now();
                apply_theme(ctx, &self.settings.theme);
                self.i18n.set_locale(&self.settings.language);
            }
        }

        // 5b. The template wizard
        if self.wizard.open {
            let action = WizardDialog::new(&mut self.wizard, &self.i18n)
                .with_capabilities(&self.capabilities)
                .with_firmware(self.uefi_firmware.is_some())
                .with_default_arch(self.settings.default_arch)
                .show(ctx);
            match action {
                Some(WizardAction::Create { vm, disk_gb }) => {
                    let name = vm.name.clone();
                    match self.create_from_wizard(vm, disk_gb) {
                        Ok(_) => self.wizard.created(),
                        Err(e) => {
                            // Nothing was added to the list. The dialog comes
                            // back with the template, the name and the ISO
                            // still in it, so the one thing that was wrong is
                            // the only thing to fix.
                            self.push_notice(Notice::error(format!(
                                "{}: {}",
                                self.i18n.tf("create_failed", &[("name", &name)]),
                                e
                            )));
                            self.wizard.creation_failed();
                        }
                    }
                }
                Some(WizardAction::Cancel) | None => {}
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
                        ui.heading(self.i18n.t("app_title"));
                        ui.label(
                            egui::RichText::new(self.i18n.t("app_copyright"))
                                .strong()
                                .size(12.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.add_space(8.0);
                        ui.label(self.i18n.t("app_tagline"));
                        ui.label(self.i18n.t("app_subtitle"));
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
        // Results are discarded deliberately: the window is going away, so
        // there is no interface left that could show a notice about a failed
        // write. The barriers are NOT optional here — this was the last place
        // that still overwrote a configuration it had promised to preserve.
        if !self.vms_write_blocked {
            let _ = save_vms(&self.vms_path, &self.vms);
        }
        if !self.settings_write_blocked {
            let _ = save_settings(&self.settings_path, &self.settings);
        }
    }
}

impl eframe::App for FaroQemApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_ui(ctx);
    }
}
