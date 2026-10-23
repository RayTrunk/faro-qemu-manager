# Teil 1 — Datenverlust schließen: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine unlesbare `vms.json` oder `settings.json` kostet keine einzige Konfiguration mehr, und fehlgeschlagene Schreibvorgänge werden sichtbar statt verschluckt.

**Architecture:** `src/config.rs` bekommt Lader, die „fehlt" von „kaputt" unterscheiden (`try_load_*` mit `ConfigLoadError`), atomares Schreiben über temp+rename, und eine Quarantäne-Funktion, die eine kaputte Datei zeitgestempelt beiseitelegt. `FaroQemApp` sammelt Meldungen in `notices: Vec<Notice>`; ein neues, schmales UI-Modul `src/ui/notice.rs` stellt sie als abweisbares Band über der Menüleiste dar. Die bisherigen lenienten Funktionen `load_vms`/`load_settings` bleiben als Hülle bestehen, damit der öffentliche Bestand und die 86 Tests unangetastet sind.

**Tech Stack:** Rust 2021, `eframe`/`egui` 0.29.1, `serde`/`serde_json`, `tempfile` (nur Tests). **Keine neuen Abhängigkeiten in diesem Teil** — Zeitstempel kommen aus `std::time::SystemTime`.

**Spec:** `docs/superpowers/specs/2026-10-23-wizard-und-haertung-design.md`, §4 (mit den Tatsachen aus §3.4)

## Global Constraints

- Rust 2021, `eframe`/`egui` 0.29.1 — kein Framework-Wechsel (Spec §2).
- Keine neuen Abhängigkeiten in diesem Teil. Erlaubt wären laut Spec §2/§11.8 nur HTTP, SHA-256 und Wirtsspeicher-Abfrage — alle drei gehören zu späteren Teilen.
- Die Sammlung von 86 Tests läuft nach der Arbeit grün durch. In diesem Teil ändert sich keine bestehende Zusicherung; `tests/test_app.rs::test_faro_qem_app_initialization_defaults` und `tests/test_config.rs::test_save_and_load_vms_persistence` müssen unverändert passieren.
- Eine nicht lesbare Konfiguration wird **niemals überschrieben** (Spec §4).
- „Datei leer/fehlt" und „Datei kaputt" werden auseinandergehalten; nur der erste Fall führt zur Standard-VM (Spec §4).
- Schreibfehler werden nicht verschluckt (Spec §4).
- Neue Benutzertexte gehören in alle drei Kataloge `assets/locales/{en,de,fr}.json` (Spec §8).
- Code und Kommentare auf Englisch, wie im Bestand. `cargo fmt --check` und `cargo clippy --all-targets` bleiben ohne Beanstandung.

## Review Focus

Fünf Eingaben, die die Spec voraussetzt, ohne sie zu nennen. Jede bekommt ihren Test in der Aufgabe, die den Code besitzt:

1. **Gültiges JSON mit falscher Gestalt** — `vms.json` enthält ein Objekt statt eines Arrays. Muss als „kaputt" gelten, nicht als leer, und darf nicht panikken. → Aufgabe 2.
2. **Datei mit 0 Bytes** — entsteht, wenn ein Schreibvorgang ganz am Anfang abbricht. Muss als „leer/fehlt" gelten und zur Standard-VM führen, nicht in Quarantäne. → Aufgabe 2.
3. **Zwei Quarantänen in derselben Sekunde** — der Zielname existiert bereits. Die ältere Quarantäne darf nicht überschrieben werden. → Aufgabe 3.
4. **Quarantäne schlägt selbst fehl** (schreibgeschützter Ordner). Die App muss trotzdem starten und sagen, dass die Datei *nicht* gesichert werden konnte — und sie darf sie dann erst recht nicht überschreiben. → Aufgabe 5.
5. **Elternverzeichnis existiert nicht** — der heutige Code legt es an. Das muss mit temp+rename erhalten bleiben. → Aufgabe 1.

---

## File Structure

