// FARO QEMU MANAGER (C) R.Trunk 2026
// Downloading an installation image, and proving it arrived intact.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

use faro_qem::download::{
    body_timeouts, human_bytes, part_path, resolve, sha256_of, source_for, sources, start,
    start_resolving, text_timeouts, ChecksumStyle, DownloadEvent, DownloadHandle, Http, IsoSource,
    Resolved, UreqHttp,
};

// --------------------------------------------------------------------------
// Task 1: the digest
// --------------------------------------------------------------------------

#[test]
fn test_sha256_matches_a_known_value() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("x");
    std::fs::write(&path, b"abc").unwrap();
    assert_eq!(
        sha256_of(&path, &AtomicBool::new(false)).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn test_sha256_of_an_empty_file_is_the_empty_digest() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("empty");
    std::fs::write(&path, b"").unwrap();
    assert_eq!(
        sha256_of(&path, &AtomicBool::new(false)).unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn test_sha256_stops_when_asked_to() {
    // A 6 GB image must not hold up the application's exit.
    let dir = tempdir().unwrap();
    let path = dir.path().join("big");
    std::fs::write(&path, vec![7u8; 8 * 1024 * 1024]).unwrap();
    let err = sha256_of(&path, &AtomicBool::new(true))
        .expect_err("a cancelled digest must not pretend to be a digest");
    assert!(!err.is_empty());
}

#[test]
fn test_sha256_of_a_missing_file_names_the_path() {
    let err = sha256_of(Path::new(r"C:\nope\x.iso"), &AtomicBool::new(false)).unwrap_err();
    assert!(err.contains("x.iso"), "got: {}", err);
}

// --------------------------------------------------------------------------
// From the whole-branch review: the transport must not cap the image size
// --------------------------------------------------------------------------

#[test]
fn test_the_body_transfer_runs_under_no_total_time_budget() {
    // This one cost a working download. `timeout_global` in ureq is
    // end-to-end *including reading the response body*, so a global budget
    // on the body GET is a cap on how large an image can ever arrive: 30 s
    // on a 10 MB/s line is 300 MB, and the Ubuntu desktop image is 6 GB.
    // A stalled connection is caught by a per-read timeout instead, which
    // bounds silence rather than the transfer.
    let policy = body_timeouts();
    assert_eq!(
        policy.global, None,
        "a total budget on the body is a limit on the size of image that can ever finish"
    );
    assert!(
        policy.per_read.is_some(),
        "without a per-read timeout a dead connection hangs for ever"
    );
    assert!(
        policy.connect.is_some(),
        "and an unreachable mirror must not hang either"
    );

    // Small documents are the opposite case: a checksum file that takes
    // longer than this is a mirror that is not answering.
    assert!(
        text_timeouts().global.is_some(),
        "a checksum file has a size we can bound"
    );
}

// --------------------------------------------------------------------------
// Task 2: resolving today's filename and its digest
// --------------------------------------------------------------------------

/// An `Http` that answers from a table instead of the network.
///
/// Every failure worth testing — a checksum file that cannot be fetched, a
/// name the mirror no longer carries — is something this can be told to do
/// and a live mirror cannot.
#[derive(Default)]
struct FakeHttp {
    texts: HashMap<String, String>,
    present: HashSet<String>,
    /// Bytes `get_from` serves, when this fake is standing in for a mirror
    /// that carries an image.
    body: Option<Vec<u8>>,
    /// Break the connection after this many bytes of the stream.
    fail_after: Option<usize>,
    /// Pause between blocks, so a cancellation has a chance to be noticed.
    slow: bool,
    /// Answer a range request with the whole body, as some mirrors do. The
    /// fake was strictly more cooperative than every real mirror until this
    /// existed, which is why a whole class of defect was invisible.
    ignores_range: bool,
}

impl FakeHttp {
    fn new() -> Self {
        Self::default()
    }

    /// A mirror that serves exactly these bytes at any URL.
    fn serving(body: &[u8]) -> Self {
        Self {
            body: Some(body.to_vec()),
            ..Self::default()
        }
    }

    fn failing_after(mut self, bytes: usize) -> Self {
        self.fail_after = Some(bytes);
        self
    }

    fn slow(mut self) -> Self {
        self.slow = true;
        self
    }

    fn ignoring_range(mut self) -> Self {
        self.ignores_range = true;
        self
    }

    fn text(mut self, url: &str, body: &str) -> Self {
        self.texts.insert(url.to_string(), body.to_string());
        self.present.insert(url.to_string());
        self
    }

    fn present(mut self, url: &str) -> Self {
        self.present.insert(url.to_string());
        self
    }
}

impl Http for FakeHttp {
    fn get_text(&self, url: &str) -> Result<String, String> {
        self.texts
            .get(url)
            .cloned()
            .ok_or_else(|| format!("{}: nothing there", url))
    }

    fn head_ok(&self, url: &str) -> bool {
        self.present.contains(url)
    }

    fn get_from(
        &self,
        url: &str,
        offset: u64,
    ) -> Result<(Box<dyn std::io::Read + Send>, Option<u64>), String> {
        let Some(body) = self.body.as_ref() else {
            return Err(format!("{}: this fake only answers text", url));
        };
        if self.ignores_range && offset > 0 {
            // What a real mirror does when it does not support ranges: it
            // answers 200 with everything. `UreqHttp` reports that as an
            // error so the caller starts over instead of appending.
            return Err(format!("{}: the mirror ignored the range request", url));
        }
        if offset as usize > body.len() {
            // A range past the end is a 416 on every real mirror.
            return Err(format!("{}: http status: 416", url));
        }
        let from = (offset as usize).min(body.len());
        Ok((
            Box::new(FakeBody {
                data: body[from..].to_vec(),
                pos: 0,
                fail_after: self.fail_after,
                slow: self.slow,
            }),
            Some(body.len() as u64),
        ))
    }
}

/// A byte stream that can be made to break, or to dawdle.
struct FakeBody {
    data: Vec<u8>,
    pos: usize,
    fail_after: Option<usize>,
    slow: bool,
}

impl std::io::Read for FakeBody {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(limit) = self.fail_after {
            if self.pos >= limit {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "the connection dropped",
                ));
            }
        }
        if self.slow {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let left = self.data.len() - self.pos;
        if left == 0 {
            return Ok(0);
        }
        // Small reads, so a break or a cancellation lands mid-transfer rather
        // than after it.
        let take = buf.len().min(left).min(512);
        buf[..take].copy_from_slice(&self.data[self.pos..self.pos + take]);
        self.pos += take;
        Ok(take)
    }
}

