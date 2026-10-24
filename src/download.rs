// FARO QEMU MANAGER (C) R.Trunk 2026
// Fetching an installation image, and proving it arrived intact.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// How long fetching a small document may take, end to end.
///
/// A checksum file is tens of kilobytes. One that takes longer than this is
/// a mirror that is not answering, and a total budget is exactly the right
/// instrument for it.
const TEXT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long waiting for a connection may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the transfer may go silent before it counts as dead.
///
/// This is what bounds a stalled download — not its total duration. Several
/// minutes rather than seconds, because a mirror under load can pause.
const STALL_TIMEOUT: Duration = Duration::from_secs(120);

/// The timeouts a transfer runs under.
///
/// Public because the suite holds a line here that cost a working download
/// once: `timeout_global` in ureq is end-to-end *including reading the
/// response body*, so a total budget on an image transfer is a cap on how
/// large an image can ever arrive. 30 seconds on a 10 MB/s line is 300 MB,
/// and the Ubuntu desktop image is six gigabytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// Total budget for the whole operation, body included.
    pub global: Option<Duration>,
    /// Budget for establishing the connection.
    pub connect: Option<Duration>,
    /// How long a single read may take — silence, not size.
    pub per_read: Option<Duration>,
}

/// What a small document is fetched under: a total budget is right.
pub fn text_timeouts() -> Timeouts {
    Timeouts {
        global: Some(TEXT_TIMEOUT),
        connect: Some(CONNECT_TIMEOUT),
        per_read: Some(STALL_TIMEOUT),
    }
}

/// What an image transfer is fetched under: no total budget.
pub fn body_timeouts() -> Timeouts {
    Timeouts {
        global: None,
        connect: Some(CONNECT_TIMEOUT),
        per_read: Some(STALL_TIMEOUT),
    }
}

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
///
/// Two agents, because the two kinds of request need opposite treatment: a
/// checksum file gets a total budget, an image must not have one. Built once
/// and kept, so the connection pool survives — resolving an image means one
/// GET plus a HEAD per candidate, and a fresh agent each time would mean a
/// fresh TLS handshake each time.
pub struct UreqHttp {
    text: ureq::Agent,
    body: ureq::Agent,
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl UreqHttp {
    pub fn new() -> Self {
        Self {
            text: agent_for(text_timeouts(), true),
            body: agent_for(body_timeouts(), false),
        }
    }
}

impl Http for UreqHttp {
    fn get_text(&self, url: &str) -> Result<String, String> {
        let mut response = self
            .text
            .get(url)
            .call()
            .map_err(|e| format!("{}: {}", url, e))?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("{}: {}", url, e))
    }

    fn head_ok(&self, url: &str) -> bool {
        self.text.head(url).call().is_ok()
    }

    fn get_from(
        &self,
        url: &str,
        offset: u64,
    ) -> Result<(Box<dyn Read + Send>, Option<u64>), String> {
        let mut request = self.body.get(url);
        if offset > 0 {
            request = request.header("Range", &format!("bytes={}-", offset));
        }
        let response = request.call().map_err(|e| format!("{}: {}", url, e))?;

        // A mirror that ignores the Range header answers 200 with the whole
        // body. Appending that to a partial file would duplicate the prefix,
        // so the caller has to know: 206 means the offset was honoured.
        let ranged = response.status().as_u16() == 206;

        // Content-Length is the length of what is being sent, which after an
        // honoured range request is the remainder. The total is that plus the
        // offset; when the range was ignored it is the whole thing already.
        let base = if ranged { offset } else { 0 };
        let total = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(|len| len + base);

        if offset > 0 && !ranged {
            return Err(format!(
                "{}: the mirror ignored the range request (status {}); starting over",
                url,
                response.status().as_u16()
            ));
        }

        Ok((Box::new(response.into_body().into_reader()), total))
    }
}

fn agent_for(timeouts: Timeouts, compress: bool) -> ureq::Agent {
    let mut config = ureq::Agent::config_builder()
        .timeout_global(timeouts.global)
        .timeout_connect(timeouts.connect)
        .timeout_per_read(timeouts.per_read);
    if !compress {
        // An image is already compressed, and a mirror that content-encoded
        // it would make a range request incoherent: a byte offset into a
        // gzip stream cannot be resumed.
        config = config.accept_encoding(ureq::config::AutoHeaderValue::None);
    }
    config.build().into()
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
        // Nothing to exclude: the suffix already rejects the bootstrap
        // tarball, which is the only other thing in that directory.
        exclude: &[],
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

/// The page a user fetches this image from themselves.
///
/// Three images are not downloaded, for two different reasons. Microsoft
/// hands out session-bound links from a form page, so automating it would be
/// fragile and outside the intended use (spec section 6). virtio-win is here
/// because its publisher gives no SHA-256 for the ISO at all — only MD5, and
/// only for RPM packages — and an unverified driver ISO mounted into a
/// Windows guest is exactly the anti-goal in section 1: something that looks
/// checked and is not.
///
/// These addresses cannot be verified from here: Microsoft answers an
/// ordinary HTTP client with 403 regardless of whether the page exists. They
/// are the documented official ones, and the application only ever hands them
/// to the browser.
pub fn official_page(key: &str) -> Option<&'static str> {
    match key {
        "windows-11" => Some("https://www.microsoft.com/software-download/windows11"),
        "windows-10" => Some("https://www.microsoft.com/software-download/windows10ISO"),
        "virtio-win" => {
            Some("https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/stable-virtio/")
        }
        _ => None,
    }
}

