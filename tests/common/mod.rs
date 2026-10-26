// FARO QEMU MANAGER (C) R.Trunk 2026
// Shared test helpers: ask QEMU whether it accepts a command line.

#![allow(dead_code)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use faro_qem::config::{Arch, DriveFormat};
use faro_qem::qemu::{create_disk_image, find_qemu_binary, find_qemu_img};

/// How long to let QEMU run before concluding it accepted the arguments.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// What the installed QEMU thinks of a command line.
#[derive(Debug)]
pub enum Acceptance {
    /// QEMU started and was still running when we stopped it.
    Accepted,
    /// QEMU exited with an error; `first_line` is its first complaint.
    Rejected { first_line: String },
    /// No QEMU for this architecture on this machine.
    NoQemu,
}

/// Start QEMU with `args` and report whether it accepted them.
///
/// QEMU has no dry-run mode, so acceptance can only be established by actually
/// starting it. `-S` keeps the guest paused so no guest code runs, and
/// `-display none` keeps a window from appearing; both go last so they cannot
/// be overridden by the configuration under test.
pub fn qemu_accepts(arch: Arch, args: &[String]) -> Acceptance {
    let Some(binary) = find_qemu_binary(None, arch) else {
        return Acceptance::NoQemu;
    };

    let mut cmd = Command::new(&binary);
    cmd.args(args);
    cmd.args(["-S", "-display", "none"]);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Acceptance::Rejected {
                first_line: format!("could not start {}: {}", binary.display(), e),
            }
        }
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            // Exited cleanly on its own — nothing was refused.
            Ok(Some(status)) if status.success() => return Acceptance::Accepted,
            Ok(Some(_)) => {
                let first_line = read_first_stderr_line(&mut child);
                return Acceptance::Rejected { first_line };
            }
            Ok(None) => {
                if started.elapsed() >= PROBE_TIMEOUT {
                    // Still running, so the arguments were accepted.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Acceptance::Accepted;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                return Acceptance::Rejected {
                    first_line: format!("could not wait for QEMU: {}", e),
                };
            }
        }
    }
}

/// Start a paused QEMU with the given extra arguments, for probing a live
/// server.
///
/// `-S` keeps the guest stopped so no guest code runs, `-display none` keeps a
/// window from appearing, and 128 MB is enough for a machine that will never
/// boot anything. The caller owns the child and must kill it.
pub fn spawn_paused_qemu(binary: &Path, extra: &[String]) -> std::process::Child {
    let mut cmd = Command::new(binary);
    cmd.args(["-machine", "q35", "-m", "128M", "-S", "-display", "none"]);
    cmd.args(extra);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd.spawn().expect("QEMU starts");
    // The QMP server is listening before the monitor greets a client, but not
    // instantly. This keeps the first attempt from landing before the socket
    // exists; the caller's timeout covers the rest.
    std::thread::sleep(Duration::from_millis(300));
    child
}

/// First line QEMU wrote to stderr, for use in a failure message.
fn read_first_stderr_line(child: &mut std::process::Child) -> String {
    use std::io::Read;

    let mut text = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut text);
    }
    text.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("(QEMU said nothing)")
        .to_string()
}

/// Fail the test unless QEMU accepts `args`, naming what was being probed.
///
/// Skips silently when QEMU is absent, following the precedent of
/// `test_create_disk_image_with_system_qemu_img_if_present`.
pub fn assert_accepted(label: &str, arch: Arch, args: &[String]) {
    match qemu_accepts(arch, args) {
        Acceptance::Accepted => {}
        Acceptance::NoQemu => eprintln!("skipped {}: no qemu-system-{}", label, arch.as_str()),
        Acceptance::Rejected { first_line } => panic!(
            "QEMU rejected {}:\n  {}\n  args: {}",
            label,
            first_line,
            args.join(" ")
        ),
    }
}

/// Create a small disk image so drives point at something real.
///
/// Does nothing when `qemu-img` is absent; the acceptance probe skips in that
/// case anyway.
pub fn make_disk_image(path: &Path, format: DriveFormat) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Some(img) = find_qemu_img(None) {
        let _ = create_disk_image(&img, path, format, 1);
    }
}

/// `make_disk_image` for the common case.
pub fn make_empty_qcow2(path: &Path) {
    make_disk_image(path, DriveFormat::Qcow2);
}