/// A `Resolved` whose digest is the real digest of these bytes.
fn resolved_for(body: &[u8]) -> Resolved {
    let dir = tempdir().unwrap();
    let path = dir.path().join("for-digest");
    std::fs::write(&path, body).unwrap();
    Resolved {
        url: "https://example/image.iso".to_string(),
        file_name: "image.iso".to_string(),
        sha256: sha256_of(&path, &AtomicBool::new(false)).unwrap(),
    }
}

/// Every event the worker sent, in order, until the channel closed.
fn drain(handle: DownloadHandle) -> Vec<DownloadEvent> {
    let mut events = Vec::new();
    while let Ok(event) = handle.rx.recv() {
        events.push(event);
    }
    events
}

/// The same source as `debian_like`, but `'static` — `start_resolving` owns
/// its source for the life of the thread.
fn debian_static() -> &'static IsoSource {
    static SOURCE: IsoSource = IsoSource {
        key: "test-debian",
        dir_url: "https://example/",
        checksum_url: "https://example/SHA256SUMS",
        style: ChecksumStyle::TwoSpace,
        name_prefix: "debian-",
        name_suffix: "-amd64-netinst.iso",
        exclude: &["debian-edu-", "debian-mac-"],
    };
    &SOURCE
}

fn debian_like() -> IsoSource {
    IsoSource {
        key: "test-debian",
        dir_url: "https://example/",
        checksum_url: "https://example/SHA256SUMS",
        style: ChecksumStyle::TwoSpace,
        name_prefix: "debian-",
        name_suffix: "-amd64-netinst.iso",
        exclude: &["debian-edu-", "debian-mac-"],
    }
}

fn ubuntu_like() -> IsoSource {
    IsoSource {
        key: "test-ubuntu",
        dir_url: "https://example/",
        checksum_url: "https://example/SHA256SUMS",
        style: ChecksumStyle::Starred,
        name_prefix: "ubuntu-",
        name_suffix: "-desktop-amd64.iso",
        exclude: &[],
    }
}

fn arch_like() -> IsoSource {
    IsoSource {
        key: "test-arch",
        dir_url: "https://example/",
        checksum_url: "https://example/sha256sums.txt",
        style: ChecksumStyle::TwoSpace,
        name_prefix: "archlinux-",
        name_suffix: "-x86_64.iso",
        exclude: &["bootstrap"],
    }
}

