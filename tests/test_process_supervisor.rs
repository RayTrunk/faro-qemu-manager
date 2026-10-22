use faro_qem::qemu::{spawn_qemu, QemuEvent, VmProcess};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

fn find_binary(candidates: &[&str]) -> Option<PathBuf> {
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            for cand in candidates {
                let candidate_path = path.join(cand);
                if candidate_path.is_file() {
                    return Some(candidate_path);
                }
                #[cfg(windows)]
                {
                    let exe_path = path.join(format!("{}.exe", cand));
                    if exe_path.is_file() {
                        return Some(exe_path);
                    }
                }
            }
        }
    }
    None
}

fn get_rustc_bin() -> PathBuf {
    if let Some(r) = std::env::var_os("RUSTC") {
        let p = PathBuf::from(r);
        if p.exists() {
            return p;
        }
    }
    find_binary(&["rustc"]).unwrap_or_else(|| PathBuf::from("rustc"))
}

fn get_python_bin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("PYTHON") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    find_binary(&["python", "python3", "py"])
}

fn drain_events_until_exit(rx: &Receiver<QemuEvent>, timeout: Duration) -> Vec<QemuEvent> {
    let mut events = Vec::new();
    let start = Instant::now();

    while start.elapsed() < timeout {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                let is_exit = matches!(event, QemuEvent::Exited(_));
                events.push(event);
                if is_exit {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    events
}

#[test]
fn test_spawn_nonexistent_binary() {
    let (tx, rx) = mpsc::channel();
    let bad_path = Path::new("faro_qem_non_existent_binary_xyz_12345.exe");
    let result = spawn_qemu(bad_path, &[], tx);

    assert!(result.is_err(), "Spawning nonexistent binary should return Err");

    // Also verify any channel event or error
    let events: Vec<_> = rx.try_iter().collect();
    if !events.is_empty() {
        assert!(
            matches!(&events[0], QemuEvent::Error(_)),
            "If an event was sent, it should be Error"
        );
    }
}

#[test]
fn test_spawn_rustc_stdout_streaming_and_exit() {
    let (tx, rx) = mpsc::channel();
    let rustc = get_rustc_bin();
    let args = vec!["--version".to_string()];

    let vm: VmProcess = spawn_qemu(&rustc, &args, tx).expect("Failed to spawn rustc --version");

    assert!(vm.pid() > 0, "PID should be greater than 0");
    let debug_str = format!("{:?}", vm);
    assert!(debug_str.contains("VmProcess"), "Debug representation should include struct name");

    let events = drain_events_until_exit(&rx, Duration::from_secs(5));

    // Must have Started(pid)
    assert!(
        events.iter().any(|e| matches!(e, QemuEvent::Started(pid) if *pid == vm.pid())),
        "Should receive Started with matching PID. Events: {:?}",
        events
    );

    // Must have Stdout containing "rustc"
    assert!(
        events.iter().any(|e| match e {
            QemuEvent::Stdout(line) => line.to_lowercase().contains("rustc"),
            _ => false,
        }),
        "Should receive Stdout with rustc version info. Events: {:?}",
        events
    );

    // Must have Exited(Some(0))
    assert!(
        events.iter().any(|e| matches!(e, QemuEvent::Exited(Some(0)))),
        "Should receive Exited(Some(0)). Events: {:?}",
        events
    );

    // Give supervisor thread a moment to update alive status
    std::thread::sleep(Duration::from_millis(50));
    assert!(!vm.is_alive(), "Process should not be alive after exit");
}

#[test]
fn test_spawn_rustc_stderr_streaming_and_non_zero_exit() {
    let (tx, rx) = mpsc::channel();
    let rustc = get_rustc_bin();
    let args = vec!["--invalid-faro-flag-xyz-test".to_string()];

    let vm = spawn_qemu(&rustc, &args, tx).expect("Failed to spawn rustc with invalid flag");

    let events = drain_events_until_exit(&rx, Duration::from_secs(5));

    // Must have Started
    assert!(
        events.iter().any(|e| matches!(e, QemuEvent::Started(_))),
        "Should receive Started event. Events: {:?}",
        events
    );

    // Must have Stderr
    assert!(
        events.iter().any(|e| matches!(e, QemuEvent::Stderr(_))),
        "Should receive Stderr lines for invalid flag. Events: {:?}",
        events
    );

    // Exit code should be non-zero
    assert!(
        events.iter().any(|e| matches!(e, QemuEvent::Exited(Some(code)) if *code != 0)),
        "Should receive Exited with non-zero code. Events: {:?}",
        events
    );

    std::thread::sleep(Duration::from_millis(50));
    assert!(!vm.is_alive(), "Process should be dead");
}

#[test]
fn test_spawn_multiple_lines_stdout_and_stderr() {
    let (tx, rx) = mpsc::channel();

    let (bin, args) = if let Some(py) = get_python_bin() {
        let script = "import sys\n\
                      print('OUT_1')\n\
                      print('OUT_2')\n\
                      sys.stderr.write('ERR_1\\n')\n\
                      sys.stderr.flush()\n\
                      print('OUT_3')\n\
                      sys.stderr.write('ERR_2\\n')\n\
                      sys.stderr.flush()\n";
        (py, vec!["-u".to_string(), "-c".to_string(), script.to_string()])
    } else {
        #[cfg(windows)]
        {
            (
                PathBuf::from("cmd.exe"),
                vec![
                    "/c".to_string(),
                    "echo OUT_1& echo OUT_2& echo ERR_1 1>&2& echo OUT_3& echo ERR_2 1>&2".to_string(),
                ],
            )
        }
        #[cfg(not(windows))]
        {
            (
                PathBuf::from("sh"),
                vec![
                    "-c".to_string(),
                    "echo OUT_1; echo OUT_2; echo ERR_1 >&2; echo OUT_3; echo ERR_2 >&2".to_string(),
                ],
            )
        }
    };

    let vm = spawn_qemu(&bin, &args, tx).expect("Failed to spawn multi-line command");

    let events = drain_events_until_exit(&rx, Duration::from_secs(5));

    let stdout_lines: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            QemuEvent::Stdout(line) => Some(line.trim().to_string()),
            _ => None,
        })
        .collect();

    let stderr_lines: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            QemuEvent::Stderr(line) => Some(line.trim().to_string()),
            _ => None,
        })
        .collect();

    assert!(
        stdout_lines.iter().any(|l| l.contains("OUT_1")),
        "Expected OUT_1 in stdout: {:?}",
        stdout_lines
    );
    assert!(
        stdout_lines.iter().any(|l| l.contains("OUT_2")),
        "Expected OUT_2 in stdout: {:?}",
        stdout_lines
    );
    assert!(
        stdout_lines.iter().any(|l| l.contains("OUT_3")),
        "Expected OUT_3 in stdout: {:?}",
        stdout_lines
    );

    assert!(
        stderr_lines.iter().any(|l| l.contains("ERR_1")),
        "Expected ERR_1 in stderr: {:?}",
        stderr_lines
    );
    assert!(
        stderr_lines.iter().any(|l| l.contains("ERR_2")),
        "Expected ERR_2 in stderr: {:?}",
        stderr_lines
    );

    assert!(
        events.iter().any(|e| matches!(e, QemuEvent::Exited(Some(0)))),
        "Should exit with 0. Events: {:?}",
        events
    );

    std::thread::sleep(Duration::from_millis(50));
    assert!(!vm.is_alive());
}

