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
