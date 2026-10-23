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

/// How a publisher writes its checksum file.
///
/// All three were read off the real mirrors rather than assumed; the
/// differences are small and every one of them breaks a parser that expects
/// the other shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumStyle {
    /// `<hash>` + two spaces + name. Debian and Arch.
    TwoSpace,
    /// `<hash>` + space + `*` + name. Ubuntu marks binary mode with a star.
    Starred,
    /// One `<name>.sha256` file per image, holding a single `TwoSpace` line.
    /// Alpine. `checksum_url` is then the directory to list candidates from.
    PerFile,
}

/// Where one publisher's images live, and how to read its checksums.
///
/// The filename is never stored. Mirrors keep the directories stable and move
/// the names, so the name is read out of the checksum file each time — which
/// also means nobody has to maintain version numbers in this source code.
pub struct IsoSource {
    pub key: &'static str,
    /// Directory the image itself is served from, with a trailing slash.
    pub dir_url: &'static str,
    /// The checksum file, or for `PerFile` the directory to list.
    pub checksum_url: &'static str,
    pub style: ChecksumStyle,
    pub name_prefix: &'static str,
    pub name_suffix: &'static str,
    /// Fragments that disqualify a name even though it matches the pattern:
    /// Debian's edu and mac variants, Alpine's release candidates.
    pub exclude: &'static [&'static str],
}

/// The four publishers whose mirrors answer in a shape this can read.
///
/// virtio-win is deliberately absent. Spec section 6 names it as a download
/// and in the same breath requires every download to be checked against its
/// SHA-256; its publisher's CHECKSUM file holds four MD5 lines, all for RPM
/// packages, and nothing at all for the ISO. The hard requirement wins, so
/// virtio-win is handled like the Windows images: the page opens, the user
/// fetches the file.
static SOURCES: &[IsoSource] = &[
    IsoSource {
        key: "debian",
        dir_url: "https://cdimage.debian.org/debian-cd/current/amd64/iso-cd/",
        checksum_url: "https://cdimage.debian.org/debian-cd/current/amd64/iso-cd/SHA256SUMS",
        style: ChecksumStyle::TwoSpace,
        name_prefix: "debian-",
        name_suffix: "-amd64-netinst.iso",
        exclude: &["debian-edu-", "debian-mac-"],
    },
    IsoSource {
        key: "ubuntu",
        dir_url: "https://releases.ubuntu.com/24.04/",
        checksum_url: "https://releases.ubuntu.com/24.04/SHA256SUMS",
        style: ChecksumStyle::Starred,
        name_prefix: "ubuntu-",
        name_suffix: "-desktop-amd64.iso",
        exclude: &[],
    },
    IsoSource {
        key: "arch",
        dir_url: "https://geo.mirror.pkgbuild.com/iso/latest/",
        checksum_url: "https://geo.mirror.pkgbuild.com/iso/latest/sha256sums.txt",
        style: ChecksumStyle::TwoSpace,
        name_prefix: "archlinux-",
        name_suffix: "-x86_64.iso",
        exclude: &["bootstrap"],
    },
    IsoSource {
        key: "alpine",
        dir_url: "https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64/",
        checksum_url: "https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64/",
        style: ChecksumStyle::PerFile,
        name_prefix: "alpine-standard-",
        name_suffix: "-x86_64.iso",
        exclude: &["_rc"],
    },
];

/// Every source a download can be started from.
pub fn sources() -> &'static [IsoSource] {
    SOURCES
}

/// The source a template's `iso_source` names, if there is one.
pub fn source_for(key: &str) -> Option<&'static IsoSource> {
    SOURCES.iter().find(|s| s.key == key)
}

/// What a resolution produced: where to fetch, what to call it, what it
/// must hash to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub url: String,
    pub file_name: String,
    pub sha256: String,
}

/// Work out today's filename and its digest from the publisher's own files.
///
/// Candidates come from the checksum file (or, for `PerFile`, the directory
/// listing), are filtered to the pattern, sorted newest-first by name, and
/// tried in that order until one is actually served. Ubuntu's checksum file
/// lists point releases that have already been removed from the directory, so
/// "the first name that matches" is not enough — it has to be the first name
/// that matches **and** answers.
///
/// No digest means no download. A checksum file that cannot be fetched is an
/// error, never a reason to fetch the image unchecked.
pub fn resolve(source: &IsoSource, http: &dyn Http) -> Result<Resolved, String> {
    let listing = http.get_text(source.checksum_url)?;

    let mut candidates: Vec<String> = match source.style {
        ChecksumStyle::PerFile => names_in_listing(&listing),
        _ => checksum_lines(&listing, source.style)
            .map(|(_, name)| name.to_string())
            .collect(),
    };
    candidates.retain(|name| acceptable(name, source));
    // Newest first: these names carry their version, so a descending sort is
    // a descending version order for every one of the four publishers.
    candidates.sort_by(|a, b| b.cmp(a));
    candidates.dedup();

    if candidates.is_empty() {
        return Err(format!(
            "{}: no image matching {}*{} in {}",
            source.key, source.name_prefix, source.name_suffix, source.checksum_url
        ));
    }

    for name in &candidates {
        let url = format!("{}{}", source.dir_url, name);
        if !http.head_ok(&url) {
            continue;
        }
        let sha256 = match source.style {
            ChecksumStyle::PerFile => {
                let per_file = format!("{}{}.sha256", source.dir_url, name);
                let text = http.get_text(&per_file)?;
                let first = checksum_lines(&text, ChecksumStyle::TwoSpace)
                    .next()
                    .map(|(hash, _)| hash.to_string());
                first.ok_or_else(|| format!("{}: no digest in {}", source.key, per_file))?
            }
            style => checksum_lines(&listing, style)
                .find(|(_, candidate)| *candidate == name.as_str())
                .map(|(hash, _)| hash.to_string())
                .ok_or_else(|| format!("{}: no digest for {}", source.key, name))?,
        };
        return Ok(Resolved {
            url,
            file_name: name.clone(),
            sha256,
        });
    }

    Err(format!(
        "{}: the checksum file lists {} image(s), none of which {} still serves",
        source.key,
        candidates.len(),
        source.dir_url
    ))
}

/// The `(hash, name)` pairs in a checksum file.
fn checksum_lines(text: &str, style: ChecksumStyle) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(move |line| {
        let (hash, rest) = line.trim().split_once(char::is_whitespace)?;
        let name = rest.trim_start();
        // Ubuntu writes a star before the name to mark binary mode. Debian
        // and Arch write a second space, which `trim_start` already ate.
        let name = match style {
            ChecksumStyle::Starred => name.strip_prefix('*').unwrap_or(name),
            _ => name,
        };
        (!hash.is_empty() && !name.is_empty()).then_some((hash, name))
    })
}

/// Filenames mentioned in a directory listing.
///
/// Alpine's directory index is HTML, and the names appear in `href`
/// attributes. Anything that is not a plausible filename is dropped by the
/// pattern filter afterwards, so this can afford to be generous.
fn names_in_listing(html: &str) -> Vec<String> {
    let mut names = Vec::new();
    for piece in html.split("href=") {
        let value = piece.trim_start().trim_start_matches(['"', '\'']);
        if let Some(end) = value.find(['"', '\'', '>', ' ']) {
            let name = &value[..end];
            if !name.is_empty() && !name.contains('/') {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// Whether this name is the image the source means.
fn acceptable(name: &str, source: &IsoSource) -> bool {
    name.starts_with(source.name_prefix)
        && name.ends_with(source.name_suffix)
        && !source.exclude.iter().any(|bad| name.contains(bad))
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