/// Hand a URL to whatever the system uses for the web.
///
/// Not a download: the user fetches the file and the application never sees
/// the transfer, which is the whole point for the three images above.
pub fn open_in_browser(url: &str) -> Result<(), String> {
    let result = if cfg!(target_os = "windows") {
        // The empty "" is the window title start insists on; without it a
        // URL containing spaces becomes the title and nothing opens.
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    result.map(|_| ()).map_err(|e| format!("{}: {}", url, e))
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
                // A candidate whose digest file does not answer is a
                // candidate to skip, not the end of the search -- trying the
                // next one is what this loop is for.
                let Ok(text) = http.get_text(&per_file) else {
                    continue;
                };
                // The line must be about this file. A digest file that names
                // something else is not this image's digest.
                let found = checksum_lines(&text, ChecksumStyle::TwoSpace)
                    .find(|(_, listed)| *listed == name.as_str() || text.lines().count() == 1)
                    .map(|(hash, _)| hash.to_string());
                match found {
                    Some(hash) => hash,
                    None => continue,
                }
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

/// What the worker reports while it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadEvent {
    /// Today's filename is known, and this is where it will land. Sent
    /// before the first byte, so the machine's ISO slot can point at the
    /// real name and the start check can see the `.part` beside it.
    Target(PathBuf),
    /// `total` is `None` when the mirror did not state a length.
    Progress { done: u64, total: Option<u64> },
    /// The bytes are all here; the digest is being computed. Worth saying,
    /// because on a 6 GB image it takes long enough that a progress bar
    /// sitting at 100% would look stuck.
    Verifying,
    /// Verified and moved to its final name.
    Done(PathBuf),
    /// Nothing was installed. The message says what went wrong.
    Failed(String),
}

/// A running download: the flag that stops it, and the channel it reports on.
pub struct DownloadHandle {
    pub cancel: Arc<AtomicBool>,
    pub rx: Receiver<DownloadEvent>,
}

/// Where the partial file lives while a download is in progress.
///
/// Appending a suffix rather than using a directory keeps it beside the
/// target, on the same volume, so the final step is a rename and not a copy.
/// It is also the whole of the "is this download finished?" bookkeeping: a
/// file under the target name is verified, a `.part` beside it is not, and
/// that remains true after the application is restarted and the thread that
/// was writing it is long gone.
pub fn part_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    target.with_file_name(name)
}

/// How often progress is reported, at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// And how many bytes, at most, between reports.
const PROGRESS_BYTES: u64 = 4 * 1024 * 1024;

/// Fetch `resolved` to `target`, in a thread of its own.
///
/// The order is what makes this safe: bytes go to `<target>.part`, the digest
/// is computed there, and only a file whose digest matches is renamed to
/// `target`. A file under the target name has therefore always been checked —
/// spec section 6's hard requirement — and no stored "download in progress"
/// flag is needed, which is good, because such a flag would lie after a
/// restart.
pub fn start(http: Arc<dyn Http>, resolved: Resolved, target: PathBuf) -> DownloadHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let worker_cancel = Arc::clone(&cancel);

    std::thread::spawn(move || {
        let part = part_path(&target);
        let outcome = fetch_and_verify(&*http, &resolved, &target, &part, &worker_cancel, &tx);
        let event = match outcome {
            Ok(()) => DownloadEvent::Done(target.clone()),
            Err(problem) => {
                // A digest that does not match means the bytes are wrong, so
                // the partial is worthless and must not look resumable. A
                // transfer that merely broke off leaves its partial for the
                // next attempt.
                if problem.discard_partial {
                    let _ = std::fs::remove_file(&part);
                }
                DownloadEvent::Failed(problem.message)
            }
        };
        let _ = tx.send(event);
    });

    DownloadHandle { cancel, rx }
}

