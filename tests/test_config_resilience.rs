// FARO QEMU MANAGER (C) R.Trunk 2026
// Resilience of configuration persistence: atomic writes, corrupt files, quarantine

use faro_qem::config::{save_settings, save_vms, AppSettings, Arch, VmConfig};
use tempfile::tempdir;

#[test]
fn test_concurrent_reader_never_sees_a_half_written_config() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");

    // Large enough that a non-atomic write has an observable truncated window.
    let vms: Vec<VmConfig> = (0..2000)
        .map(|i| VmConfig::new_default(format!("VM {}", i), Arch::X86_64))
        .collect();
    save_vms(&path, &vms).expect("seed the file");

    // Every write stores the same payload, so a complete file always has this
    // length. Sampling the length rather than parsing keeps the reader fast
    // enough to actually catch the window.
    let full_len = std::fs::metadata(&path).unwrap().len();
    assert!(full_len > 100_000, "payload must be big enough to tear");

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let writer = {
        let path = path.clone();
        let vms = vms.clone();
        let stop = std::sync::Arc::clone(&stop);
        std::thread::spawn(move || {
            for _ in 0..200 {
                save_vms(&path, &vms).expect("write keeps succeeding");
            }
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
        })
    };

    // A reader must always find a COMPLETE config. A zero-length file counts as
    // torn: std::fs::write truncates the target before refilling it, so zero
    // bytes is the most common half-written state there is.
    let mut torn = 0usize;
    let mut reads = 0usize;
    while !stop.load(std::sync::atomic::Ordering::SeqCst) {
        if let Ok(meta) = std::fs::metadata(&path) {
            reads += 1;
            if meta.len() != full_len {
                torn += 1;
            }
        }
    }
    writer.join().unwrap();

    assert!(reads > 0, "the reader has to have observed something");
    assert_eq!(
        torn, 0,
        "a reader saw {} torn file(s) out of {} reads — the write is not atomic",
        torn, reads
    );
}

#[test]
fn test_save_leaves_no_temporary_files_behind() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");

    save_vms(&path, &[VmConfig::new_default("A", Arch::X86_64)]).expect("save succeeds");

    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(entries, vec!["vms.json".to_string()]);
}

// Review Focus 5
#[test]
fn test_save_creates_missing_parent_directories() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("deep").join("deeper").join("settings.json");

    save_settings(&path, &AppSettings::default()).expect("save into fresh tree");

    assert!(path.is_file());
}