/// All text egui actually drew this frame.
pub fn rendered_text(output: &egui::FullOutput) -> String {
    fn walk(shape: &egui::Shape, out: &mut String) {
        match shape {
            egui::Shape::Text(t) => {
                out.push_str(t.galley.text());
                out.push('\n');
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    walk(s, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut out);
    }
    out
}

/// The area egui actually painted this frame.
///
/// `rendered_text` answers *what* was drawn; this answers *where*. A
/// control laid out past the right edge of the window is invisible and
/// unreachable, and no assertion about text can see that.
pub fn drawn_bounds(output: &egui::FullOutput) -> egui::Rect {
    fn walk(shape: &egui::Shape, out: &mut egui::Rect) {
        match shape {
            egui::Shape::Text(t) => {
                *out = out.union(egui::Rect::from_min_size(t.pos, t.galley.size()));
            }
            egui::Shape::Rect(r) => {
                *out = out.union(r.rect);
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    walk(s, out);
                }
            }
            _ => {}
        }
    }
    let mut out = egui::Rect::NOTHING;
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut out);
    }
    out
}

/// Keys present in one locale catalogue, read from the file.
///
/// `I18n::t` falls back to English for a missing key, so asking it whether a
/// key "exists" cannot tell a German entry apart from an English one standing
/// in for it. The files are the only honest source.
pub fn catalogue_keys(lang: &str) -> std::collections::BTreeSet<String> {
    let path = format!("assets/locales/{}.json", lang);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {}", path, e));
    let map: std::collections::BTreeMap<String, String> = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{} is not a catalogue: {}", path, e));
    map.into_keys().collect()
}

/// An `Http` that serves a body slowly enough to still be running when the
/// test looks at it.
///
/// Used where a download has to be in flight — deleting a machine mid
/// transfer, for instance — rather than finished before the next line runs.
pub struct SlowHttp {
    body: Vec<u8>,
}

impl SlowHttp {
    pub fn serving(body: &[u8]) -> Self {
        Self {
            body: body.to_vec(),
        }
    }
}

impl faro_qem::download::Http for SlowHttp {
    fn get_text(&self, url: &str) -> Result<String, String> {
        Err(format!("{}: this fake only serves a body", url))
    }

    fn head_ok(&self, _url: &str) -> bool {
        true
    }

    fn get_from(
        &self,
        _url: &str,
        offset: u64,
    ) -> Result<(Box<dyn std::io::Read + Send>, Option<u64>), String> {
        let from = (offset as usize).min(self.body.len());
        Ok((
            Box::new(Dawdler {
                data: self.body[from..].to_vec(),
                pos: 0,
            }),
            Some(self.body.len() as u64),
        ))
    }
}

struct Dawdler {
    data: Vec<u8>,
    pos: usize,
}

impl std::io::Read for Dawdler {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::thread::sleep(Duration::from_millis(50));
        let left = self.data.len() - self.pos;
        if left == 0 {
            return Ok(0);
        }
        let take = buf.len().min(left).min(256);
        buf[..take].copy_from_slice(&self.data[self.pos..self.pos + take]);
        self.pos += take;
        Ok(take)
    }
}

/// Every Rust source file under `dir`, concatenated.
///
/// Used to ask questions about the whole interface at once — whether a key is
/// referenced anywhere, whether a hardcoded string survived — which no single
/// file can answer. `dir` is a subtree so that part 4 can ask the same
/// question about one module.
pub fn concat_sources(dir: &str) -> String {
    fn walk(dir: &Path, out: &mut String) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.push_str(&text);
                    out.push('\n');
                }
            }
        }
    }

    let mut out = String::new();
    walk(Path::new(dir), &mut out);
    assert!(!out.is_empty(), "no Rust sources under {}", dir);
    out
}

/// An app whose VM folders go into `vm_dir`.
///
/// Used from four test files, which is why it lives here rather than being
/// copied into each.
///
/// The app's own files -- `settings.json`, `vms.json`, and since part 4
/// `running.json` -- land in the **parent** of `vm_dir`. Pass a
/// subdirectory of the temporary directory (`dir.path().join("vms")`),
/// never `dir.path()` itself: that would put them in the system temporary
/// directory, shared with every other test and every other run, and the
/// run state would then accumulate machines from runs long gone.
pub fn app_with_vm_dir(vm_dir: &Path) -> faro_qem::app::FaroQemApp {
    let parent = vm_dir.parent().unwrap_or(vm_dir);
    let mut app = faro_qem::app::FaroQemApp::with_paths(
        parent.join("settings.json"),
        parent.join("vms.json"),
        None,
    );
    app.settings.vm_dir = Some(vm_dir.to_path_buf());
    app
}

/// Some long-lived process that is not one of our machines.
///
/// Stands in for the stranger a recycled process number points at. The
/// caller owns it and must kill it.
pub fn spawn_long_sleep() -> std::process::Child {
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("powershell.exe");
        c.args(["-Command", "Start-Sleep -Seconds 60"]);
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = Command::new("sleep");
        c.arg("60");
        c
    };
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.spawn().expect("a long-lived process starts")
}

