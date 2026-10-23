// FARO QEMU MANAGER (C) R.Trunk 2026
// Fetching an installation image, and proving it arrived intact.

use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

/// How long any single request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How much is read at a time, both from the network and from disk.
const CHUNK: usize = 64 * 1024;

/// The HTTP calls this module needs, and nothing more.
///
/// A trait rather than `ureq` directly, so the tests can answer from a table:
/// a download worker that can only be exercised against a live mirror is a
/// worker whose failure paths are never tested.
pub trait Http: Send + Sync {
    /// Fetch a small document — a checksum file, a directory listing.
    fn get_text(&self, url: &str) -> Result<String, String>;

    /// Whether this URL is served. Used to skip names a checksum file still
    /// lists but the mirror no longer carries.
    fn head_ok(&self, url: &str) -> bool;

    /// Open a byte stream from `offset`, with the total length when the
    /// mirror states it. An `offset` of 0 asks for the whole thing.
    fn get_from(
        &self,
        url: &str,
        offset: u64,
    ) -> Result<(Box<dyn Read + Send>, Option<u64>), String>;
}

/// The real thing.
pub struct UreqHttp;

impl Http for UreqHttp {
    fn get_text(&self, url: &str) -> Result<String, String> {
        let mut response = agent()
            .get(url)
            .call()
            .map_err(|e| format!("{}: {}", url, e))?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("{}: {}", url, e))
    }

    fn head_ok(&self, url: &str) -> bool {
        agent().head(url).call().is_ok()
    }

    fn get_from(
        &self,
        url: &str,
        offset: u64,
    ) -> Result<(Box<dyn Read + Send>, Option<u64>), String> {
        let mut request = agent().get(url);
        if offset > 0 {
            request = request.header("Range", &format!("bytes={}-", offset));
        }
        let response = request.call().map_err(|e| format!("{}: {}", url, e))?;

        // Content-Length is the length of what is being sent, which after a
        // range request is the remainder. The total is that plus the offset.
        let total = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(|len| len + offset);

        Ok((Box::new(response.into_body().into_reader()), total))
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into()
}

/// The SHA-256 of a file, as lowercase hex.
///
/// Reads in blocks and gives up when `cancel` is set, because an image is
/// several gigabytes and the digest must not be what keeps the application
/// from closing. A cancelled digest is an error, never a value: handing back
/// a partial digest as if it were the file's would be worse than no answer.
pub fn sha256_of(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {}", name, e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];

    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(format!("{}: checksum cancelled", name));
        }
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("{}: {}", name, e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex_of(&hasher.finalize()))
}

/// Lowercase hex, which is how every publisher writes its checksum files.
fn hex_of(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{:02x}", byte);
    }
    out
}
