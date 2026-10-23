# Teil 3b — Download: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wer noch keine ISO hat, klickt im Wizard auf „Herunterladen", die VM entsteht sofort, und der Start wartet, bis die Datei da **und gegen die Prüfsumme des Herausgebers geprüft** ist.

**Architecture:** `src/download.rs` trennt drei Dinge, die sonst verfilzen: ein `Http`-Merkmal, hinter dem die echte Bibliothek genauso steckt wie die Testantwort; eine Auflösung, die aus einem Spiegelverzeichnis und seiner Prüfsummendatei den heutigen Dateinamen samt Summe herausliest, damit niemand Versionsnummern pflegen muss; und ein Arbeitsvorgang in einem eigenen Faden, der über einen Kanal berichtet und nach `<ziel>.part` schreibt. Erst eine geprüfte Datei wird auf den Zielnamen umbenannt — damit braucht es kein gespeichertes „läuft noch"-Feld, das nach einem App-Neustart lügt.

**Tech Stack:** Rust 2021, `eframe`/`egui` 0.29.1, **neu: `ureq` 3.5 (blockierend, rustls, keine async-Laufzeit) und `sha2` 0.11** — beides durch Spec §11.8 ausdrücklich erlaubt.

**Spec:** `docs/superpowers/specs/2026-10-23-wizard-und-haertung-design.md`, §6, mit §11.5 und §11.8

## Global Constraints

- **Die Oberfläche darf nicht einfrieren** (Spec §6, harte Vorgabe). Kein Netzzugriff und keine Prüfsummenrechnung im Bild-Thread.
- **Jeder Download wird geprüft** (Spec §6, harte Vorgabe). Eine Datei, die nicht passt, wird **verworfen, nicht eingehängt**.
- **Die Grenze muss in der Oberfläche stehen** (Spec §6): eine Prüfsumme vom selben Spiegel wie das Abbild belegt Unverfälschtheit der Übertragung, **nicht Echtheit der Herkunft**. Signaturprüfung ist Nicht-Ziel (§1).
- **Keine Windows-ISOs** (Spec §6): Microsoft vergibt sitzungsgebundene Links. Die App öffnet die offizielle Seite, die Datei holt der Benutzer.
- Neue Benutzertexte in alle drei Kataloge `assets/locales/{en,de,fr}.json`.
- Code und Kommentare auf Englisch. `cargo fmt --check` und `cargo clippy --all-targets` ohne Beanstandung. Die Tests aus Teilen 1–3a bleiben grün.

## Am 2026-10-23 abgefragte Tatsachen

Abgefragt, nicht nachgelesen. Jede Adresse hat geantwortet.

| Quelle | Prüfsummendatei | Format | Heutiger Dateiname |
|---|---|---|---|
| Debian | `https://cdimage.debian.org/debian-cd/current/amd64/iso-cd/SHA256SUMS` | `<hash>` + zwei Leerzeichen + Name | `debian-13.7.0-amd64-netinst.iso` |
| Ubuntu | `https://releases.ubuntu.com/24.04/SHA256SUMS` | `<hash>` + Leerzeichen + `*` + Name — **Stern vor dem Namen** | `ubuntu-24.04.4-desktop-amd64.iso` |
| Arch | `https://geo.mirror.pkgbuild.com/iso/latest/sha256sums.txt` | `<hash>` + zwei Leerzeichen + Name | **`archlinux-x86_64.iso` ist ein fester Name** mit derselben Summe wie die datierte Datei |
| Alpine | `…/x86_64/<name>.iso.sha256` — **eine Datei je Abbild** | `<hash>` + zwei Leerzeichen + Name | `alpine-standard-3.24.0-x86_64.iso` |