/// Resolve today's filename and then fetch it, all in one thread.
///
/// `resolve` talks to a mirror, which is why it cannot happen while a frame
/// is being drawn (spec section 6: the interface must not freeze). The
/// machine therefore exists before its filename is known, and the worker
/// sends [`DownloadEvent::Target`] as soon as it is.
pub fn start_resolving(
    http: Arc<dyn Http>,
    source: &'static IsoSource,
    folder: PathBuf,
) -> DownloadHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let worker_cancel = Arc::clone(&cancel);

    std::thread::spawn(move || {
        let resolved = match resolve(source, &*http) {
            Ok(resolved) => resolved,
            Err(problem) => {
                let _ = tx.send(DownloadEvent::Failed(problem));
                return;
            }
        };
        let target = folder.join(&resolved.file_name);
        let _ = tx.send(DownloadEvent::Target(target.clone()));

        let part = part_path(&target);
        let outcome = fetch_and_verify(&*http, &resolved, &target, &part, &worker_cancel, &tx);
        let event = match outcome {
            Ok(()) => DownloadEvent::Done(target),
            Err(problem) => {
                if problem.discard_partial {
                    let _ = std::fs::remove_file(&part);
                }
                DownloadEvent::Failed(problem.message)
            }
        };
        let _ = tx.send(event);
    });

    DownloadHandle { cancel, rx }
}

/// A byte count a person can read.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

/// A failure, and whether the partial file is worth keeping.
struct Problem {
    message: String,
    discard_partial: bool,
}

impl Problem {
    /// The transfer broke off. The bytes so far are good; keep them.
    fn resumable(message: String) -> Self {
        Self {
            message,
            discard_partial: false,
        }
    }

    /// The bytes are wrong, or there are none. Nothing to resume from.
    fn fatal(message: String) -> Self {
        Self {
            message,
            discard_partial: true,
        }
    }
}

fn fetch_and_verify(
    http: &dyn Http,
    resolved: &Resolved,
    target: &Path,
    part: &Path,
    cancel: &AtomicBool,
    tx: &Sender<DownloadEvent>,
) -> Result<(), Problem> {
    // Resume from whatever is already there. A `.part` is only ever written
    // by this function, so its length is a byte count we can trust — but not
    // a byte count that necessarily belongs to *this* image. Arch re-stocks
    // `archlinux-x86_64.iso` under the same name every month, so last
    // month's partial sits exactly where this month's resume would start.
    let mut done = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);

    let (mut stream, total) = match http.get_from(&resolved.url, done) {
        Ok(answer) => answer,
        // A resume that the mirror refuses — 416 because the partial is
        // longer than the file now is, or a mirror that ignores ranges
        // altogether — must not be a dead end. Nothing can be salvaged from
        // those bytes, so they go and the transfer starts over. Without
        // this, a stale partial makes the machine unstartable for ever.
        Err(problem) if done > 0 => {
            let _ = std::fs::remove_file(part);
            done = 0;
            http.get_from(&resolved.url, 0)
                .map_err(|e| Problem::resumable(format!("{} (after {})", e, problem)))?
        }
        Err(problem) => return Err(Problem::resumable(problem)),
    };

    // The same check from the other side: a partial longer than the file
    // cannot be a prefix of it, whatever the mirror said.
    if let Some(total) = total {
        if done > total {
            let _ = std::fs::remove_file(part);
            done = 0;
            let (fresh, _) = http
                .get_from(&resolved.url, 0)
                .map_err(Problem::resumable)?;
            stream = fresh;
        }
    }

    let had_partial = done > 0;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(part)
        .map_err(|e| {
            let message = format!("{}: {}", part.display(), e);
            // A file that exists and merely cannot be opened — locked by a
            // scanner, marked read-only — holds bytes whose contents are
            // not in question. Discarding gigabytes for a reason unrelated
            // to them would be wrong. With nothing there, there is nothing
            // to keep.
            if had_partial {
                Problem::resumable(message)
            } else {
                Problem::fatal(message)
            }
        })?;

    let mut buffer = vec![0u8; CHUNK];
    let mut last_report = Instant::now();
    let mut reported = done;
    let _ = tx.send(DownloadEvent::Progress { done, total });

    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(Problem::resumable("cancelled".to_string()));
        }
        let read = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return Err(Problem::resumable(format!("{}: {}", resolved.url, e))),
        };
        file.write_all(&buffer[..read])
            .map_err(|e| Problem::fatal(format!("{}: {}", part.display(), e)))?;
        done += read as u64;

        if last_report.elapsed() >= PROGRESS_EVERY || done - reported >= PROGRESS_BYTES {
            let _ = tx.send(DownloadEvent::Progress { done, total });
            last_report = Instant::now();
            reported = done;
        }
    }

    file.flush()
        .map_err(|e| Problem::fatal(format!("{}: {}", part.display(), e)))?;
    drop(file);
    let _ = tx.send(DownloadEvent::Progress { done, total });

    let _ = tx.send(DownloadEvent::Verifying);
    let actual = sha256_of(part, cancel).map_err(Problem::resumable)?;
    if !actual.eq_ignore_ascii_case(&resolved.sha256) {
        return Err(Problem::fatal(format!(
            "{}: sha256 does not match. Expected {}, got {}. The file was discarded.",
            resolved.file_name, resolved.sha256, actual
        )));
    }

    std::fs::rename(part, target)
        .map_err(|e| Problem::fatal(format!("{}: {}", target.display(), e)))?;
    Ok(())
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
