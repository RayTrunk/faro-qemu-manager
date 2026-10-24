// FARO QEMU MANAGER (C) R.Trunk 2026
// What is running right now: its own file, beside the configuration.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::{default_vms_path, write_config_file};

/// One running machine, as the next start of the application will find it.
///
/// `pid` alone proves nothing — the operating system hands numbers out
/// again — which is why `qmp_port` travels with it: the handshake on that
/// port is the identity proof from spec section 7.3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub vm_id: Uuid,
    pub pid: u32,
    pub qmp_port: u16,
    /// Seconds since the Unix epoch, so the file stays readable by a human
    /// and by a future version that has forgotten this struct.
    pub started_at: u64,
    pub log: PathBuf,
}

/// `running.json`, beside `vms.json`.
///
/// Deliberately not inside `vms.json`: that file is the truth the user
/// maintains, and process numbers have no business in it (spec section
/// 7.3).
pub fn default_runstate_path() -> PathBuf {
    let vms = default_vms_path();
    match vms.parent() {
        Some(dir) => dir.join("running.json"),
        None => PathBuf::from("running.json"),
    }
}

/// What the file says is running, or nothing.
///
/// Every failure — missing, empty, truncated, hand-edited, a field of the
/// wrong type — yields an empty list. This file is a convenience the
/// application rebuilds by itself; refusing to start over it, or reporting
/// it as a loss the way `vms.json` is reported, would make a scratch file
/// more important than the user's machines (Review Focus 3).
pub fn load(path: &Path) -> Vec<RunRecord> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Record this machine as running, leaving every other entry untouched.
///
/// Read, change one entry, write the whole list back atomically. Two
/// instances of the application share this file, and a blind overwrite
/// would drop the other one's machines (Review Focus 4). The window
/// between the read and the write remains, which is why this is one short
/// function rather than a list cached in memory.
pub fn put(path: &Path, record: RunRecord) -> Result<(), String> {
    let mut records = load(path);
    records.retain(|r| r.vm_id != record.vm_id);
    records.push(record);
    save(path, &records)
}

/// Forget this machine, keeping the rest. A machine that is not there is
/// not an error: both stopping and a recovery sweep end here.
pub fn remove(path: &Path, vm_id: Uuid) -> Result<(), String> {
    let mut records = load(path);
    let before = records.len();
    records.retain(|r| r.vm_id != vm_id);
    if records.len() == before {
        return Ok(());
    }
    save(path, &records)
}

/// The whole list, written the way the configuration is written: into a
/// sibling and then renamed, so an interrupted write cannot leave half a
/// list behind.
fn save(path: &Path, records: &[RunRecord]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(records)
        .map_err(|e| format!("could not write the run state: {}", e))?;
    write_config_file(path, json.as_bytes())
}

/// Seconds since the Unix epoch, for `started_at`.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}