- `src/config.rs` — Laden, Speichern, Quarantäne. Bekommt `ConfigLoadError`, `try_load_vms`, `try_load_settings`, `quarantine_file`; `write_config_file` wird atomar.
- `src/ui/notice.rs` — **neu.** Meldungsmodell (`Notice`, `NoticeLevel`) und das Band (`NoticeBar`). Eigene Datei, weil `src/ui/vm_detail.rs` mit 1350 Zeilen schon zu groß ist (Spec §8) und Meldungen nichts mit VM-Details zu tun haben.
- `src/ui/mod.rs` — Modul anmelden.
- `src/app.rs` — `notices`-Feld, Startbehandlung in `with_paths`, Schreibfehler melden, Band darstellen.
- `assets/locales/{en,de,fr}.json` — Texte für die Meldungen.
- `tests/test_config_resilience.rs` — **neu.** Alles auf `config.rs`-Ebene.
- `tests/test_app_resilience.rs` — **neu.** Alles auf App-Ebene, inklusive der Regression zum bewiesenen Datenverlust.

---

## Task 1: Atomares Schreiben

**Files:**
- Modify: `src/config.rs:426-434` (`write_config_file`)
- Test: `tests/test_config_resilience.rs` (create)

**Interfaces:**
- Consumes: nichts.
- Produces: `write_config_file` bleibt privat; die Signaturen von `save_vms(path: &Path, vms: &[VmConfig]) -> Result<(), String>` und `save_settings(path: &Path, settings: &AppSettings) -> Result<(), String>` bleiben unverändert, nur die Haltbarkeit ändert sich.

- [ ] **Step 1: Write the failing tests**

Der erste Test ist der eigentliche: Ein Leser, der die Datei während des
Schreibens liest, darf **niemals** unvollständiges JSON sehen. Mit dem heutigen
`std::fs::write` wird das Ziel zuerst gekürzt und dann gefüllt — ein
gleichzeitiger Leser erwischt den Torso. Mit temp+rename ist das unmöglich, weil
der Leser entweder die alte oder die neue Datei sieht, nie eine halbe.

Die beiden anderen Tests sind Haltelinien: sie bestehen heute schon und dürfen
durch den Umbau nicht brechen.

```rust
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

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let writer = {
        let path = path.clone();
        let vms = vms.clone();
        let stop = std::sync::Arc::clone(&stop);
        std::thread::spawn(move || {
            for _ in 0..40 {
                save_vms(&path, &vms).expect("write keeps succeeding");
            }
            stop.store(true, std::sync::atomic::Ordering::SeqCst);
        })
    };

    let mut torn = 0usize;
    let mut reads = 0usize;
    while !stop.load(std::sync::atomic::Ordering::SeqCst) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            reads += 1;
            if !text.is_empty() && serde_json::from_str::<Vec<VmConfig>>(&text).is_err() {
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
```

- [ ] **Step 2: Run tests to verify the atomicity test fails**

Run: `cargo test --test test_config_resilience -- --nocapture`
Expected: `test_concurrent_reader_never_sees_a_half_written_config` FAILS mit „a reader saw N torn file(s)". Die beiden anderen Tests bestehen bereits — sie sichern nur, dass der Umbau nichts kaputt macht.

**Wenn der Atomaritätstest wider Erwarten durchläuft, hat er keine Zähne** — dann die Anzahl der VMs oder der Schreibdurchläufe erhöhen, bis er zuverlässig fehlschlägt, und erst danach weiter. Ein Test, der die Lücke nicht zeigt, bewacht sie auch später nicht.

- [ ] **Step 3: Make `write_config_file` atomic in `src/config.rs`**

Signatur bleibt `fn write_config_file(path: &Path, data: &[u8]) -> Result<(), String>`. Vorgehen: Elternverzeichnis wie bisher anlegen, dann in eine Nachbardatei im **selben** Verzeichnis schreiben (gleiches Laufwerk, sonst schlägt das Umbenennen fehl), die Nachbardatei schließen, dann mit `std::fs::rename` über das Ziel bewegen — das ersetzt auf Windows wie auf Unix ein vorhandenes Ziel. Schlägt ein Schritt fehl, die Nachbardatei aufräumen und den Fehler mit Pfad im Text zurückgeben.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_config_resilience`
Expected: PASS (3 Tests)

- [ ] **Step 5: Run the whole suite and the lints**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets`
Expected: alle Tests grün, keine Beanstandung

- [ ] **Step 6: Commit**

```bash
git add src/config.rs tests/test_config_resilience.rs
git commit -m "fix(config): write configuration files atomically via temp+rename"
```