- Alle vier Spiegel antworten auf eine Bereichsanfrage mit `206 Partial Content`. Fortsetzbarkeit ist damit möglich.
- Die Verzeichnisse `debian-cd/current/`, `iso/latest/` und `alpine/latest-stable/releases/x86_64/` sind stabil; nur die Dateinamen wandern. Darum **liest die App den Namen aus der Prüfsummendatei** statt Versionsnummern zu pflegen.
- Ubuntus `SHA256SUMS` führt mehrere Punktversionen (24.04.3 **und** 24.04.4); nicht jede davon liegt noch im Verzeichnis. Der Auflöser muss absteigend sortieren und den ersten Namen nehmen, dessen Adresse antwortet.
- **virtio-win hat keine SHA-256-Summe.** `…/direct-downloads/latest-virtio/CHECKSUM` enthält vier Zeilen, alle **MD5 und alle für RPM-Pakete**; für `virtio-win.iso` steht dort überhaupt nichts. Siehe die Festlegung unten.

## Festlegung: virtio-win wird nicht heruntergeladen, sondern aufgerufen

Spec §6 nennt die virtio-win-Treiber-ISO als Download **und** verlangt im selben Abschnitt, dass jeder Download gegen seine SHA-256-Summe geprüft wird. Beides ist nicht zugleich erfüllbar: der Herausgeber veröffentlicht für die ISO keine Prüfsumme. Die harte Vorgabe gewinnt. virtio-win bekommt dieselbe Behandlung wie die Windows-Abbilder — die App öffnet die offizielle Seite, die Datei holt der Benutzer. Eine ungeprüfte Treiber-ISO in einen Windows-Gast einzuhängen wäre genau das Anti-Ziel aus §1: etwas, das „geprüft" aussieht und es nicht ist.

---

## Review Focus

1. **Das Netz bricht mitten im Download ab** — die halbe Datei darf nicht als ISO gelten, und ein erneuter Versuch muss möglich sein. (Aufgabe 3)
2. **Die Prüfsumme passt nicht** (Spiegel neu bestückt, Übertragung verfälscht) — die Datei wird gelöscht und der Benutzer hört warum; sie landet **nie** unter dem Zielnamen. (Aufgabe 3)
3. **Die App wird während eines Downloads geschlossen** — beim nächsten Start darf keine VM so aussehen, als hätte sie eine ISO. (Aufgabe 4)
4. **Die Prüfsummendatei ist erreichbar, der Name darin aber nicht mehr im Verzeichnis** (Ubuntus Punktversionen) — die Auflösung nimmt den nächsten Kandidaten statt zu scheitern. (Aufgabe 2)
5. **Das Zielverzeichnis ist nicht beschreibbar** — der Fehler erscheint in der Oberfläche, und es bleibt keine `.part`-Datei zurück, die beim nächsten Start als Fortsetzung missverstanden wird. (Aufgabe 3)

---

### Task 1: Das `Http`-Merkmal und die Prüfsummenrechnung

**Files:**
- Create: `src/download.rs`, `tests/test_download.rs`
- Modify: `Cargo.toml`, `src/lib.rs`

**Interfaces:**
- Consumes: nichts.
- Produces:
  - `pub trait Http: Send + Sync { fn get_text(&self, url: &str) -> Result<String, String>; fn head_ok(&self, url: &str) -> bool; fn get_from(&self, url: &str, offset: u64) -> Result<(Box<dyn std::io::Read + Send>, Option<u64>), String>; }`
  - `pub struct UreqHttp;` mit `impl Http for UreqHttp`
  - `pub fn sha256_of(path: &Path, cancel: &AtomicBool) -> Result<String, String>`

`get_from` liefert den Datenstrom ab `offset` (per `Range`-Kopfzeile, nur wenn `offset > 0`) und die Gesamtlänge, soweit der Spiegel sie nennt. `sha256_of` liest in Blöcken und bricht bei gesetztem `cancel` ab, damit eine 6-GB-Datei das Beenden nicht blockiert.

- [ ] **Step 1: Write the failing tests**

```rust
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
    let dir = tempdir().unwrap();
    let path = dir.path().join("big");
    std::fs::write(&path, vec![7u8; 8 * 1024 * 1024]).unwrap();
    let err = sha256_of(&path, &AtomicBool::new(true))
        .expect_err("a cancelled digest must not pretend to be a digest");
    assert!(!err.is_empty());
}

#[test]
fn test_sha256_of_a_missing_file_names_the_path() {
    let err = sha256_of(Path::new("C:\\nope\\x.iso"), &AtomicBool::new(false)).unwrap_err();
    assert!(err.contains("x.iso"), "got: {}", err);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_download`
