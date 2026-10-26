// FARO QEMU MANAGER (C) R.Trunk 2026
// Application State Integration & Main Controller

use std::collections::HashMap;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

use crate::config::{
    claim_vm_folder, containment, default_settings_path, default_vms_path, find_vm_folders,
    is_inside, quarantine_file, recovery_for, save_settings, save_vms, try_load_settings,
    try_load_vms, AppSettings, ConfigLoadError, ConfigRecovery, Containment, CreationMode,
    DriveConfig, DriveFormat, DriveInterface, Firmware, VmConfig,
};
use crate::download::{
    human_bytes, source_for, start_resolving, DownloadEvent, DownloadHandle, UreqHttp,
};
use crate::i18n::I18n;
use crate::preflight::{blocks, check_vm, Finding, HostFacts};
use crate::qemu::{
    build_qemu_args, create_disk_image, find_qemu_binary, find_qemu_img, find_uefi_firmware,
    nvram_path_for, prepare_nvram, probe_capabilities, spawn_qemu, QemuCapabilities, QemuEvent,
    UefiFirmware, VmProcess,
};
use crate::qmp::{connect_tcp, free_local_port, qmp_args, uuid_args, Greeting, QmpClient};
use crate::runstate::{self, now_secs, RunRecord};
use crate::templates::VmTemplate;
use crate::ui::console::ConsoleLogBuffer;
use crate::ui::notice::{Notice, NoticeBar};
use crate::ui::settings::SettingsDialog;
use crate::ui::sidebar::{SidebarAction, SidebarView, VmStatus};
use crate::ui::theme::{apply_theme, configure_modern_fonts, icons, COLOR_PRIMARY, COLOR_RUNNING};
use crate::ui::vm_detail::{CreateDiskState, DetailTab, VmDetailAction, VmDetailView};
use crate::ui::wizard::{WizardAction, WizardDialog, WizardState, PENDING_ISO};

/// How this application holds a running machine.
///
/// A machine started here comes with a process handle and a channel of its
/// output. A machine picked up again after the application was closed has
/// neither: the pipes belonged to the parent that ended, and the process is
/// nobody's child any more. It is still fully operable — the monitor and
/// the process number are what the stop paths need — so the difference
/// lives in the type rather than in a flag somebody forgets to check.
pub enum Attached {
    Owned {
        proc: VmProcess,
        rx: mpsc::Receiver<QemuEvent>,
    },
    Reattached {
        pid: u32,
    },
}

impl Attached {
    /// The machine's process number, however it is held.
    pub fn pid(&self) -> u32 {
        match self {
            Attached::Owned { proc, .. } => proc.pid(),
            Attached::Reattached { pid } => *pid,
        }
    }
}

/// A disk image being made in a thread.
///
/// What the frame thread needs to remember while `qemu-img` works: where
/// the answer arrives, which machine the image belongs to, and what to
/// attach once it is there.
pub struct DiskJob {
    rx: mpsc::Receiver<Result<(), String>>,
    attach_to: Option<Uuid>,
    target: PathBuf,
    format: DriveFormat,
}

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
    pub running_processes: HashMap<Uuid, Attached>,
    pub status_map: HashMap<Uuid, VmStatus>,
    /// The QMP port each running machine was started with. This is what
    /// makes a graceful shutdown possible at all; a machine missing from
    /// here can only be stopped the hard way, and the interface says so
    /// rather than quietly killing it.
    pub qmp_ports: HashMap<Uuid, u16>,
    /// Where the list of running machines is kept: `running.json`, beside
    /// `vms.json` and never inside it (spec section 7.3).
    pub runstate_path: PathBuf,
    /// `qemu-img`, resolved beside the capabilities.
    ///
    /// Searching for it meant a handful of `is_file` calls while drawing,
    /// sixty times a second, for an answer that only changes when the user
    /// picks a different directory (spec section 7.2).
    pub qemu_img: Option<PathBuf>,
    /// The emulator for the default architecture, resolved at the same
    /// time. The settings dialog shows it; a machine still resolves its own
    /// on launch, because that depends on the machine's architecture.
    pub qemu_binary: Option<PathBuf>,
    /// The disk image being made right now, if any.
    pub disk_job: Option<DiskJob>,
    /// When the reattached machines were last asked whether they are still
    /// there. They have no channel to report their end through.
    last_liveness_check: Instant,
    /// Whether `vms.json` was read successfully. A quarantined or refused
    /// list must not make the run state look stale.
    pub vms_loaded: bool,
    /// Whether the QEMU directory field in the settings has been typed in
    /// and not yet acted on. Probing QEMU costs five processes, so it waits
    /// until the field is done rather than running per keystroke.
    qemu_dir_edited: bool,
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
    /// Downloads in flight, one per machine. The worker does not watch its
    /// channel -- every send discards its error -- so dropping a handle does
    /// not stop it; setting its `cancel` flag does. Quitting is survivable
    /// because the thread is detached and the process exits.
    pub downloads: HashMap<Uuid, DownloadHandle>,
    /// What to say about each running download, shown in the detail panel.
    /// The console gets the technical lines; this is the one sentence.
    pub download_status: HashMap<Uuid, String>,
}

/// Point the machine at a downloaded image, without losing anything else.
///
/// Only the placeholder is replaced. A machine can have a second CD-ROM —
/// the virtio-win driver image, say — and the user can have named a file by
/// hand between creation and the mirror answering; neither is ours to throw
/// away.
fn replace_pending_iso(vm: &mut VmConfig, path: &Path) {
    if vm.cdroms.iter().any(|iso| iso == path) {
        return;
    }
    match vm
        .cdroms
        .iter()
        .position(|iso| iso == Path::new(PENDING_ISO))
    {
        Some(index) => vm.cdroms[index] = path.to_path_buf(),
        None => vm.cdroms.push(path.to_path_buf()),
    }
}