fn alpine_like() -> IsoSource {
    IsoSource {
        key: "test-alpine",
        dir_url: "https://example/",
        checksum_url: "https://example/",
        style: ChecksumStyle::PerFile,
        name_prefix: "alpine-standard-",
        name_suffix: "-x86_64.iso",
        exclude: &["_rc"],
    }
}

#[test]
fn test_debian_style_two_spaces() {
    let http = FakeHttp::new()
        .text(
            "https://example/SHA256SUMS",
            "aaaa  debian-13.7.0-amd64-netinst.iso\nbbbb  debian-edu-13.7.0-amd64-netinst.iso\n",
        )
        .present("https://example/debian-13.7.0-amd64-netinst.iso");
    let r = resolve(&debian_like(), &http).unwrap();
    assert_eq!(r.file_name, "debian-13.7.0-amd64-netinst.iso");
    assert_eq!(r.sha256, "aaaa");
    assert!(r.url.ends_with(&r.file_name));
}

#[test]
fn test_the_excluded_variants_are_never_chosen() {
    // debian-edu and debian-mac match the plain pattern and are not what the
    // template means.
    let http = FakeHttp::new()
        .text(
            "https://example/SHA256SUMS",
            "bbbb  debian-edu-13.7.0-amd64-netinst.iso\n",
        )
        .present("https://example/debian-edu-13.7.0-amd64-netinst.iso");
    assert!(
        resolve(&debian_like(), &http).is_err(),
        "no acceptable name must mean an error, not a wrong file"
    );
}

#[test]
fn test_ubuntu_style_strips_the_star_and_takes_the_newest() {
    let http = FakeHttp::new()
        .text(
            "https://example/SHA256SUMS",
            "1111 *ubuntu-24.04.3-desktop-amd64.iso\n2222 *ubuntu-24.04.4-desktop-amd64.iso\n",
        )
        .present("https://example/ubuntu-24.04.4-desktop-amd64.iso");
    let r = resolve(&ubuntu_like(), &http).unwrap();
    assert_eq!(r.file_name, "ubuntu-24.04.4-desktop-amd64.iso");
    assert_eq!(r.sha256, "2222");
}

#[test]
fn test_a_name_in_the_checksum_file_that_is_no_longer_served_is_skipped() {
    // Review Focus 4: Ubuntu lists point releases that have been removed.
    let http = FakeHttp::new()
        .text(
            "https://example/SHA256SUMS",
            "1111 *ubuntu-24.04.3-desktop-amd64.iso\n2222 *ubuntu-24.04.4-desktop-amd64.iso\n",
        )
        .present("https://example/ubuntu-24.04.3-desktop-amd64.iso");
    let r = resolve(&ubuntu_like(), &http).unwrap();
    assert_eq!(r.file_name, "ubuntu-24.04.3-desktop-amd64.iso");
    assert_eq!(r.sha256, "1111");
}

#[test]
fn test_arch_uses_its_stable_name() {
    let http = FakeHttp::new()
        .text(
            "https://example/sha256sums.txt",
            "6666  archlinux-2026.10.01-x86_64.iso\n6666  archlinux-x86_64.iso\n0000  archlinux-bootstrap-x86_64.tar.zst\n",
        )
        .present("https://example/archlinux-x86_64.iso");
    let r = resolve(&arch_like(), &http).unwrap();
    assert_eq!(r.file_name, "archlinux-x86_64.iso");
    assert_eq!(r.sha256, "6666");
}

#[test]
fn test_alpine_reads_a_checksum_file_per_image_and_skips_candidates() {
    let http = FakeHttp::new()
        .text(
            "https://example/",
            "<a href=\"alpine-standard-3.24.0-x86_64.iso\">x</a><a href=\"alpine-standard-3.24.0_rc2-x86_64.iso\">y</a>",
        )
        .text(
            "https://example/alpine-standard-3.24.0-x86_64.iso.sha256",
            "dddd  alpine-standard-3.24.0-x86_64.iso\n",
        )
        .present("https://example/alpine-standard-3.24.0-x86_64.iso");
    let r = resolve(&alpine_like(), &http).unwrap();
    assert_eq!(r.file_name, "alpine-standard-3.24.0-x86_64.iso");
    assert_eq!(r.sha256, "dddd", "a release candidate is not a release");
}