---

## Task 2: Lader, die „fehlt" von „kaputt" unterscheiden

**Files:**
- Modify: `src/config.rs:389-416` (`load_vms`, `load_settings`)
- Test: `tests/test_config_resilience.rs`

**Interfaces:**
- Consumes: Aufgabe 1 — nur indirekt über `save_*` in den Tests.
- Produces:
  - `pub enum ConfigLoadError { Missing, Unreadable(std::io::Error), Corrupt { detail: String } }`, abgeleitet `Debug`.
  - `pub fn try_load_vms(path: &Path) -> Result<Vec<VmConfig>, ConfigLoadError>`
  - `pub fn try_load_settings(path: &Path) -> Result<AppSettings, ConfigLoadError>`
  - `load_vms(path: &Path) -> Vec<VmConfig>` und `load_settings(path: &Path) -> AppSettings` behalten Signatur und Verhalten, werden aber Hüllen um `try_load_*`.

- [ ] **Step 1: Write the failing tests**

```rust
use faro_qem::config::{try_load_settings, try_load_vms, ConfigLoadError};

#[test]
fn test_try_load_vms_reports_missing_file() {
    let dir = tempdir().unwrap();
    let err = try_load_vms(&dir.path().join("nope.json")).unwrap_err();
    assert!(matches!(err, ConfigLoadError::Missing));
}

#[test]
fn test_try_load_vms_reads_valid_file() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");
    save_vms(&path, &[VmConfig::new_default("A", Arch::X86_64)]).unwrap();

    let loaded = try_load_vms(&path).expect("valid file loads");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "A");
}

#[test]
fn test_try_load_vms_reports_truncated_file_as_corrupt() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");
    std::fs::write(&path, r#"[{"name":"KEEP ME","arch":"x86_64"},{"#).unwrap();

    let err = try_load_vms(&path).unwrap_err();
    assert!(matches!(err, ConfigLoadError::Corrupt { .. }));
}

// Review Focus 1
#[test]
fn test_try_load_vms_reports_wrong_shape_as_corrupt() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");
    std::fs::write(&path, r#"{"vms": []}"#).unwrap();

    let err = try_load_vms(&path).unwrap_err();
    assert!(matches!(err, ConfigLoadError::Corrupt { .. }));
}

// Review Focus 2
#[test]
fn test_try_load_vms_treats_empty_file_as_missing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");
    std::fs::write(&path, "").unwrap();

    let err = try_load_vms(&path).unwrap_err();
    assert!(
        matches!(err, ConfigLoadError::Missing),
        "a zero-byte file is an aborted write, not a corrupt config"
    );
}

#[test]
fn test_try_load_settings_distinguishes_missing_from_corrupt() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    assert!(matches!(
        try_load_settings(&path).unwrap_err(),
        ConfigLoadError::Missing
    ));

    std::fs::write(&path, "{not json").unwrap();
    assert!(matches!(
        try_load_settings(&path).unwrap_err(),
        ConfigLoadError::Corrupt { .. }
    ));
}

#[test]
fn test_lenient_loaders_keep_their_old_behaviour() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");
    std::fs::write(&path, "{broken").unwrap();

    assert!(faro_qem::config::load_vms(&path).is_empty());
    assert_eq!(
        faro_qem::config::load_settings(&path).language,
        "en",
        "lenient loader still falls back to defaults"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test test_config_resilience`
Expected: FAIL — `cannot find type ConfigLoadError` / `cannot find function try_load_vms`

- [ ] **Step 3: Implement `ConfigLoadError`, `try_load_vms`, `try_load_settings` in `src/config.rs`**

Prüfreihenfolge in beiden `try_load_*`: Datei nicht vorhanden → `Missing`; Lesefehler → `Unreadable`; Inhalt nach `trim()` leer → `Missing`; `serde_json`-Fehler → `Corrupt { detail }` mit der serde-Meldung im Text. `load_vms`/`load_settings` rufen `try_load_*` und fallen bei jedem `Err` auf `Vec::new()` bzw. `AppSettings::default()` zurück — ein Doc-Kommentar vermerkt, dass sie nachsichtig sind und neuer Code `try_load_*` nehmen soll.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_config_resilience`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/config.rs tests/test_config_resilience.rs
git commit -m "feat(config): distinguish a missing config from a corrupt one"
```