/// The partial file of a machine's first ISO, if it has one.
///
/// Used when a download is cancelled: the bytes belong to a machine that is
/// going away, and a `.part` nobody will ever resume is litter that the
/// start check would read as an unfinished download.
fn part_of_first_iso(app: &FaroQemApp, id: Uuid) -> PathBuf {
    app.vms
        .iter()
        .find(|v| v.id == id)
        .and_then(|vm| vm.cdroms.first())
        .map(|iso| crate::download::part_path(iso))
        .unwrap_or_default()
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

        // The run state lives beside the machine list, whatever path that
        // list was given -- a test with its own temporary directory must not
        // reach into the real configuration directory.
        let runstate_path = match vms_path.parent() {
            Some(dir) => dir.join("running.json"),
            None => PathBuf::from("running.json"),
        };

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
        // Whether the machine list is the user's real one. A quarantined or
        // refused file leaves this false, and then "this id is unknown" says
        // nothing about a machine -- only about the file. A file that was
        // never there is a different matter: it means there are no machines
        // yet, and that answer is trustworthy.
        let list_was_there = vms_path.exists();
        let mut vms_loaded = true;
        let (vms, vms_write_blocked) = match try_load_vms(&vms_path) {
            Ok(loaded) => (loaded, false),
            Err(why) => {
                vms_loaded = !list_was_there;
                let (notice, blocked) = recover_config(&i18n, &vms_path, &why);
                notices.extend(notice);
                (Vec::new(), blocked)
            }
        };

        // No sample machine is invented here. Seeding one was a convenience
        // while developing, but it meant that anyone installing FARO found a
        // machine called "Debian 12" they had never made -- and that the
        // program wrote vms.json to their disk before they had asked for
        // anything. An empty list is the honest state of a fresh install.

        let selected_vm_id = vms.first().map(|v| v.id);

        // Resolved once here, read by every frame afterwards. The emulator
        // is searched for anyway to probe the capabilities, so the answer is
        // kept rather than thrown away.
        let qemu_binary = find_qemu_binary(settings.qemu_dir.as_deref(), settings.default_arch);
        let qemu_img = find_qemu_img(settings.qemu_dir.as_deref());
        let capabilities = qemu_binary
            .as_deref()
            .map(probe_capabilities)
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

        let mut app = Self {
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
            qmp_ports: HashMap::new(),
            runstate_path,
            qemu_img,
            qemu_binary,
            disk_job: None,
            last_liveness_check: Instant::now(),
            vms_loaded,
            qemu_dir_edited: false,
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
            downloads: HashMap::new(),
            download_status: HashMap::new(),
            host: HostFacts::probe(),
        };

        // Last, with the machine list in hand: a record can only be matched
        // to a machine that is known, and an unmatched one is cleaned up --
        // but only if the list could be read at all.
        app.reattach_running_vms();
        app
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
        // The two resolved paths travel with the capabilities: all three
        // answers depend on which QEMU is in use.
        self.qemu_img = find_qemu_img(self.settings.qemu_dir.as_deref());
        self.qemu_binary = find_qemu_binary(
            self.settings.qemu_dir.as_deref(),
            self.settings.default_arch,
        );
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

        // The download goes first: the worker holds an append handle inside
        // the folder that may be about to be removed, and on success it
        // renames a multi-gigabyte image into it. Cancelling is what makes
        // `remove_dir_all` below a race against nothing.
        if let Some(handle) = self.downloads.remove(&id) {
            handle
                .cancel
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = std::fs::remove_file(part_of_first_iso(self, id));
        }
        self.download_status.remove(&id);

        // Deleting a machine takes its process with it: the files are about
        // to go, so asking the guest to shut down politely has nothing left
        // to shut down onto.
        let _ = self.force_stop_vm(id);
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

    /// Start fetching the ISO for a machine that already exists.
    ///
    /// Spec section 11.5: the machine is created at once, with the target
    /// path as its ISO, and the start check holds the start until the bytes
    /// are there and verified. Resolving today's filename means talking to a
    /// mirror, so that happens in the worker's own thread — a slow mirror
    /// must not freeze the moment of creation.
    fn begin_download(&mut self, id: Uuid, source_key: &str) {
        let Some(source) = source_for(source_key) else {
            return;
        };
        let Some(vm) = self.vms.iter().find(|v| v.id == id) else {
            return;
        };
        let folder = find_vm_folders(&self.settings.vm_dir_or_default(), id)
            .pop()
            .unwrap_or_else(|| self.settings.vm_dir_or_default());
        let name = vm.name.clone();

        self.push_notice(Notice::info(
            self.i18n.tf("download_resolving", &[("name", source.key)]),
        ));

        let handle = start_resolving(Arc::new(UreqHttp::new()), source, folder);
        self.downloads.insert(id, handle);
        self.log(
            id,
            format!("[DOWNLOAD] {}: resolving for {}", source.key, name),
        );
    }

    /// Read whatever the download workers have to say, without waiting.
    ///
    /// Called once per frame. `try_recv` in a loop rather than `recv`,
    /// because a frame that waits on a mirror is a frozen interface — the
    /// hard requirement in spec section 6.
    fn process_download_events(&mut self) {
        let ids: Vec<Uuid> = self.downloads.keys().copied().collect();
        for id in ids {
            let mut finished = false;
            let mut messages: Vec<String> = Vec::new();
            let mut arrived: Option<PathBuf> = None;
            let mut failure: Option<String> = None;
            let mut target: Option<PathBuf> = None;
            let mut progress: Option<(String, String)> = None;
            let mut verifying = false;

            if let Some(handle) = self.downloads.get(&id) {
                while let Ok(event) = handle.rx.try_recv() {
                    match event {
                        DownloadEvent::Target(path) => {
                            // Spec section 11.5: the machine was created with
                            // a placeholder, because the filename is only
                            // known once a mirror has been asked. Now it is.
                            // The start check sees the .part beside this and
                            // holds the start until the bytes are verified.
                            target = Some(path);
                        }
                        DownloadEvent::Progress { done, total } => {
                            let total_text =
                                total.map(human_bytes).unwrap_or_else(|| "?".to_string());
                            messages.push(format!(
                                "[DOWNLOAD] {} of {}",
                                human_bytes(done),
                                total_text
                            ));
                            progress = Some((human_bytes(done), total_text));
                        }
                        DownloadEvent::Verifying => {
                            messages.push("[DOWNLOAD] verifying the checksum".to_string());
                            verifying = true;
                        }
                        DownloadEvent::Done(path) => {
                            arrived = Some(path);
                            finished = true;
                        }
                        DownloadEvent::Failed(problem) => {
                            failure = Some(problem);
                            finished = true;
                        }
                    }
                }
            }

            for message in messages {
                self.log(id, message);
            }

            // The one sentence the detail panel shows. Verifying wins over
            // progress: it is the later state, and a bar sitting at 100%
            // while the digest is computed looks stuck.
            // The name comes from this drain's own Target when there is one:
            // the machine's ISO is still the placeholder until it is applied
            // a few lines below, and "Downloading : 0 B of 756 MB" names
            // nothing.
            let shown_name = target
                .as_deref()
                .or_else(|| {
                    self.vms
                        .iter()
                        .find(|v| v.id == id)
                        .and_then(|v| v.cdroms.first())
                        .map(|p| p.as_path())
                })
                .and_then(|iso| iso.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if verifying {
                let text = self.i18n.tf("download_verifying", &[("name", &shown_name)]);
                self.download_status.insert(id, text);
            } else if let Some((done, total)) = progress {
                let text = self.i18n.tf(
                    "download_running",
                    &[("name", &shown_name), ("done", &done), ("total", &total)],
                );
                self.download_status.insert(id, text);
            }

            if let Some(path) = target {
                if let Some(vm) = self.vms.iter_mut().find(|v| v.id == id) {
                    // Replace only the placeholder this download put there.
                    // Clearing the list would throw away a path the user
                    // chose by hand, or a second image they mounted since --
                    // which is the anti-goal in spec section 1: something
                    // quietly doing other than it says.
                    replace_pending_iso(vm, &path);
                }
                self.save_vms_now();
                self.log(id, format!("[DOWNLOAD] target: {}", path.display()));
            }

            if let Some(path) = arrived {
                let file = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                // The machine was created with the target path already in its
                // ISO slot, so nothing has to be rewritten here -- unless the
                // resolved name turned out different from the placeholder.
                if let Some(vm) = self.vms.iter_mut().find(|v| v.id == id) {
                    replace_pending_iso(vm, &path);
                }
                self.save_vms_now();
                let text = self.i18n.tf("download_done", &[("name", &file)]);
                self.push_notice(Notice::info(text));
                self.log(id, format!("[DOWNLOAD] done: {}", path.display()));
            }
            if let Some(problem) = failure {
                let name = self
                    .vms
                    .iter()
                    .find(|v| v.id == id)
                    .map(|v| v.name.clone())
                    .unwrap_or_default();
                let text = self
                    .i18n
                    .tf("download_failed", &[("name", &name), ("error", &problem)]);
                self.push_notice(Notice::error(text));
                self.log(id, format!("[DOWNLOAD] failed: {}", problem));
            }
            if finished {
                self.downloads.remove(&id);
                self.download_status.remove(&id);
            }
        }
    }

    /// Put a line in a machine's console.
    fn log(&mut self, id: Uuid, line: String) {
        self.console_logs.entry(id).or_default().push_system(line);
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

        for (&id, attached) in self.running_processes.iter_mut() {
            // A reattached machine has no channel: its output goes to its
            // log file, which the reattach read into the console once.
            let Attached::Owned { proc, rx } = attached else {
                continue;
            };
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
            // The monitor died with the process; a port left behind here
            // would offer a shutdown that cannot reach anything, and a
            // record left behind would be a ghost at the next start.
            self.qmp_ports.remove(&id);
            self.forget_running(id);
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

        let mut args = match build_qemu_args(&vm, self.uefi_firmware.as_ref()) {
            Ok(args) => args,
            Err(err_msg) => {
                self.status_map.insert(id, VmStatus::Error(err_msg.clone()));
                let log_buf = self.console_logs.entry(id).or_default();
                log_buf.push_error(&err_msg);
                return Err(err_msg);
            }
        };

        // The monitor the shutdown button speaks through. The port is asked
        // for, never chosen: on this host whole ranges refuse to be bound.
        // Failing to find one is not a reason to refuse the machine -- it
        // starts, and the panel then says that only the hard stop is left.
        self.qmp_ports.remove(&id);
        match free_local_port() {
            Ok(port) => {
                args.extend(qmp_args(port));
                // The machine's own id as its machine UUID, so the reattach
                // can ask *which* machine answers on a recorded port rather
                // than only whether some QEMU does.
                args.extend(uuid_args(&id.to_string()));
                self.qmp_ports.insert(id, port);
            }
            Err(e) => {
                let log_buf = self.console_logs.entry(id).or_default();
                log_buf.push_error(format!(
                    "[QMP] no monitor for this machine: {} -- only a hard stop will be available",
                    e
                ));
            }
        }

        let cli_str = format!("{} {}", bin_path.display(), shell_words::join(&args));
        self.cli_previews.insert(id, cli_str);

        let (tx, rx) = mpsc::channel();
        // Everything the machine says also goes into its own file, so a
        // guest that outlived the application still has a console to come
        // back to (spec section 7.3, point 4).
        let log_path = self.log_path_for(id);
        match spawn_qemu(&bin_path, &args, tx, Some(&log_path)) {
            Ok(proc) => {
                let pid = proc.pid();
                self.running_processes
                    .insert(id, Attached::Owned { proc, rx });
                self.status_map.insert(id, VmStatus::Running { pid });
                self.record_running(id, pid);
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

    /// Whether this application still holds the machine's process.
    pub fn is_attached(&self, id: Uuid) -> bool {
        self.running_processes.contains_key(&id)
    }

    /// Where this machine's console log lives.
    ///
    /// Inside the machine's own folder when it has one, so the log goes
    /// when the folder goes; otherwise under `logs/` beside the
    /// configuration, named after the machine.
    pub fn log_path_for(&self, id: Uuid) -> PathBuf {
        if let Some(folder) = find_vm_folders(&self.settings.vm_dir_or_default(), id).pop() {
            return folder.join("console.log");
        }
        let base = self
            .runstate_path
            .parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_default();
        base.join("logs").join(format!("{}.log", id))
    }

    /// Note this machine in the run state file, so the next start of the
    /// application can find it again.
    ///
    /// A failure is reported on the machine's console and nowhere else: the
    /// guest is running, and a scratch file that could not be written is no
    /// reason to tell the user their machine failed.
    fn record_running(&mut self, id: Uuid, pid: u32) {
        let Some(&qmp_port) = self.qmp_ports.get(&id) else {
            // Nothing to reattach to without a monitor, and a record
            // claiming otherwise would offer a shutdown that cannot work.
            return;
        };
        let record = RunRecord {
            vm_id: id,
            pid,
            qmp_port,
            started_at: now_secs(),
            log: self.log_path_for(id),
        };
        if let Err(e) = runstate::put(&self.runstate_path, record) {
            self.console_logs
                .entry(id)
                .or_default()
                .push_error(format!("[STATE] {}", e));
        }
    }

    /// Drop this machine from the run state file.
    fn forget_running(&mut self, id: Uuid) {
        let _ = runstate::remove(&self.runstate_path, id);
    }

    /// Pick up the machines that outlived the last run of the application.
    ///
    /// The record is a lead, not proof. A process number proves nothing —
    /// the operating system hands them out again — and a handshake on the
    /// recorded port proves only that *a* QEMU is there, which on a reused
    /// loopback port can be a different machine of ours. Both have to agree,
    /// and the machine has to say its own id: only then is it adopted.
    ///
    /// Anything that cannot be confirmed is treated as ended and its record
    /// dropped (spec section 7.3). Nothing is ever killed here: whatever is
    /// on the other end of an unconfirmable record is somebody else's
    /// program.
    pub fn reattach_running_vms(&mut self) {
        for record in runstate::load(&self.runstate_path) {
            let known = self.vms.iter().any(|v| v.id == record.vm_id);
            if !known {
                if !self.vms_loaded {
                    // The machine list could not be read, so "unknown" says
                    // nothing about this record. Part 1 took great care never
                    // to overwrite an unreadable vms.json; deleting a
                    // perfectly good running.json beside it would orphan
                    // every guest for good -- the record is the only way
                    // back to them once the configuration is repaired.
                    continue;
                }
                // The machine was deleted while it was running, or the file
                // belongs to another configuration.
                self.forget_running(record.vm_id);
                continue;
            }

            match confirm_identity(record.qmp_port, record.vm_id) {
                Ok(greeting) => {
                    self.running_processes
                        .insert(record.vm_id, Attached::Reattached { pid: record.pid });
                    self.qmp_ports.insert(record.vm_id, record.qmp_port);
                    self.status_map
                        .insert(record.vm_id, VmStatus::Running { pid: record.pid });
                    self.read_console_from_log(record.vm_id, &record.log);
                    let log_buf = self.console_logs.entry(record.vm_id).or_default();
                    log_buf.push_system(format!(
                        "[QMP] reattached to QEMU {}.{}.{} with PID {}",
                        greeting.major, greeting.minor, greeting.micro, record.pid
                    ));
                }
                Err(_) => {
                    self.forget_running(record.vm_id);
                }
            }
        }
    }

    /// Fill a reattached machine's console from its log file.
    ///
    /// The pipes belonged to the parent that ended, so the file is the only
    /// account of what the guest said. The first line says where this comes
    /// from: a console that silently mixes a file with live output would
    /// make the user guess which is which.
    fn read_console_from_log(&mut self, id: Uuid, log: &Path) {
        let text = match std::fs::read_to_string(log) {
            Ok(text) => text,
            Err(e) => {
                let notice = self
                    .i18n
                    .tf("console_log_unreadable", &[("error", &e.to_string())]);
                self.console_logs.entry(id).or_default().push_error(notice);
                return;
            }
        };

        // Only the tail: the buffer is for reading, and a capped log can
        // still hold tens of thousands of lines.
        let lines: Vec<&str> = text.lines().collect();
        let start = lines.len().saturating_sub(CONSOLE_FROM_LOG_LINES);
        let log_buf = self.console_logs.entry(id).or_default();
        log_buf.push_system(
            self.i18n
                .tf("console_from_log", &[("path", &log.display().to_string())]),
        );
        for line in &lines[start..] {
            log_buf.push_stdout(*line);
        }
    }

    /// A monitor for this machine, with its identity proven on the spot.
    ///
    /// The record is a lead at every use, not only at start-up. A machine
    /// this application did not start is held by a process number and a
    /// port, and both are handed out again by the operating system: by the
    /// time a button is pressed, the number can belong to a stranger and
    /// the port to a different machine of ours. So the handshake and the
    /// machine's own id are checked here, every time, before anything is
    /// sent or stopped.
    ///
    /// A machine that cannot prove itself counts as ended and is cleaned up
    /// (spec section 7.3). Nothing is killed on that path.
    fn proven_monitor(&mut self, id: Uuid) -> Result<(QmpClient<TcpStream>, Greeting), String> {
        let Some(&port) = self.qmp_ports.get(&id) else {
            return Err(self.i18n.t("shutdown_unavailable").to_string());
        };
        let (mut client, greeting) = connect_qmp(port).map_err(|e| {
            self.mark_ended(id);
            format!("127.0.0.1:{}: {}", port, e)
        })?;

        let reported = client.machine_uuid().inspect_err(|_| {
            self.mark_ended(id);
        })?;
        if !reported.eq_ignore_ascii_case(&id.to_string()) {
            // Someone else's machine is on that port now. Letting a command
            // through here would shut down a guest the user never named.
            self.mark_ended(id);
            return Err(format!(
                "127.0.0.1:{} answers for {}, not for {}",
                port, reported, id
            ));
        }
        Ok((client, greeting))
    }

    /// End a reattached machine through its monitor.
    ///
    /// `quit` was checked against QEMU 11.1.0: it ends the process and
    /// answers with a SHUTDOWN event and a return. A machine that cannot
    /// prove its identity is not killed by any means -- it counts as ended
    /// and is cleaned up (spec section 7.3). That can leave a guest running
    /// that nobody holds any more, which is the lesser harm: the
    /// alternative is killing a stranger.
    fn force_stop_reattached(&mut self, id: Uuid) -> Result<(), String> {
        match self.proven_monitor(id) {
            Ok((mut client, _)) => {
                let outcome = client.quit();
                let events = client.take_events();
                drop(client);
                self.running_processes.remove(&id);
                self.qmp_ports.remove(&id);
                self.forget_running(id);
                self.status_map.insert(id, VmStatus::Stopped);
                let log_buf = self.console_logs.entry(id).or_default();
                match &outcome {
                    Ok(()) => log_buf.push_system("[QMP] quit; the machine has ended"),
                    Err(e) => log_buf.push_error(format!("[QMP] quit refused: {}", e)),
                }
                for event in events {
                    log_buf.push_system(format!("[QMP] {}", event));
                }
                outcome
            }
            Err(reason) => {
                // proven_monitor has already cleaned the machine up.
                self.console_logs
                    .entry(id)
                    .or_default()
                    .push_error(format!("[QMP] {}", reason));
                Ok(())
            }
        }
    }

    /// Treat this machine as ended: nothing left to hold, nothing killed.
    fn mark_ended(&mut self, id: Uuid) {
        self.running_processes.remove(&id);
        self.qmp_ports.remove(&id);
        self.forget_running(id);
        self.status_map.insert(id, VmStatus::Stopped);
        self.console_logs.entry(id).or_default().push_system(
            "[QMP] the monitor no longer answers for this machine; it counts as ended",
        );
    }

    /// Ask the machines this application only reattached to whether they are
    /// still there.
    ///
    /// A machine it started itself reports its end through its channel. A
    /// reattached one has no channel, so without this it would stay
    /// "running" for the rest of the session -- the start button silently
    /// doing nothing, and the hard way the only thing left.
    pub fn recheck_reattached(&mut self) {
        let reattached: Vec<Uuid> = self
            .running_processes
            .iter()
            .filter(|(_, a)| matches!(a, Attached::Reattached { .. }))
            .map(|(id, _)| *id)
            .collect();
        for id in reattached {
            let Some(&port) = self.qmp_ports.get(&id) else {
                self.mark_ended(id);
                continue;
            };
            match confirm_identity_within(port, id, LIVENESS_BUDGET) {
                Ok(_) => {}
                Err(_) => self.mark_ended(id),
            }
        }
        self.last_liveness_check = Instant::now();
    }

    /// Make a disk image without freezing the window.
    ///
    /// `qemu-img` needs seconds to minutes for a large image, and the frame
    /// thread cannot wait that long (spec section 7.2). The work goes to a
    /// thread; `poll_disk_creation` picks up the answer. The finished image
    /// is attached to the machine that was selected when this started —
    /// which is the machine whose panel the dialog belongs to.
    pub fn start_disk_creation(&mut self, target: PathBuf, format: DriveFormat, size_gb: u32) {
        let Some(img) = self.qemu_img.clone() else {
            self.create_disk_modal.status_msg =
                Some(Err(self.i18n.t("qemu_img_missing").to_string()));
            return;
        };
        if self.disk_job.is_some() {
            // One at a time: two qemu-img runs on one path would fight.
            self.create_disk_modal.status_msg = Some(Ok(self.i18n.t("disk_creating").to_string()));
            return;
        }

        let (tx, rx) = mpsc::channel();
        let job_target = target.clone();
        let worker = std::thread::Builder::new()
            .name("faro-create-disk".to_string())
            .spawn(move || {
                let outcome = create_disk_image(&img, &job_target, format, size_gb);
                let _ = tx.send(outcome);
            });

        match worker {
            Ok(_) => {
                self.disk_job = Some(DiskJob {
                    rx,
                    attach_to: self.selected_vm_id,
                    target,
                    format,
                });
            }
            Err(e) => {
                self.create_disk_modal.status_msg = Some(Err(e.to_string()));
            }
        }
    }

    /// Collect the result of a disk being made, if there is one yet.
    ///
    /// Called once per frame and never waits. A finished image is attached
    /// to the machine that asked for it and the list is saved, so the
    /// machine does not point at a file that only exists in this session.
    pub fn poll_disk_creation(&mut self) {
        let Some(job) = &self.disk_job else {
            return;
        };
        let outcome = match job.rx.try_recv() {
            Ok(outcome) => outcome,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("the disk image worker stopped without saying why".to_string())
            }
        };

        let job = self.disk_job.take().expect("checked just above");
        self.create_disk_modal.busy = false;
        match outcome {
            Ok(()) => {
                if let Some(vm) = self.vms.iter_mut().find(|v| Some(v.id) == job.attach_to) {
                    vm.drives.push(DriveConfig {
                        path: job.target,
                        format: job.format,
                        interface: DriveInterface::VirtIO,
                        boot_index: None,
                    });
                }
                self.create_disk_modal.status_msg =
                    Some(Ok(self.i18n.t("disk_created").to_string()));
                // Done and attached, so the dialog has nothing left to say.
                self.create_disk_modal.open = false;
                self.save_vms_now();
            }
            Err(e) => {
                // Said twice on purpose: in the dialog, which stays open for
                // it, and as a notice -- the one place that survives the
                // user having closed the dialog in the meantime. A write
                // that failed must be reported, not swallowed (spec 4).
                self.create_disk_modal.status_msg = Some(Err(e.clone()));
                self.create_disk_modal.open = true;
                let notice = Notice::error(self.i18n.tf("disk_create_failed", &[("error", &e)]));
                self.push_notice(notice);
            }
        }
    }

    /// The machines that keep running when this window closes.
    ///
    /// Closing is not a reason to kill a guest (spec section 7.3), so the
    /// user is told which ones carry on -- stated, not asked: carrying on
    /// is the wanted behaviour, and a dialog that interrupts a shutdown of
    /// the host to ask about it would be in the way.
    pub fn surviving_vm_names(&self) -> Vec<String> {
        self.running_processes
            .keys()
            .map(|id| match self.vms.iter().find(|v| v.id == *id) {
                Some(vm) => vm.name.clone(),
                None => id.to_string(),
            })
            .collect()
    }

    /// Forget this machine's monitor port.
    ///
    /// What a machine started by an older version of the application looks
    /// like, and what happens when no port could be found. The guest keeps
    /// running; only the graceful path is gone.
    pub fn drop_qmp_port(&mut self, id: Uuid) {
        self.qmp_ports.remove(&id);
    }

    /// Ask the guest to shut itself down.
    ///
    /// This is what the stop button does, and it never kills (spec section
    /// 7.1). Without a monitor, or when the monitor does not answer, the
    /// machine is left exactly as it was and the reason is returned so the
    /// interface can say it -- silently falling back to killing would make a
    /// liar of the button.
    pub fn shutdown_vm(&mut self, id: Uuid) -> Result<(), String> {
        let (mut client, greeting) = match self.proven_monitor(id) {
            Ok(pair) => pair,
            Err(reason) => {
                self.console_logs
                    .entry(id)
                    .or_default()
                    .push_error(format!("[QMP] {}", reason));
                return Err(reason);
            }
        };

        if let Err(e) = client.powerdown() {
            self.console_logs
                .entry(id)
                .or_default()
                .push_error(format!("[QMP] system_powerdown refused: {}", e));
            return Err(e);
        }

        let pid = self
            .status_map
            .get(&id)
            .and_then(|s| s.pid())
            .or_else(|| self.running_processes.get(&id).map(|a| a.pid()))
            .unwrap_or(0);
        self.status_map.insert(
            id,
            VmStatus::ShuttingDown {
                pid,
                since: SystemTime::now(),
            },
        );

        let log_buf = self.console_logs.entry(id).or_default();
        log_buf.push_system(format!(
            "[QMP] shutdown requested of QEMU {}.{}.{}",
            greeting.major, greeting.minor, greeting.micro
        ));
        // What the machine itself reported, not what we assume it did.
        for event in client.take_events() {
            log_buf.push_system(format!("[QMP] {}", event));
        }
        Ok(())
    }

    /// Pull the plug on a virtual machine.
    ///
    /// The second, named step: the user asked for this one explicitly, after
    /// a shutdown that did not finish or on a machine that has no monitor.
    pub fn force_stop_vm(&mut self, id: Uuid) -> Result<(), String> {
        // A machine this application did not start is nobody's child. There
        // is no handle, only a recorded process number -- and the operating
        // system hands those out again, so taskkill on one would reach
        // whoever holds it now. Its hard way goes through its own monitor,
        // identity proven first: what dies is what just said who it is.
        if matches!(
            self.running_processes.get(&id),
            Some(Attached::Reattached { .. })
        ) {
            return self.force_stop_reattached(id);
        }

        self.qmp_ports.remove(&id);
        self.forget_running(id);
        if let Some(attached) = self.running_processes.remove(&id) {
            // Its own child: the handle is the identity, and killing it can
            // reach nothing else.
            let (res, rx) = match attached {
                Attached::Owned { mut proc, rx } => (proc.kill(), Some(rx)),
                // Handled above; kept exhaustive rather than unreachable.
                Attached::Reattached { .. } => (Ok(()), None),
            };
            // Read what is still in the channel before it is dropped. Taking
            // the receiver away with the process threw out QEMU's last words
            // and its exit event, which is exactly what a user looks for
            // after a stop that surprised them.
            let deadline = Instant::now() + LAST_WORDS_BUDGET;
            let log_buf = self.console_logs.entry(id).or_default();
            log_buf.push_system("[PROCESS] QEMU process terminated");
            while let (Some(rx), true) = (rx.as_ref(), Instant::now() < deadline) {
                match rx.recv_timeout(Duration::from_millis(25)) {
                    Ok(event) => {
                        let ended = matches!(event, QemuEvent::Exited(_));
                        log_buf.handle_event(&event);
                        if ended {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
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
                self.wizard = WizardState::opened();
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
            VmDetailAction::Shutdown(id) => {
                // The reason is already on the machine's console; nothing is
                // killed when this fails.
                let _ = self.shutdown_vm(id);
            }
            VmDetailAction::ForceStop(id) => {
                let _ = self.force_stop_vm(id);
            }
            VmDetailAction::Save(_id) => {
                self.save_vms_now();
            }
            VmDetailAction::CreateDisk {
                id,
                path,
                format,
                size_gb,
            } => {
                // The dialog belongs to the machine whose panel it was
                // opened from, so that machine gets the image.
                self.selected_vm_id = Some(id);
                self.start_disk_creation(path, format, size_gb);
            }
            VmDetailAction::Download(id) => {
                if let Some(key) = self
                    .vms
                    .iter()
                    .find(|v| v.id == id)
                    .and_then(|v| v.template.as_deref())
                    .and_then(VmTemplate::by_key)
                    .and_then(|t| t.iso_source)
                {
                    self.begin_download(id, key);
                }
            }
            VmDetailAction::CancelDownload(id) => {
                // Stopping also clears the partial: a `.part` nobody will
                // resume is what the start check reads as an unfinished
                // download, so leaving it would hold the start for ever.
                if let Some(handle) = self.downloads.remove(&id) {
                    handle
                        .cancel
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                }
                self.download_status.remove(&id);
                let part = part_of_first_iso(self, id);
                let _ = std::fs::remove_file(&part);
                if let Some(vm) = self.vms.iter_mut().find(|v| v.id == id) {
                    // The placeholder meant "waiting for a download". There
                    // is no longer one to wait for.
                    vm.cdroms.retain(|iso| iso != Path::new(PENDING_ISO));
                }
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
        // Emptied without waiting: a frame that blocks on a mirror is the
        // frozen interface spec section 6 forbids.
        self.process_download_events();
        // Same for a disk image being made: asked after, never waited on.
        self.poll_disk_creation();
        // A reattached machine has no channel to report its end through, so
        // it is asked -- rarely, and with a short deadline.
        if self.last_liveness_check.elapsed() >= LIVENESS_EVERY {
            self.recheck_reattached();
        }

        // eframe draws when something happens. A hanging guest says nothing,
        // so nothing would ask for the next frame: the waiting state would
        // stand at "0 s" and the hard way would never appear beside it
        // (Review Focus 2). The same goes for the liveness check above and
        // for a disk image being made in a thread.
        if self.disk_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if !self.running_processes.is_empty() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

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

        // Worked out before the menu is drawn: the closure below holds the
        // application mutably, and this reads it.
        let surviving = self.surviving_vm_names();

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
                    // Said before the click, not after: a notice shown while
                    // the window is going away is a notice nobody reads.
                    if !surviving.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                self.i18n
                                    .tf("vms_keep_running", &[("names", &surviving.join(", "))]),
                            )
                            .size(11.0)
                            .color(COLOR_RUNNING),
                        );
                    }
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
                            egui::Button::new(format!(
                                "{} {}",
                                icons::STOP,
                                self.i18n.t("shutdown")
                            )),
                        )
                        .clicked()
                    {
                        if let Some(id) = self.selected_vm_id {
                            menu_detail_action = Some(VmDetailAction::Shutdown(id));
                        }
                        ui.close_menu();
                    }
                    // The hard way is its own entry here too, never a
                    // fallback hidden behind the one above.
                    if ui
                        .add_enabled(
                            has_vm && is_running,
                            egui::Button::new(format!(
                                "{} {}",
                                icons::STOP,
                                self.i18n.t("force_stop")
                            )),
                        )
                        .clicked()
                    {
                        if let Some(id) = self.selected_vm_id {
                            menu_detail_action = Some(VmDetailAction::ForceStop(id));
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
                // Wrapped for the same reason as the panel's action row: a
                // right-aligned button that does not fit used to be placed
                // outside the window, where it cannot be clicked.
                let corner = egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true);
                ui.with_layout(corner, |ui| {
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
        let download_status = self
            .selected_vm_id
            .and_then(|id| self.download_status.get(&id).cloned());
        // Offering to fetch means: this machine came from a template with a
        // mirror, nothing is running for it, and the image is not there yet.
        // That last part is what makes one broken connection recoverable.
        let can_resume = self
            .selected_vm_id
            .filter(|id| !self.downloads.contains_key(id))
            .and_then(|id| self.vms.iter().find(|v| v.id == id))
            .filter(|vm| {
                vm.template
                    .as_deref()
                    .and_then(VmTemplate::by_key)
                    .and_then(|t| t.iso_source)
                    .and_then(source_for)
                    .is_some()
            })
            .is_some_and(|vm| vm.cdroms.is_empty() || vm.cdroms.iter().any(|iso| !iso.is_file()));

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
                    let shutting_down_since = self
                        .status_map
                        .get(&id)
                        .and_then(|s| s.shutting_down_since());
                    let cli_cmd = self.cli_previews.get(&id).map(|s| s.as_str());
                    let qemu_img = self.qemu_img.clone();
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
                    .with_download_status(download_status.as_deref())
                    .with_resumable_download(can_resume)
                    .with_console(console_buf)
                    .with_logo(self.logo_texture.as_ref())
                    .with_capabilities(&self.capabilities)
                    .with_firmware(self.uefi_firmware.is_some())
                    .with_shutdown(self.qmp_ports.contains_key(&id), shutting_down_since)
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
            // Cloned out of the field first: the dialog borrows the settings
            // mutably, and the resolved path lives beside them.
            let detected = self.qemu_binary.clone();
            let mut edited = self.qemu_dir_edited;
            let mut dialog =
                SettingsDialog::new(&mut self.settings_open, &mut self.settings, &mut self.i18n)
                    .with_detected_qemu(detected.as_deref())
                    .editing_qemu_dir(&mut edited);
            let modified = dialog.show(ctx);
            // The dialog's borrows end here; the flag goes back to the app.
            self.qemu_dir_edited = edited;
            // A path typed and then closed away without leaving the field
            // still has to take effect -- otherwise the setting would sit
            // in memory, unsaved and unprobed.
            let closed_with_an_unfinished_edit = !self.settings_open && self.qemu_dir_edited;
            if modified || closed_with_an_unfinished_edit {
                self.qemu_dir_edited = false;
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
                    let wanted_download = self.wizard.wants_download();
                    let source = vm
                        .template
                        .as_deref()
                        .and_then(VmTemplate::by_key)
                        .and_then(|t| t.iso_source);
                    match self.create_from_wizard(vm, disk_gb) {
                        Ok(id) => {
                            if wanted_download {
                                if let Some(key) = source {
                                    self.begin_download(id, key);
                                }
                            }
                            self.wizard.created()
                        }
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
                        ui.add_space(8.0);
                        // Which build, and under what terms. The GPL asks
                        // that a program under it say so where the person
                        // running it will look, and this window is that
                        // place. The number comes from the package, so it
                        // cannot drift from Cargo.toml.
                        ui.label(
                            egui::RichText::new(
                                self.i18n
                                    .tf("version", &[("version", env!("CARGO_PKG_VERSION"))]),
                            )
                            .size(12.0),
                        );
                        ui.label(
                            egui::RichText::new(self.i18n.t("license_notice"))
                                .size(11.0)
                                .color(ui.visuals().weak_text_color()),
                        );
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

/// How long to keep trying to reach a machine's monitor.
///
/// A monitor that is there answers in well under a millisecond, so this
/// budget is not paid in normal use; it covers the moment right after a
/// start, when QEMU has been handed the port but has not finished binding
/// it. A connection to a port nothing is listening on is refused at once
/// rather than hanging, which is why retrying -- not a single long timeout
/// -- is what bounds this.
///
/// Measured, not guessed: on this host a freshly launched QEMU accepts on
/// its monitor port after 511, 512, 535 and 542 ms across four runs
/// (q35 with WHPX, 256 MB, no display). So anything under about 700 ms
/// would make a shutdown right after a start fail -- and this budget is
/// paid in full only when nothing is listening at all, where the user is
/// waiting for a sentence rather than for a connection. 1.2 seconds
/// leaves room above the measurement without reading as a freeze.
const QMP_CONNECT_BUDGET: Duration = Duration::from_millis(1200);

/// How long a stop waits for the process's last lines and its exit.
///
/// The reader threads are still draining the pipes when `kill` returns, and
/// the receiving end is about to be dropped. Half a second is enough for a
/// process that has just been killed and short enough not to be felt as a
/// freeze; what does not arrive in time is in the machine's log file.
const LAST_WORDS_BUDGET: Duration = Duration::from_millis(500);

/// How many lines of a machine's log file go into its console on reattach.
///
/// Enough to see how the guest got where it is, bounded so that a capped
/// but still large log does not go into memory whole.
const CONSOLE_FROM_LOG_LINES: usize = 500;

/// How long a reattach waits for an answer on a recorded port.
///
/// Short on purpose: this runs at start-up, once per recorded machine, and
/// a port nothing listens on is refused immediately. A machine that cannot
/// answer quickly here is treated as ended, which costs nothing — it is
/// still running, and the next start tries again.
const REATTACH_BUDGET: Duration = Duration::from_millis(1500);

/// How long the repeated check of a reattached machine may take, and how
/// often it runs.
///
/// Short, because it happens while the window is drawing: a monitor that is
/// there answers in under a millisecond, and a dead port is refused at
/// once. Only a listener that accepts and then says nothing costs the full
/// budget, and then only every few seconds.
const LIVENESS_BUDGET: Duration = Duration::from_millis(250);
const LIVENESS_EVERY: Duration = Duration::from_secs(5);

/// Confirm that the machine behind a recorded port is the machine claimed.
///
/// Two things have to hold: a QMP handshake succeeds (so a QEMU is there at
/// all), and the machine reports the id it was started with (so it is
/// *this* machine and not another of ours on a port the operating system
/// handed out again). A machine from an older version, started before the
/// identity was stamped on, reports a different UUID and is therefore not
/// adopted — it keeps running, and the hard stop stays available.
fn confirm_identity(port: u16, expected: Uuid) -> Result<Greeting, String> {
    confirm_identity_within(port, expected, REATTACH_BUDGET)
}

/// `confirm_identity` with the caller's own deadline.
///
/// The repeated liveness check wants a short one: it runs while the window
/// is drawing, and a port nothing listens on is refused at once.
fn confirm_identity_within(
    port: u16,
    expected: Uuid,
    budget: Duration,
) -> Result<Greeting, String> {
    let (mut client, greeting) = connect_tcp(port, budget)?;
    let reported = client.machine_uuid()?;
    if reported.eq_ignore_ascii_case(&expected.to_string()) {
        Ok(greeting)
    } else {
        Err(format!(
            "127.0.0.1:{} answers for {}, not for {}",
            port, reported, expected
        ))
    }
}

/// Reach a machine's monitor, giving a just-started QEMU time to be there.
fn connect_qmp(port: u16) -> Result<(QmpClient<TcpStream>, Greeting), String> {
    let deadline = Instant::now() + QMP_CONNECT_BUDGET;
    loop {
        match connect_tcp(port, QMP_CONNECT_BUDGET) {
            Ok(pair) => return Ok(pair),
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(e);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

impl Drop for FaroQemApp {
    fn drop(&mut self) {
        // The guests stay up. Closing the window used to kill every one of
        // them, which is the opposite of what spec section 7.3 asks for:
        // a long installation must survive the manager being closed. The
        // handles are let go of without a `kill`, the run state file keeps
        // the port and the number, and the next start reattaches.
        self.running_processes.clear();
        // The downloads are a different matter: their worker threads die
        // with this process anyway, and a half-written `.part` is resumable
        // by design. Asking them to stop is tidier than being torn down
        // mid-write.
        for handle in self.downloads.values() {
            handle
                .cancel
                .store(true, std::sync::atomic::Ordering::SeqCst);
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