#[test]
fn test_an_unreachable_checksum_file_is_an_error_not_an_unchecked_download() {
    let http = FakeHttp::new(); // answers nothing
    assert!(
        resolve(&debian_like(), &http).is_err(),
        "no checksum must mean no download"
    );
}

#[test]
fn test_all_four_sources_are_registered_with_the_keys_the_templates_use() {
    for key in ["debian", "ubuntu", "arch", "alpine"] {
        assert!(source_for(key).is_some(), "{}", key);
    }
    assert!(
        source_for("virtio-win").is_none(),
        "its publisher gives no SHA-256"
    );
    for t in faro_qem::templates::VmTemplate::all() {
        if let Some(key) = t.iso_source {
            assert!(
                source_for(key).is_some(),
                "template {} names unknown source {}",
                t.key,
                key
            );
        }
    }
}

#[test]
#[ignore = "reaches the real mirrors; run with --ignored"]
fn test_the_real_mirrors_still_answer_in_the_shape_we_expect() {
    for source in sources() {
        let r =
            resolve(source, &UreqHttp::new()).unwrap_or_else(|e| panic!("{}: {}", source.key, e));
        assert_eq!(r.sha256.len(), 64, "{} gave {:?}", source.key, r.sha256);
        assert!(
            r.file_name.ends_with(".iso"),
            "{}: {}",
            source.key,
            r.file_name
        );
        println!("{}: {} ({}…)", source.key, r.file_name, &r.sha256[..16]);
    }
}

// --------------------------------------------------------------------------
// Task 3: the worker — progress, cancellation, verification, rename
// --------------------------------------------------------------------------

#[test]
fn test_a_good_download_lands_under_the_target_name() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("install.iso");
    let body = b"hello world".to_vec();
    let events = drain(start(
        Arc::new(FakeHttp::serving(&body)),
        resolved_for(&body),
        target.clone(),
    ));
    assert!(
        matches!(events.last(), Some(DownloadEvent::Done(_))),
        "got: {:?}",
        events
    );
    assert_eq!(std::fs::read(&target).unwrap(), body);
    assert!(
        !part_path(&target).exists(),
        "the partial file must be gone"
    );
}

#[test]
fn test_progress_and_verifying_are_both_reported() {
    let dir = tempdir().unwrap();
    let body = vec![3u8; 512 * 1024];
    let events = drain(start(
        Arc::new(FakeHttp::serving(&body)),
        resolved_for(&body),
        dir.path().join("x.iso"),
    ));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DownloadEvent::Progress { done, .. } if *done > 0)),
        "a progress bar needs something to show"
    );
    assert!(events.iter().any(|e| matches!(e, DownloadEvent::Verifying)));
}

#[test]
fn test_a_wrong_digest_discards_the_file_and_says_so() {
    // Review Focus 2, and spec section 6: a file that does not match is
    // discarded, not mounted.
    let dir = tempdir().unwrap();
    let target = dir.path().join("install.iso");
    let mut resolved = resolved_for(b"actual bytes");
    resolved.sha256 = "0".repeat(64);
    let events = drain(start(
        Arc::new(FakeHttp::serving(b"actual bytes")),
        resolved,
        target.clone(),
    ));
    match events.last() {
        Some(DownloadEvent::Failed(msg)) => {
            let m = msg.to_lowercase();
            assert!(
                m.contains("sha256") || m.contains("checksum"),
                "got: {}",
                msg
            );
        }
        other => panic!("got: {:?}", other),
    }
    assert!(
        !target.exists(),
        "an unverified file must never carry the target name"
    );
    assert!(
        !part_path(&target).exists(),
        "and must not be left as a resumable partial"
    );
}

#[test]
fn test_a_broken_connection_leaves_a_partial_that_a_second_run_finishes() {
    // Review Focus 1.
    let dir = tempdir().unwrap();
    let target = dir.path().join("install.iso");
    let body: Vec<u8> = (0..4096u32).map(|n| n as u8).collect();

    let events = drain(start(
        Arc::new(FakeHttp::serving(&body).failing_after(1024)),
        resolved_for(&body),
        target.clone(),
    ));
    assert!(matches!(events.last(), Some(DownloadEvent::Failed(_))));
    assert!(
        part_path(&target).exists(),
        "an interrupted transfer is worth resuming"
    );
    assert!(!target.exists());

    let events = drain(start(
        Arc::new(FakeHttp::serving(&body)),
        resolved_for(&body),
        target.clone(),
    ));
    assert!(
        matches!(events.last(), Some(DownloadEvent::Done(_))),
        "got: {:?}",
        events
    );
    assert_eq!(
        std::fs::read(&target).unwrap(),
        body,
        "the resumed file must be the whole file"
    );
}

