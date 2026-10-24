// FARO QEMU MANAGER (C) R.Trunk 2026
// The run state file: its place, its resilience, and one entry at a time.

mod common;

use std::path::PathBuf;
use tempfile::tempdir;
use uuid::Uuid;

use faro_qem::config::default_vms_path;
use faro_qem::runstate::{default_runstate_path, load, put, remove, RunRecord};

/// A record with the fields spelled out, so a test reads as data.
fn record(pid: u32, port: u16) -> RunRecord {
    RunRecord {
        vm_id: Uuid::new_v4(),
        pid,
        qmp_port: port,
        started_at: 1_700_000_000,
        log: PathBuf::from(format!("vm-{}.log", pid)),
    }
}

#[test]
fn test_a_record_survives_a_write_and_read() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    let mut rec = record(4242, 52400);
    rec.log = dir.path().join("vm.log");
    put(&path, rec.clone()).unwrap();
    assert_eq!(load(&path), vec![rec]);
}

#[test]
fn test_the_runstate_file_is_not_vms_json() {
    // Spec section 7.3: vms.json stays the truth the user maintains.
    assert_eq!(default_runstate_path().file_name().unwrap(), "running.json");
    assert_eq!(
        default_runstate_path().parent(),
        default_vms_path().parent()
    );
}

#[test]
fn test_a_corrupt_file_yields_nothing_and_is_not_fatal() {
    // Review Focus 3: a hand-edited or truncated file must not produce a
    // ghost machine, and must not stop the application from starting.
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    for junk in [
        "",
        "not json",
        "{}",
        "[{\"pid\": \"not a number\"}]",
        "[{\"vm_id\": \"not-a-uuid\", \"pid\": 1, \"qmp_port\": 1, \"started_at\": 1, \"log\": \"x\"}]",
        "[{\"vm_id\": \"3f2504e0-4f89-11d3-9a0c-0305e82c3301\"}]",
    ] {
        std::fs::write(&path, junk).unwrap();
        assert!(load(&path).is_empty(), "{:?} produced ghosts", junk);
    }
}

#[test]
fn test_putting_one_entry_leaves_the_others_alone() {
    // Review Focus 4: two instances of the app share this file.
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    let a = record(1, 1001);
    let b = record(2, 1002);
    put(&path, a.clone()).unwrap();
    put(&path, b.clone()).unwrap();
    let loaded = load(&path);
    assert_eq!(loaded.len(), 2, "got: {:?}", loaded);
    assert!(loaded.contains(&a) && loaded.contains(&b));
}

#[test]
fn test_putting_the_same_vm_twice_replaces_rather_than_duplicates() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    let id = Uuid::new_v4();
    let mut first = record(1, 1001);
    first.vm_id = id;
    let mut second = record(9, 1009);
    second.vm_id = id;
    put(&path, first).unwrap();
    put(&path, second).unwrap();
    let loaded = load(&path);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].pid, 9);
    assert_eq!(loaded[0].qmp_port, 1009);
}

#[test]
fn test_removing_an_entry_keeps_the_rest_and_a_missing_one_is_no_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    let keep = record(1, 1001);
    let drop_me = record(2, 1002);
    put(&path, keep.clone()).unwrap();
    put(&path, drop_me.clone()).unwrap();
    remove(&path, drop_me.vm_id).unwrap();
    assert_eq!(load(&path), vec![keep]);
    remove(&path, Uuid::new_v4()).unwrap();
}

#[test]
fn test_a_corrupt_file_is_replaced_rather_than_appended_to() {
    // The other half of Review Focus 3: nothing readable was there, so the
    // next write has to produce a valid file rather than a second document
    // glued onto rubbish.
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    std::fs::write(&path, "half a { list").unwrap();
    let rec = record(7, 1007);
    put(&path, rec.clone()).unwrap();
    assert_eq!(load(&path), vec![rec]);
}

#[test]
fn test_launching_and_force_stopping_add_and_drop_the_record() {
    if common::qemu_missing() {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    }
    let dir = tempdir().unwrap();
    // A subdirectory, so the app's own files land inside the temporary
    // directory rather than beside it -- see app_with_vm_dir.
    let root = dir.path().join("vms");
    let mut app = common::app_with_vm_dir(&root);
    let id = common::push_startable_vm(&mut app, &root);
    app.launch_vm(id, None).unwrap();

    let while_running = load(&app.runstate_path);
    assert_eq!(while_running.len(), 1, "got: {:?}", while_running);
    assert_eq!(while_running[0].vm_id, id);
    assert_eq!(
        Some(while_running[0].qmp_port),
        app.qmp_ports.get(&id).copied(),
        "the recorded port must be the one the guest listens on"
    );

    app.force_stop_vm(id).unwrap();
    assert!(
        load(&app.runstate_path).is_empty(),
        "a stopped VM leaves no record"
    );
}