/// An app on given paths, whose VM folders go into `vm_dir`.
///
/// For the reattach tests, which need a second app on the **same** files:
/// `app_with_vm_dir` derives the paths from the directory, this takes them.
pub fn app_with_paths_and_vm_dir(
    settings: &Path,
    vms: &Path,
    vm_dir: &Path,
) -> faro_qem::app::FaroQemApp {
    let mut app =
        faro_qem::app::FaroQemApp::with_paths(settings.to_path_buf(), vms.to_path_buf(), None);
    app.settings.vm_dir = Some(vm_dir.to_path_buf());
    app
}

/// Whether a process with this number is still there.
///
/// Asked of the operating system, not of the application: the question
/// under test is whether a guest outlived the app, and the app's own
/// bookkeeping cannot answer that.
pub fn pid_is_alive(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .output();
        match out {
            // Without a match tasklist prints "INFO: No tasks are running
            // which match the specified criteria.", which holds no number.
            Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()),
            Err(_) => false,
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Kill a process by number, so no guest outlives the test run.
pub fn kill_pid(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("taskkill");
        c.args(["/F", "/T", "/PID", &pid.to_string()]);
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = Command::new("kill");
        c.args(["-9", &pid.to_string()]);
        c
    };
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

/// Whether this machine has a QEMU for the usual architecture.
///
/// Tests that start a real guest skip without one rather than fail: the
/// absence of QEMU is a fact about the machine, not about the code.
pub fn qemu_missing() -> bool {
    faro_qem::qemu::find_qemu_binary(None, Arch::X86_64).is_none()
}

/// A machine that really starts, pushed into the app's list.
///
/// A small qcow2 made by `qemu-img` so the start check finds its disk, SATA
/// so it looks like what the wizard produces, and no display at all so the
/// suite stays quiet. BIOS rather than UEFI, so the test does not also
/// depend on edk2 being installed.
pub fn push_startable_vm(app: &mut faro_qem::app::FaroQemApp, dir: &Path) -> uuid::Uuid {
    use faro_qem::config::{DriveConfig, DriveInterface, VmConfig};

    let disk = dir.join(format!("startable-{}.qcow2", uuid::Uuid::new_v4()));
    make_empty_qcow2(&disk);

    let mut vm = VmConfig::new_default("startable", Arch::X86_64);
    vm.memory_mb = 256;
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::Sata,
        boot_index: Some(0),
    });
    vm.display.vga_type = "none".to_string();
    vm.display.display_backend = "none".to_string();

    let id = vm.id;
    app.vms.push(vm);
    // Written to disk the way the interface writes it, so a second app on the
    // same files knows this machine -- which the reattach tests depend on.
    app.handle_vm_detail_action(faro_qem::ui::vm_detail::VmDetailAction::Save(id), None);
    id
}

/// Everything the settings dialog draws, after a layout pass.
pub fn rendered_settings(
    settings: &mut faro_qem::config::AppSettings,
    i18n: &mut faro_qem::i18n::I18n,
) -> String {
    rendered_settings_with_qemu(settings, i18n, None)
}

/// The settings dialog, told which emulator the application resolved.
pub fn rendered_settings_with_qemu(
    settings: &mut faro_qem::config::AppSettings,
    i18n: &mut faro_qem::i18n::I18n,
    detected: Option<&Path>,
) -> String {
    let ctx = egui::Context::default();
    let mut open = true;
    let _ = ctx.run(Default::default(), |ctx| {
        faro_qem::ui::SettingsDialog::new(&mut open, settings, i18n)
            .with_detected_qemu(detected)
            .show(ctx);
    });
    let output = ctx.run(Default::default(), |ctx| {
        faro_qem::ui::SettingsDialog::new(&mut open, settings, i18n)
            .with_detected_qemu(detected)
            .show(ctx);
    });
    rendered_text(&output)
}

/// One plain machine in the app's list, and its id.
///
/// The application used to seed a sample machine on a first start, and many
/// tests leaned on it being there. It no longer does -- a fresh install must
/// not invent a machine -- so a test that needs one says so here. No disk
/// image and no files: for a test that only needs a machine to exist, the
/// machine is the point, not its storage.
pub fn push_plain_vm(app: &mut faro_qem::app::FaroQemApp) -> uuid::Uuid {
    use faro_qem::config::VmConfig;

    let mut vm = VmConfig::new_default("Test VM", app.settings.default_arch);
    vm.machine_type = vm.arch.default_machine().to_string();
    vm.cpu_model = vm.arch.default_cpu().to_string();
    let id = vm.id;
    app.vms.push(vm);
    app.selected_vm_id = Some(id);
    id
}