---

## Task 3: Quarantäne

**Files:**
- Modify: `src/config.rs` (neue öffentliche Funktion)
- Test: `tests/test_config_resilience.rs`

**Interfaces:**
- Consumes: nichts.
- Produces: `pub fn quarantine_file(path: &Path) -> Result<PathBuf, String>` — bewegt `path` auf einen zeitgestempelten Nachbarnamen und gibt dessen Pfad zurück. Bewegt, nicht kopiert: danach ist `path` frei.

- [ ] **Step 1: Write the failing tests**

```rust
use faro_qem::config::quarantine_file;

#[test]
fn test_quarantine_moves_file_aside_with_content_intact() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");
    std::fs::write(&path, "ORIGINAL CONTENT").unwrap();

    let moved = quarantine_file(&path).expect("quarantine succeeds");

    assert!(moved != path);
    assert_eq!(moved.parent(), path.parent(), "stays next to the original");
    assert_eq!(std::fs::read_to_string(&moved).unwrap(), "ORIGINAL CONTENT");
    assert!(!path.exists(), "original path is freed for a fresh start");
}

// Review Focus 3
#[test]
fn test_quarantine_twice_keeps_both_copies() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");

    std::fs::write(&path, "FIRST").unwrap();
    let first = quarantine_file(&path).unwrap();

    std::fs::write(&path, "SECOND").unwrap();
    let second = quarantine_file(&path).unwrap();

    assert_ne!(first, second, "the earlier quarantine must not be clobbered");
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "FIRST");
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "SECOND");
}

#[test]
fn test_quarantine_reports_error_for_missing_file() {
    let dir = tempdir().unwrap();
    assert!(quarantine_file(&dir.path().join("absent.json")).is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test test_config_resilience`
Expected: FAIL — `cannot find function quarantine_file`

- [ ] **Step 3: Implement `quarantine_file` in `src/config.rs`**

Name: ursprünglicher Dateiname plus `.corrupt-<unix-sekunden>`, z. B. `vms.json.corrupt-1792751825`. Zeit aus `SystemTime::now().duration_since(UNIX_EPOCH)`; schlägt das fehl, `0` verwenden. Existiert der Name bereits (Review Focus 3), einen aufsteigenden Zähler anhängen, bis ein freier Name gefunden ist — niemals ein vorhandenes Ziel ersetzen. Verschieben mit `std::fs::rename`; schlägt das fehl, Fehler mit beiden Pfaden im Text zurückgeben.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_config_resilience`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/config.rs tests/test_config_resilience.rs
git commit -m "feat(config): add quarantine_file to set a corrupt config aside"
```

---

## Task 4: Meldungsmodell und Band

**Files:**
- Create: `src/ui/notice.rs`
- Modify: `src/ui/mod.rs:4-14`
- Modify: `assets/locales/en.json`, `assets/locales/de.json`, `assets/locales/fr.json`
- Test: `tests/test_app_resilience.rs` (create)

**Interfaces:**
- Consumes: nichts.
- Produces:
  - `pub enum NoticeLevel { Info, Warning, Error }` — `Debug, Clone, Copy, PartialEq, Eq`
  - `pub struct Notice { pub level: NoticeLevel, pub text: String }` — `Debug, Clone, PartialEq, Eq`
  - `impl Notice { pub fn info(text: impl Into<String>) -> Self; pub fn warning(text: impl Into<String>) -> Self; pub fn error(text: impl Into<String>) -> Self }`
  - `pub struct NoticeBar<'a> { pub notices: &'a mut Vec<Notice> }`
  - `impl<'a> NoticeBar<'a> { pub fn new(notices: &'a mut Vec<Notice>) -> Self; pub fn show(self, ui: &mut egui::Ui, i18n: &crate::i18n::I18n) }`

  `show` zeichnet je Meldung eine Zeile mit Symbol in der Farbe des Grads (`COLOR_ERROR`, `COLOR_WARNING`, `COLOR_PRIMARY` aus `crate::ui::theme`) und einen Abweisen-Knopf; abgewiesene Meldungen entfernt es aus dem Vektor. Bei leerem Vektor zeichnet es nichts.