#[test]
fn test_cancelling_stops_the_work_and_leaves_no_target() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("install.iso");
    let body = vec![1u8; 4 * 1024 * 1024];
    let handle = start(
        Arc::new(FakeHttp::serving(&body).slow()),
        resolved_for(&body),
        target.clone(),
    );
    handle.cancel.store(true, Ordering::SeqCst);
    let _ = drain(handle);
    assert!(!target.exists());
}

#[test]
fn test_a_partial_longer_than_the_remote_file_is_started_over() {
    // From the review: Arch re-stocks `archlinux-x86_64.iso` monthly under
    // the same name, so a `.part` from last month sits where this month's
    // download wants to resume. If last month's image was larger, a range
    // request past the end answers 416 for ever and the machine can never
    // be started again. A partial longer than the file cannot be a prefix
    // of it, so it is discarded and the transfer starts over.
    let dir = tempdir().unwrap();
    let target = dir.path().join("archlinux-x86_64.iso");
    let body = vec![9u8; 2048];
    std::fs::write(part_path(&target), vec![1u8; 8192]).unwrap();

    let events = drain(start(
        Arc::new(FakeHttp::serving(&body)),
        resolved_for(&body),
        target.clone(),
    ));

    assert!(
        matches!(events.last(), Some(DownloadEvent::Done(_))),
        "a stale oversized partial must not be a dead end; got: {:?}",
        events
    );
    assert_eq!(
        std::fs::read(&target).unwrap(),
        body,
        "and the result is this month's image, whole"
    );
}

#[test]
fn test_a_mirror_that_ignores_the_range_header_starts_over_rather_than_appending() {
    // Appending a whole body to a partial would duplicate the prefix. The
    // digest would catch it, but only after the user has paid for the
    // transfer twice.
    let dir = tempdir().unwrap();
    let target = dir.path().join("install.iso");
    let body: Vec<u8> = (0..4096u32).map(|n| n as u8).collect();
    std::fs::write(part_path(&target), &body[..1024]).unwrap();

    let events = drain(start(
        Arc::new(FakeHttp::serving(&body).ignoring_range()),
        resolved_for(&body),
        target.clone(),
    ));

    assert!(
        matches!(events.last(), Some(DownloadEvent::Done(_))),
        "got: {:?}",
        events
    );
    assert_eq!(
        std::fs::read(&target).unwrap(),
        body,
        "the file is the file, not the file with a repeated prefix"
    );
}