#[test]
fn test_vm_process_kill_terminates_running_process() {
    let (tx, rx) = mpsc::channel();

    let (bin, args) = if let Some(py) = get_python_bin() {
        let script = "import time, sys\n\
                      sys.stdout.write('ONLINE_NOW\\n')\n\
                      sys.stdout.flush()\n\
                      time.sleep(30)\n";
        (py, vec!["-u".to_string(), "-c".to_string(), script.to_string()])
    } else {
        #[cfg(windows)]
        {
            (
                PathBuf::from("powershell.exe"),
                vec![
                    "-Command".to_string(),
                    "Write-Host 'ONLINE_NOW'; Start-Sleep -Seconds 30".to_string(),
                ],
            )
        }
        #[cfg(not(windows))]
        {
            (
                PathBuf::from("sh"),
                vec![
                    "-c".to_string(),
                    "echo ONLINE_NOW; sleep 30".to_string(),
                ],
            )
        }
    };

    let mut vm = spawn_qemu(&bin, &args, tx).expect("Failed to spawn long-running process");

    // Wait until child signals ONLINE_NOW
    let start = Instant::now();
    let mut saw_online = false;
    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(QemuEvent::Stdout(l)) = rx.recv_timeout(Duration::from_millis(100)) {
            if l.contains("ONLINE_NOW") {
                saw_online = true;
                break;
            }
        }
    }
    assert!(saw_online, "Child process should output ONLINE_NOW before kill");

    // Process should be alive right now
    assert!(vm.is_alive(), "Process should be alive before kill");

    // Kill the process
    let kill_res = vm.kill();
    assert!(kill_res.is_ok(), "vm.kill() should succeed: {:?}", kill_res);

    // Wait for Exited event
    let exit_events = drain_events_until_exit(&rx, Duration::from_secs(5));
    assert!(
        exit_events.iter().any(|e| matches!(e, QemuEvent::Exited(_))),
        "Should receive Exited event after kill. Events: {:?}",
        exit_events
    );

    // Process should now report not alive
    std::thread::sleep(Duration::from_millis(50));
    assert!(!vm.is_alive(), "Process should be reported dead after kill");

    // Calling kill() again on an already dead process should be idempotent (Ok)
    let second_kill = vm.kill();
    assert!(
        second_kill.is_ok(),
        "Subsequent kill() on dead process should return Ok(())"
    );
}

#[test]
fn test_spawn_with_arguments_preserves_spaces_and_quotes() {
    let (tx, rx) = mpsc::channel();
    let py = match get_python_bin() {
        Some(p) => p,
        None => return, // Skip if python is not present
    };

    let script = "import sys\n\
                  print(f'ARG1:{sys.argv[1]}')\n\
                  print(f'ARG2:{sys.argv[2]}')\n";

    let args = vec![
        "-u".to_string(),
        "-c".to_string(),
        script.to_string(),
        "Hello World with Spaces".to_string(),
        "Quotes: \"and\" 'single'".to_string(),
    ];

    let _vm = spawn_qemu(&py, &args, tx).expect("Failed to spawn python with spaced args");

    let events = drain_events_until_exit(&rx, Duration::from_secs(5));

    let stdout_lines: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            QemuEvent::Stdout(l) => Some(l.clone()),
            _ => None,
        })
        .collect();

    assert!(
        stdout_lines.iter().any(|l| l.contains("ARG1:Hello World with Spaces")),
        "ARG1 should preserve spaces. Stdout: {:?}",
        stdout_lines
    );
    assert!(
        stdout_lines.iter().any(|l| l.contains("ARG2:Quotes: \"and\" 'single'")),
        "ARG2 should preserve quotes. Stdout: {:?}",
        stdout_lines
    );
}