- [ ] **Step 1: Add the new catalogue keys to all three catalogues**

Schlüssel mit englischem Text; deutsche und französische Fassung sinngemäß, Platzhalter `{file}`, `{path}`, `{detail}` bleiben wörtlich stehen:

| Schlüssel | en |
|---|---|
| `notice_dismiss` | `Dismiss` |
| `config_corrupt_quarantined` | `Could not read {file}. The file was kept at {path} and nothing was overwritten.` |
| `config_corrupt_not_quarantined` | `Could not read {file}, and it could not be moved aside either ({detail}). The file was left untouched.` |
| `config_save_failed` | `Could not save {file}: {detail}` |

- [ ] **Step 2: Write the failing test**

```rust
use egui::Context;
use faro_qem::i18n::I18n;
use faro_qem::ui::notice::{Notice, NoticeBar, NoticeLevel};

#[test]
fn test_notice_bar_renders_and_dismisses() {
    let i18n = I18n::new("en");
    let mut notices = vec![
        Notice::error("something broke"),
        Notice::warning("something is odd"),
        Notice::info("something happened"),
    ];
    assert_eq!(notices[0].level, NoticeLevel::Error);

    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            NoticeBar::new(&mut notices).show(ui, &i18n);
        });
    });
    assert_eq!(notices.len(), 3, "rendering alone dismisses nothing");

    notices.clear();
    let ctx2 = Context::default();
    let _ = ctx2.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            NoticeBar::new(&mut notices).show(ui, &i18n);
        });
    });
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --test test_app_resilience`
Expected: FAIL — `unresolved import faro_qem::ui::notice`

- [ ] **Step 4: Implement `src/ui/notice.rs` and register it in `src/ui/mod.rs`**

In `src/ui/mod.rs` neben die bestehenden Module: `pub mod notice;` und `pub use notice::*;`, in derselben Form wie die anderen.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --test test_app_resilience`
Expected: PASS

- [ ] **Step 6: Verify all three catalogues still have identical key sets**

Run: `cargo test --test test_i18n`
Expected: PASS — der Bestand prüft die Deckungsgleichheit der Kataloge

- [ ] **Step 7: Commit**

```bash
git add src/ui/notice.rs src/ui/mod.rs assets/locales tests/test_app_resilience.rs
git commit -m "feat(ui): add a dismissible notice bar for configuration problems"
```

---

## Task 5: Der Start zerstört keine kaputte Konfiguration mehr

Das ist die Regression zum in Spec §3.4 bewiesenen Datenverlust.

**Files:**
- Modify: `src/app.rs:24-42` (Feld `notices`), `src/app.rs:65-116` (`with_paths`)
- Test: `tests/test_app_resilience.rs`

**Interfaces:**
- Consumes: `try_load_vms`, `try_load_settings`, `ConfigLoadError`, `quarantine_file` (Aufgaben 2–3); `Notice` (Aufgabe 4).
- Produces: `FaroQemApp` bekommt `pub notices: Vec<Notice>`. `with_paths(settings_path: PathBuf, vms_path: PathBuf, ctx: Option<&egui::Context>) -> Self` behält die Signatur.

- [ ] **Step 1: Write the failing tests**

```rust
use faro_qem::app::FaroQemApp;

#[test]
fn test_corrupt_vms_json_is_preserved_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let vms_path = dir.path().join("vms.json");
    std::fs::write(
        &vms_path,
        r#"[{"id":"550e8400-e29b-41d4-a716-446655440000","name":"WICHTIGE VM","arch":"x86_64","cpu_cores":8,"memory_mb":16384},{"#,
    )
    .unwrap();

    let app = FaroQemApp::with_paths(dir.path().join("settings.json"), vms_path.clone(), None);

    assert!(
        app.vms.is_empty(),
        "a corrupt file must not be replaced by a default VM"
    );
    assert!(app.notices.iter().any(|n| n.level == NoticeLevel::Error));

    let preserved: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains("corrupt"))
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    assert_eq!(preserved.len(), 1, "exactly one quarantined copy");
    assert!(
        preserved[0].contains("WICHTIGE VM"),
        "the original bytes survived"
    );

    let notice = app
        .notices
        .iter()
        .find(|n| n.level == NoticeLevel::Error)
        .unwrap();
    assert!(
        notice.text.contains("corrupt"),
        "the notice names where the file went: {}",
        notice.text
    );
}