#[test]
fn test_an_unwritable_target_fails_without_leaving_a_partial() {
    // Review Focus 5.
    let dir = tempdir().unwrap();
    let blocker = dir.path().join("dir-is-a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let target = blocker.join("install.iso");
    let events = drain(start(
        Arc::new(FakeHttp::serving(b"x")),
        resolved_for(b"x"),
        target.clone(),
    ));
    assert!(
        matches!(events.last(), Some(DownloadEvent::Failed(_))),
        "got: {:?}",
        events
    );
    assert!(!part_path(&target).exists());
}

// --------------------------------------------------------------------------
// From the review: start_resolving is what the application calls, and it
// had no test at all. `start` was well covered and is production-dead.
// --------------------------------------------------------------------------

/// A fake that both resolves and serves, which `start_resolving` needs.
fn resolving_fake(body: &[u8]) -> FakeHttp {
    FakeHttp {
        body: Some(body.to_vec()),
        ..FakeHttp::new()
            .text("https://example/SHA256SUMS", "")
            .present("https://example/debian-13.7.0-amd64-netinst.iso")
    }
}

#[test]
fn test_start_resolving_names_the_target_before_the_first_byte() {
    // The ordering the whole design rests on: the machine is created with a
    // placeholder, and the real name has to arrive before the bytes do, or
    // the start check cannot see the .part beside it.
    let dir = tempdir().unwrap();
    let body = b"an image".to_vec();
    let digest = {
        let p = dir.path().join("d");
        std::fs::write(&p, &body).unwrap();
        sha256_of(&p, &AtomicBool::new(false)).unwrap()
    };
    let http = resolving_fake(&body).text(
        "https://example/SHA256SUMS",
        &format!(
            "{}  debian-13.7.0-amd64-netinst.iso
",
            digest
        ),
    );

    let events = drain(start_resolving(
        Arc::new(http),
        debian_static(),
        dir.path().to_path_buf(),
    ));

    let first = events.first().expect("something was reported");
    match first {
        DownloadEvent::Target(path) => {
            assert_eq!(
                path.file_name().unwrap(),
                "debian-13.7.0-amd64-netinst.iso",
                "the name comes from the publisher, not from us"
            );
        }
        other => panic!("Target must come first, got: {:?}", other),
    }
    assert!(
        matches!(events.last(), Some(DownloadEvent::Done(_))),
        "got: {:?}",
        events
    );
}

#[test]
fn test_start_resolving_reports_a_mirror_it_cannot_reach() {
    // The case that leaves a machine with no image and nothing running: it
    // has to say so, or the machine looks ready and installs nothing.
    let dir = tempdir().unwrap();
    let events = drain(start_resolving(
        Arc::new(FakeHttp::new()),
        debian_static(),
        dir.path().to_path_buf(),
    ));
    assert!(
        matches!(events.first(), Some(DownloadEvent::Failed(_))),
        "got: {:?}",
        events
    );
    assert!(
        !events.iter().any(|e| matches!(e, DownloadEvent::Target(_))),
        "nothing was resolved, so nothing may be named"
    );
}

#[test]
fn test_human_bytes_reads_like_a_size() {
    assert_eq!(faro_qem::download::human_bytes(0), "0 B");
    assert_eq!(faro_qem::download::human_bytes(512), "512 B");
    assert_eq!(faro_qem::download::human_bytes(1024), "1.0 KB");
    assert_eq!(faro_qem::download::human_bytes(1536), "1.5 KB");
    assert_eq!(faro_qem::download::human_bytes(792_723_456), "756.0 MB");
    assert_eq!(faro_qem::download::human_bytes(6_000_000_000), "5.6 GB");
}

#[test]
fn test_cancelling_keeps_the_partial_for_a_later_attempt() {
    // Ruling from task 3, now pinned: whoever cancels usually does not want
    // to start over, and the detail panel offers to pick it up.
    let dir = tempdir().unwrap();
    let target = dir.path().join("install.iso");
    let body = vec![1u8; 4 * 1024 * 1024];
    let handle = start(
        Arc::new(FakeHttp::serving(&body).slow()),
        resolved_for(&body),
        target.clone(),
    );
    std::thread::sleep(std::time::Duration::from_millis(80));
    handle.cancel.store(true, Ordering::SeqCst);
    let _ = drain(handle);
    assert!(
        !target.exists(),
        "nothing unverified carries the target name"
    );
    assert!(
        part_path(&target).exists(),
        "and the bytes so far are worth keeping"
    );
}

#[test]
#[ignore = "reaches a real mirror and deliberately takes over a minute; run with --ignored"]
fn test_a_real_transfer_outlives_the_old_global_budget() {
    // The proof for the defect this suite exists to prevent coming back.
    // Reading deliberately slowly, this takes well past the 30 s that used
    // to be a hard ceiling on the whole operation. If a global budget is
    // ever put back on the body agent, this fails.
    use std::io::Read;
    let source = source_for("debian").expect("debian is registered");
    let resolved = resolve(source, &UreqHttp::new()).expect("resolved");

    let (mut stream, total) = UreqHttp::new()
        .get_from(&resolved.url, 0)
        .expect("the mirror answered");
    assert!(total.unwrap_or(0) > 100_000_000, "a real image, not a stub");

    let started = std::time::Instant::now();
    let mut read_total = 0u64;
    let mut buffer = vec![0u8; 16 * 1024];
    // 40 seconds of paced reading: past the old ceiling, and far short of
    // the whole image.
    while started.elapsed() < std::time::Duration::from_secs(40) {
        let n = stream.read(&mut buffer).expect("the transfer keeps going");
        if n == 0 {
            break;
        }
        read_total += n as u64;
        std::thread::sleep(std::time::Duration::from_millis(40));
    }

    assert!(
        started.elapsed() >= std::time::Duration::from_secs(40),
        "the read stopped early -- a total budget is back on the body agent"
    );
    println!(
        "read {} in {:?} without a timeout",
        human_bytes(read_total),
        started.elapsed()
    );
}