Expected: FAIL — `src/download.rs` existiert nicht.

- [ ] **Step 3: Implement**

`Cargo.toml` bekommt `ureq = "3.5"` und `sha2 = "0.11"`. `src/lib.rs` bekommt `pub mod download;`. `UreqHttp` setzt für jede Anfrage eine Zeitgrenze und folgt Weiterleitungen.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test --test test_download && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/download.rs src/lib.rs tests/test_download.rs
git commit -m "feat(download): an HTTP seam and a cancellable SHA-256"
```

---

### Task 2: Den heutigen Dateinamen und seine Summe auflösen

**Files:**
- Modify: `src/download.rs`, `tests/test_download.rs`

**Interfaces:**
- Consumes: `Http` aus Aufgabe 1.
- Produces:
  - `pub enum ChecksumStyle { TwoSpace, Starred, PerFile }`
  - `pub struct IsoSource { pub key: &'static str, pub dir_url: &'static str, pub checksum_url: &'static str, pub style: ChecksumStyle, pub name_prefix: &'static str, pub name_suffix: &'static str, pub exclude: &'static [&'static str] }`
  - `pub fn sources() -> &'static [IsoSource]`
  - `pub fn source_for(key: &str) -> Option<&'static IsoSource>`
  - `pub struct Resolved { pub url: String, pub file_name: String, pub sha256: String }`
  - `pub fn resolve(source: &IsoSource, http: &dyn Http) -> Result<Resolved, String>`

Die vier Schlüssel heißen `debian`, `ubuntu`, `arch`, `alpine` — genau die `iso_source`-Werte, die Teil 3a in den Vorlagen gesetzt hat. Bei `PerFile` (Alpine) ist `checksum_url` das **Verzeichnis**, aus dessen Auflistung die Kandidaten kommen; die Summe steht in `<dir_url><name>.sha256`.

- [ ] **Step 1: Write the failing tests**

```rust
/// An Http that answers from a table instead of the network.
struct FakeHttp { texts: HashMap<String, String>, present: HashSet<String> }

#[test]
fn test_debian_style_two_spaces() {
    let http = FakeHttp::new()
        .text("https://example/SHA256SUMS", "aaaa  debian-13.7.0-amd64-netinst.iso\nbbbb  debian-edu-13.7.0-amd64-netinst.iso\n")
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
        .text("https://example/SHA256SUMS", "bbbb  debian-edu-13.7.0-amd64-netinst.iso\n")
        .present("https://example/debian-edu-13.7.0-amd64-netinst.iso");
    assert!(
        resolve(&debian_like(), &http).is_err(),
        "no acceptable name must mean an error, not a wrong file"
    );
}

#[test]
fn test_ubuntu_style_strips_the_star_and_takes_the_newest() {
    let http = FakeHttp::new()
        .text("https://example/SHA256SUMS", "1111 *ubuntu-24.04.3-desktop-amd64.iso\n2222 *ubuntu-24.04.4-desktop-amd64.iso\n")
        .present("https://example/ubuntu-24.04.4-desktop-amd64.iso");
    let r = resolve(&ubuntu_like(), &http).unwrap();
    assert_eq!(r.file_name, "ubuntu-24.04.4-desktop-amd64.iso");
    assert_eq!(r.sha256, "2222");
}

#[test]
fn test_a_name_in_the_checksum_file_that_is_no_longer_served_is_skipped() {
    // Review Focus 4: Ubuntu lists point releases that have been removed.
    let http = FakeHttp::new()
        .text("https://example/SHA256SUMS", "1111 *ubuntu-24.04.3-desktop-amd64.iso\n2222 *ubuntu-24.04.4-desktop-amd64.iso\n")
        .present("https://example/ubuntu-24.04.3-desktop-amd64.iso");  // only the older one
    let r = resolve(&ubuntu_like(), &http).unwrap();
    assert_eq!(r.file_name, "ubuntu-24.04.3-desktop-amd64.iso");
}

#[test]
fn test_arch_uses_its_stable_name() {
    let http = FakeHttp::new()
        .text("https://example/sha256sums.txt", "6666  archlinux-2026.10.01-x86_64.iso\n6666  archlinux-x86_64.iso\n0000  archlinux-bootstrap-x86_64.tar.zst\n")
        .present("https://example/archlinux-x86_64.iso");
    let r = resolve(&arch_like(), &http).unwrap();
    assert_eq!(r.file_name, "archlinux-x86_64.iso");
    assert_eq!(r.sha256, "6666");
}

#[test]
fn test_alpine_reads_a_checksum_file_per_image_and_skips_candidates() {
    let http = FakeHttp::new()
        .text("https://example/", "<a href=\"alpine-standard-3.24.0-x86_64.iso\">x</a><a href=\"alpine-standard-3.24.0_rc2-x86_64.iso\">y</a>")
        .text("https://example/alpine-standard-3.24.0-x86_64.iso.sha256", "dddd  alpine-standard-3.24.0-x86_64.iso\n")
        .present("https://example/alpine-standard-3.24.0-x86_64.iso");
    let r = resolve(&alpine_like(), &http).unwrap();
    assert_eq!(r.file_name, "alpine-standard-3.24.0-x86_64.iso");
    assert_eq!(r.sha256, "dddd", "a release candidate is not a release");
}

#[test]
fn test_an_unreachable_checksum_file_is_an_error_not_an_unchecked_download() {
    let http = FakeHttp::new();  // answers nothing
    assert!(resolve(&debian_like(), &http).is_err(), "no checksum must mean no download");
}

#[test]
fn test_all_four_sources_are_registered_with_the_keys_the_templates_use() {
    for key in ["debian", "ubuntu", "arch", "alpine"] {
        assert!(source_for(key).is_some(), "{}", key);
    }
    assert!(source_for("virtio-win").is_none(), "its publisher gives no SHA-256");
    for t in faro_qem::templates::VmTemplate::all() {
        if let Some(key) = t.iso_source {
            assert!(source_for(key).is_some(), "template {} names unknown source {}", t.key, key);
        }
    }
}

#[test]
#[ignore = "reaches the real mirrors; run with --ignored"]
fn test_the_real_mirrors_still_answer_in_the_shape_we_expect() {
    for source in sources() {
        let r = resolve(source, &UreqHttp).unwrap_or_else(|e| panic!("{}: {}", source.key, e));
        assert_eq!(r.sha256.len(), 64, "{} gave {:?}", source.key, r.sha256);
        assert!(r.file_name.ends_with(".iso"), "{}: {}", source.key, r.file_name);
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_download`
Expected: FAIL — `resolve`, `sources`, `IsoSource` existieren nicht.

- [ ] **Step 3: Implement**

`resolve` holt die Prüfsummendatei (bei `PerFile` die Verzeichnisauflistung), sammelt alle Namen, die mit `name_prefix` beginnen, auf `name_suffix` enden und keinen `exclude`-Teil enthalten, sortiert sie absteigend und nimmt den ersten, für den `head_ok` wahr ist. Bleibt keiner, ist das ein Fehler — **niemals ein ungeprüfter Download**.

Für Alpine gehört `_rc` in `exclude`, für Debian `debian-edu-` und `debian-mac-`; für Arch wählt das Suffix `-x86_64.iso` die ISO und schließt `.tar.zst` aus.

Der ignorierte Test ist der einzige, der ins Netz greift; er gehört in die Pflege, nicht in jeden Lauf.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test --test test_download && cargo test --test test_download -- --ignored --nocapture`
Expected: PASS für beide. Scheitert der ignorierte Test, hat ein Spiegel sein Format geändert — ein echter Befund, der ins Ledger gehört.

- [ ] **Step 5: Commit**

```bash
git add src/download.rs tests/test_download.rs
git commit -m "feat(download): read today's filename and its digest from the publisher"
```

---

### Task 3: Der Arbeitsvorgang — Fortschritt, Abbruch, Prüfung, Umbenennen

**Files:**
- Modify: `src/download.rs`, `tests/test_download.rs`

**Interfaces:**
- Consumes: `Http`, `sha256_of`, `Resolved`.
- Produces:
  - `pub enum DownloadEvent { Progress { done: u64, total: Option<u64> }, Verifying, Done(PathBuf), Failed(String) }`
  - `pub struct DownloadHandle { pub cancel: Arc<AtomicBool>, pub rx: Receiver<DownloadEvent> }`
  - `pub fn start(http: Arc<dyn Http>, resolved: Resolved, target: PathBuf) -> DownloadHandle`
  - `pub fn part_path(target: &Path) -> PathBuf`

Ablauf im Faden: nach `<target>.part` schreiben und `Progress` senden; war schon eine `.part` da, mit `get_from(offset)` fortsetzen; am Ende `Verifying` senden, `sha256_of` rechnen, bei Gleichheit nach `target` umbenennen und `Done` senden, sonst **die `.part` löschen** und `Failed` senden.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_a_good_download_lands_under_the_target_name() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("install.iso");
    let body = b"hello world".to_vec();
    let events = drain(start(Arc::new(FakeHttp::serving(&body)), resolved_for(&body), target.clone()));
    assert!(matches!(events.last(), Some(DownloadEvent::Done(_))), "got: {:?}", events);
    assert_eq!(std::fs::read(&target).unwrap(), body);
    assert!(!part_path(&target).exists(), "the partial file must be gone");
}

#[test]
fn test_progress_and_verifying_are_both_reported() {
    let dir = tempdir().unwrap();
    let body = vec![3u8; 512 * 1024];
    let events = drain(start(Arc::new(FakeHttp::serving(&body)), resolved_for(&body), dir.path().join("x.iso")));
    assert!(
        events.iter().any(|e| matches!(e, DownloadEvent::Progress { done, .. } if *done > 0)),
        "a progress bar needs something to show"
    );
    assert!(events.iter().any(|e| matches!(e, DownloadEvent::Verifying)));
}

#[test]
fn test_a_wrong_digest_discards_the_file_and_says_so() {
    // Review Focus 2, and Spec §6: a file that does not match is discarded,
    // not mounted.
    let dir = tempdir().unwrap();
    let target = dir.path().join("install.iso");
    let mut resolved = resolved_for(b"actual bytes");
    resolved.sha256 = "0".repeat(64);
    let events = drain(start(Arc::new(FakeHttp::serving(b"actual bytes")), resolved, target.clone()));
    match events.last() {
        Some(DownloadEvent::Failed(msg)) => {
            let m = msg.to_lowercase();
            assert!(m.contains("sha256") || m.contains("checksum"), "got: {}", msg);
        }
        other => panic!("got: {:?}", other),
    }
    assert!(!target.exists(), "an unverified file must never carry the target name");
    assert!(!part_path(&target).exists(), "and must not be left as a resumable partial");
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
    assert!(part_path(&target).exists(), "an interrupted transfer is worth resuming");
    assert!(!target.exists());

    let events = drain(start(Arc::new(FakeHttp::serving(&body)), resolved_for(&body), target.clone()));
    assert!(matches!(events.last(), Some(DownloadEvent::Done(_))), "got: {:?}", events);
    assert_eq!(std::fs::read(&target).unwrap(), body, "the resumed file must be the whole file");
}

#[test]
fn test_cancelling_stops_the_work_and_leaves_no_target() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("install.iso");
    let body = vec![1u8; 4 * 1024 * 1024];
    let handle = start(Arc::new(FakeHttp::serving(&body).slow()), resolved_for(&body), target.clone());
    handle.cancel.store(true, Ordering::SeqCst);
    let _ = drain(handle);
    assert!(!target.exists());
}

#[test]
fn test_an_unwritable_target_fails_without_leaving_a_partial() {
    // Review Focus 5.
    let dir = tempdir().unwrap();
    let blocker = dir.path().join("dir-is-a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let target = blocker.join("install.iso");
    let events = drain(start(Arc::new(FakeHttp::serving(b"x")), resolved_for(b"x"), target.clone()));
    assert!(matches!(events.last(), Some(DownloadEvent::Failed(_))), "got: {:?}", events);
    assert!(!part_path(&target).exists());
}
```

`FakeHttp::serving`, `.failing_after`, `.slow`, `resolved_for` und `drain` sind Testhilfen in `tests/test_download.rs`; `drain` sammelt den Kanal leer, bis er schließt.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_download`
Expected: FAIL — `start`, `DownloadEvent`, `part_path` existieren nicht.

- [ ] **Step 3: Implement**

Ein `std::thread::spawn`, ein `mpsc::channel`, Blöcke von 64 KB, `Progress` höchstens alle 250 ms oder alle 4 MB, damit der Kanal nicht zum Engpass wird. Der `cancel`-Wert wird je Block geprüft und an `sha256_of` weitergegeben.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/download.rs tests/test_download.rs
git commit -m "feat(download): a worker that reports, resumes, verifies, and discards"
```

---

### Task 4: Im Wizard, und die Startprüfung wartet

**Files:**
- Modify: `src/ui/wizard.rs`, `src/app.rs`, `src/preflight.rs`, `src/download.rs`, `assets/locales/*`
- Test: `tests/test_wizard.rs`, `tests/test_preflight.rs`, `tests/test_app.rs`

**Interfaces:**
- Consumes: `start`, `resolve`, `source_for`, `part_path`, `VmTemplate.iso_source`, `check_vm`.
- Produces: `FaroQemApp.downloads: HashMap<Uuid, DownloadHandle>`; `WizardState.download_requested: bool`; `pub fn open_in_browser(url: &str) -> Result<(), String>`; ein neuer blockierender Befund in `check_vm`.

Spec §11.5: die VM entsteht **sofort**, mit dem Zielpfad als ISO; der Download läuft weiter, und die Startprüfung hält den Start, bis die Datei da und geprüft ist. Durch das `.part`-Verfahren braucht es dafür **kein gespeichertes Merkmal**: fehlt die ISO und liegt eine `.part` daneben, ist der Download unfertig — und das stimmt auch nach einem App-Neustart, bei dem der Faden längst tot ist.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_the_download_button_appears_only_where_there_is_a_stable_link() {
    let i18n = I18n::new("en");
    for key in ["debian", "ubuntu", "arch", "alpine"] {
        let mut state = WizardState { open: true, ..Default::default() };
        state.select(key);
        assert!(render_wizard(&mut state).contains(&i18n.t("download")), "{}", key);
    }
    for key in ["windows-11", "windows-10"] {
        let mut state = WizardState { open: true, ..Default::default() };
        state.select(key);
        let drawn = render_wizard(&mut state);
        assert!(!drawn.contains(&i18n.t("download")), "{} has no stable link", key);
        assert!(drawn.contains(&i18n.t("open_official_page")), "{} drew:\n{}", key, drawn);
    }
}

#[test]
fn test_the_wizard_states_what_a_checksum_proves_and_what_it_does_not() {
    // Spec §6: the limit must be in the interface, not only in the document.
    let i18n = I18n::new("en");
    let mut state = WizardState { open: true, ..Default::default() };
    state.select("debian");
    assert!(render_wizard(&mut state).contains(&i18n.t("download_limit_notice")));
}

#[test]
fn test_an_unfinished_download_holds_the_start_and_names_the_file() {
    // Review Focus 3, and Spec §11.5.
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    let iso = dir.path().join("debian.iso");
    std::fs::write(part_path(&iso), b"half of it").unwrap();
    vm.cdroms.push(iso);

    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    let blocking: Vec<_> = findings.iter().filter(|f| f.severity == Severity::Blocking).collect();
    assert_eq!(blocking.len(), 1, "got: {:?}", findings);
    assert!(blocking[0].text.contains("debian.iso"), "got: {}", blocking[0].text);
}

#[test]
fn test_a_finished_download_no_longer_holds_the_start() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    let iso = dir.path().join("debian.iso");
    std::fs::write(&iso, b"all of it").unwrap();
    vm.cdroms.push(iso);
    assert!(!blocks(&check_vm(&vm, &host(), None, None, &I18n::new("en"))));
}

#[test]
fn test_creating_with_a_download_fills_the_iso_slot_right_away() {
    // The two-click promise has to hold for someone with no ISO yet.
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let mut draft = VmTemplate::by_key("debian").unwrap().instantiate("Debian", Path::new(""));
    draft.cdroms.push(PathBuf::from("placeholder.iso"));
    let id = app.create_from_wizard(draft).unwrap();
    assert_eq!(app.vms.iter().find(|v| v.id == id).unwrap().cdroms.len(), 1);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_wizard --test test_preflight --test test_app`
Expected: FAIL — die Schaltfläche, der Grenzhinweis und der `.part`-Befund fehlen.

- [ ] **Step 3: Implement**

- Am ISO-Feld des Wizards: bei gesetztem `iso_source` ein Knopf „Herunterladen", der den Zielpfad auf `<VM-Ordner>/<aufgelöster Name>` setzt und `download_requested` merkt; bei den Windows-Vorlagen stattdessen „Offizielle Seite öffnen". Dasselbe gilt für die virtio-win-Treiber-ISO, wo sie im Detailbereich angeboten wird.
- Unter dem Feld steht `download_limit_notice` — in allen drei Sprachen der Satz aus Spec §6: die Prüfsumme kommt über HTTPS von der Prüfsummendatei des Herausgebers und belegt die Unverfälschtheit der Übertragung, nicht die Echtheit der Herkunft; wer den Spiegel kontrolliert, kontrolliert beides.
- `create_from_wizard` startet den Download nach dem Anlegen, wenn `download_requested` gesetzt war, und legt den Griff in `app.downloads` ab. **`resolve` läuft im Faden**, nicht im Bild — sonst friert das Anlegen bei einem langsamen Spiegel ein.
- `FaroQemApp::update` leert die Kanäle einmal je Bild ohne zu blockieren (`try_recv` in einer Schleife), schreibt den Fortschritt in die Konsole der VM und zeigt ihn im Detailbereich.
- `check_vm` bekommt den Befund: fehlt eine ISO, liegt aber `part_path(&iso)` vor, ist das **blockierend** mit dem Dateinamen im Text — ein unfertiger Download, nicht eine fehlende Datei.
- `open_in_browser`: auf Windows `cmd /c start "" <url>`, auf macOS `open`, sonst `xdg-open`.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A src tests assets/locales
git commit -m "feat: download at the ISO field, and a start that waits for the bytes"
```

---

## Selbstprüfung

**Spec-Abdeckung:** §6 Download der vier Linux-ISOs → Aufgaben 2 bis 4. §6 Prüfung jeder Datei → Aufgabe 3. §6 ehrliche Grenze in der Oberfläche → Aufgabe 4. §6 keine Windows-ISOs, Seite im Browser → Aufgabe 4. §6 virtio-win → **eingeschränkt**, siehe die Festlegung oben: kein Download, weil der Herausgeber keine SHA-256-Summe veröffentlicht. §6 Oberfläche friert nicht ein → Aufgabe 3 (eigener Faden) und Aufgabe 4 (`try_recv`, Auflösung im Faden). §11.5 VM entsteht sofort → Aufgabe 4. §11.8 erlaubte Abhängigkeiten → `ureq`, `sha2`.

**Bewusst offen gelassen:** Gestalt der Fortschrittsanzeige (Spec §6 überlässt es), Blockgröße und Berichtsfrequenz, ob der ignorierte Spiegel-Test in eine CI-Pflege wandert.

**Nicht in diesem Plan:** Laufzeitverhalten (Teil 4).