#[test]
fn test_missing_vms_json_still_creates_the_default_vm() {
    let dir = tempfile::tempdir().unwrap();
    let app = FaroQemApp::with_paths(
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
        None,
    );

    assert_eq!(app.vms.len(), 1);
    assert_eq!(app.vms[0].name, "Debian 12");
    assert!(app.notices.is_empty(), "a first start is not a problem");
}

#[test]
fn test_corrupt_settings_json_is_preserved_and_defaults_are_used() {
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    std::fs::write(&settings_path, r#"{"language":"de","theme":"#).unwrap();

    let app = FaroQemApp::with_paths(settings_path.clone(), dir.path().join("vms.json"), None);

    assert_eq!(app.settings.language, "en", "falls back to defaults");
    assert!(app.notices.iter().any(|n| n.level == NoticeLevel::Error));
    assert!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().contains("corrupt")),
        "the unreadable settings file was kept"
    );
}

// Review Focus 4
#[test]
fn test_app_starts_even_when_quarantine_fails() {
    let dir = tempfile::tempdir().unwrap();
    // A directory where the config file is expected: unreadable as JSON and
    // impossible to rename onto a plain file name on every platform.
    let vms_path = dir.path().join("vms.json");
    std::fs::create_dir(&vms_path).unwrap();

    let app = FaroQemApp::with_paths(dir.path().join("settings.json"), vms_path.clone(), None);

    assert!(vms_path.is_dir(), "nothing was destroyed");
    assert!(
        app.notices.iter().any(|n| n.level == NoticeLevel::Error),
        "the failure is reported rather than swallowed"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test test_app_resilience`
Expected: FAIL — `no field notices on type FaroQemApp`; und `test_corrupt_vms_json_is_preserved_not_overwritten` schlägt mit dem heutigen Verhalten fehl, weil die Datei überschrieben wird

- [ ] **Step 3: Add `notices: Vec<Notice>` to `FaroQemApp` and rework `with_paths`**

Ablauf in `with_paths`: `try_load_settings` und `try_load_vms` statt der nachsichtigen Lader. Bei `Ok` wie bisher weiter. Bei `Missing` die bisherigen Vorgaben (Standard-Einstellungen, Standard-VM samt Speichern). Bei `Unreadable` oder `Corrupt` **nicht speichern**, stattdessen `quarantine_file` versuchen und das Ergebnis vermerken: gelingt es, `Notice::error` aus `config_corrupt_quarantined` mit Dateiname und Zielpfad; gelingt es nicht, `Notice::error` aus `config_corrupt_not_quarantined` mit dem Grund. In beiden Fällen leere VM-Liste bzw. `AppSettings::default()` und **kein** Schreiben in diesem Durchlauf.

Weil die Texte aus dem Katalog kommen, wird `I18n` vor dem Erzeugen der Meldungen gebraucht — bei kaputten Einstellungen also mit der Standardsprache. Für die Platzhalter ist `I18n::tf` da (heute nur im Test benutzt).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_app_resilience`
Expected: PASS

- [ ] **Step 5: Verify the existing suite is untouched**

Run: `cargo test`
Expected: PASS — insbesondere `test_faro_qem_app_initialization_defaults`

- [ ] **Step 6: Commit**

```bash
git add src/app.rs tests/test_app_resilience.rs
git commit -m "fix(app): never overwrite an unreadable configuration on startup"
```

---

## Task 6: Schreibfehler werden sichtbar

**Files:**
- Modify: `src/app.rs:80,269,279,289,317,618-619` (alle `let _ = save_vms(...)` / `let _ = save_settings(...)`)
- Test: `tests/test_app_resilience.rs`

**Interfaces:**
- Consumes: `Notice` (Aufgabe 4), `notices`-Feld (Aufgabe 5).
- Produces: `impl FaroQemApp { fn record_save_result(&mut self, file_label: &str, result: Result<(), String>) }` — privat; bei `Err` eine `Notice::error` aus `config_save_failed` anhängen, bei `Ok` nichts tun.

- [ ] **Step 1: Write the failing test**

```rust
use faro_qem::ui::sidebar::SidebarAction;

#[test]
fn test_failing_save_is_reported_to_the_user() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = FaroQemApp::with_paths(
        dir.path().join("settings.json"),
        dir.path().join("vms.json"),
        None,
    );
    app.notices.clear();

    // Make the target unwritable by putting a directory in its place.
    let blocked = dir.path().join("blocked").join("vms.json");
    std::fs::create_dir_all(&blocked).unwrap();
    app.vms_path = blocked;

    app.handle_sidebar_action(SidebarAction::NewVm, None);

    assert!(
        app.notices.iter().any(|n| n.level == NoticeLevel::Error),
        "a save that failed must not be silent"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_app_resilience test_failing_save_is_reported_to_the_user`
Expected: FAIL — keine Meldung, weil der Fehler mit `let _ =` verworfen wird

- [ ] **Step 3: Replace every discarded save result in `src/app.rs` with `record_save_result`**

Alle Stellen außer der in `Drop` (`src/app.rs:618-619`). Die behält `let _ =`, mit einem Kommentar warum: beim Beenden gibt es keine Oberfläche mehr, die eine Meldung zeigen könnte.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_app_resilience`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/app.rs tests/test_app_resilience.rs
git commit -m "fix(app): surface configuration save failures instead of discarding them"
```

---

## Task 7: Band in der Oberfläche

**Files:**
- Modify: `src/app.rs:351-359` (Anfang von `render_ui`)
- Test: `tests/test_app_resilience.rs`

**Interfaces:**
- Consumes: `NoticeBar` (Aufgabe 4), `notices` (Aufgabe 5).
- Produces: nichts Neues; `render_ui(&mut self, ctx: &egui::Context)` behält die Signatur.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_render_ui_shows_notices_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let vms_path = dir.path().join("vms.json");
    std::fs::write(&vms_path, "{broken").unwrap();

    let mut app = FaroQemApp::with_paths(dir.path().join("settings.json"), vms_path, None);
    assert!(!app.notices.is_empty(), "precondition: a notice exists");

    let ctx = Context::default();
    for _ in 0..2 {
        let _ = ctx.run(Default::default(), |ctx| {
            app.render_ui(ctx);
        });
    }

    assert!(
        !app.notices.is_empty(),
        "a notice stays until it is dismissed"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_app_resilience test_render_ui_shows_notices_without_panicking`
Expected: FAIL, solange `render_ui` das Band nicht zeichnet und die Meldungen damit nie dargestellt werden. Die Darstellung selbst wird in Step 6 von Hand bestätigt.

- [ ] **Step 3: Draw the notice bar at the top of `render_ui`**

Als eigenes `egui::TopBottomPanel::top("notice_bar")`, **vor** `top_menu_bar`, und nur wenn `!self.notices.is_empty()` — sonst nimmt ein leeres Band Höhe weg. Wegen der gleichzeitigen Entleihe von `self.notices` und `self.i18n` die Meldungen in eine lokale Variable nehmen, zeichnen, danach zurückschreiben; dasselbe Muster wie bei `menu_sidebar_action` im Bestand.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_app_resilience`
Expected: PASS

- [ ] **Step 5: Run the whole suite and the lints**

Run: `cargo test && cargo fmt --check && cargo clippy --all-targets`
Expected: alle Tests grün, keine Beanstandung

- [ ] **Step 6: Manual check**

Run: `cargo run`
Expected: Start ohne Band. Danach `vms.json` im Konfigurationsverzeichnis mutwillig beschädigen und neu starten: Band mit rotem Hinweis und Quarantäne-Pfad erscheint, die beschädigte Datei liegt zeitgestempelt daneben, und Abweisen lässt das Band verschwinden.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs tests/test_app_resilience.rs
git commit -m "feat(app): show configuration notices above the menu bar"
```

---

## Abschluss von Teil 1

- [ ] `cargo test` — die 86 Bestandstests plus die neuen laufen grün
- [ ] `cargo fmt --check` ohne Beanstandung (schließt `build.rs:10` aus Spec §8 mit ein, falls noch offen)
- [ ] `cargo clippy --all-targets` ohne Warnung
- [ ] Der in Spec §3.4 bewiesene Datenverlust ist durch `test_corrupt_vms_json_is_preserved_not_overwritten` abgedeckt und tritt nicht mehr ein
