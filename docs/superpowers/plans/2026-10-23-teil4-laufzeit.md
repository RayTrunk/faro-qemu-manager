# Teil 4 — Laufzeitverhalten: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Der Stop-Knopf fährt den Gast herunter statt ihn zu töten, laufende VMs überleben das Schließen der App und sind beim nächsten Start wieder bedienbar — mit ihrer Konsole.

**Architecture:** `src/qmp.rs` spricht QMP über alles, was `Read + Write` ist; die echte Verbindung ist ein `TcpStream` auf `127.0.0.1`, die Tests reichen eine Leitung im Speicher herein, und ein Test startet ein echtes QEMU. `src/runstate.rs` hält je laufender VM Prozess-ID, QMP-Port, Startzeit und Protokollpfad in einer **eigenen** Datei neben `vms.json`, weil das flüchtige Angaben sind und `vms.json` die vom Benutzer gepflegte Wahrheit bleibt. Die Wiederanbindung bestätigt die Identität über einen QMP-Handschlag, nicht über die Prozess-ID — die wird wiederverwendet.

**Tech Stack:** Rust 2021, `eframe`/`egui` 0.29.1, `serde`/`serde_json`, `uuid`. **Keine neuen Abhängigkeiten** — QMP ist zeilenweises JSON über TCP, und beides liegt vor.

**Spec:** `docs/superpowers/specs/2026-10-23-wizard-und-haertung-design.md`, §7, mit §3.5

## Global Constraints

- **Ein Knopf, der „Herunterfahren" sagt, zieht nie den Stecker** (Spec §7.1, §1). Lässt sich kein QMP-Zugang herstellen, sagt die Oberfläche das und bietet den harten Weg **ausdrücklich** an — sie fällt nicht still darauf zurück.
- **Hartes Töten bleibt erreichbar, als zweiter eigener Schritt** (Spec §7.1, Entscheidung des Auftraggebers).
- **Beim Beenden der App wird keine VM getötet** (Spec §7.3). Die Konfiguration darf gespeichert werden, Gäste nicht umgebracht.
- **Eine nackte „läuft diese PID noch?"-Prüfung genügt nicht** (Spec §7.3). Prozess-IDs werden wiederverwendet; ohne Handschlag hängt die App sich an einen fremden Prozess und tötet ihn auf Knopfdruck.
- **Was sich nicht bestätigen lässt, gilt als beendet und wird aufgeräumt** (Spec §7.3).
- **Der Laufzeitzustand gehört nicht in `vms.json`** (Spec §7.3).
- **Nichts Teures pro Bild** (Spec §7.2, §3.5).
- Neue Benutzertexte in alle drei Kataloge `assets/locales/{en,de,fr}.json`.
- Code und Kommentare auf Englisch. `cargo fmt --check` und `cargo clippy --all-targets` ohne Beanstandung. Die Tests aus Teilen 1–3b bleiben grün.

## Am 2026-10-23 gegen QEMU 11.1.0 ausgeführte Tatsachen

Ausgeführt, aus derselben Shell, in der `cargo test` läuft. Der Umsetzer kann es als gesichert behandeln.

- **`-qmp tcp:127.0.0.1:<port>,server=on,wait=off` wird angenommen.**
- **Der Server nimmt nacheinander mehrere Clients an.** Drei Verbindungen hintereinander, jede mit vollständiger Begrüßung und `{"return": {}}` auf `qmp_capabilities`. **Darauf beruht die ganze Wiederanbindung.**
- Die Begrüßung lautet `{"QMP": {"version": {"qemu": {"micro": 0, "minor": 1, "major": 11}, "package": "v11.1.0-12130-ge470268ff4"}, "capabilities": ["oob"]}}` — **das ist der Identitätsnachweis** aus §7.3.
- Vor jedem Befehl muss `{"execute":"qmp_capabilities"}` gehen; Antworten sind **ein JSON-Objekt je Zeile**.
- `{"execute":"system_powerdown"}` wird angenommen und liefert `{"timestamp": …, "event": "POWERDOWN"}`.
- **Eine benannte Windows-Pipe (`-qmp pipe:<name>`) taugt nicht:** QEMU nimmt dort genau **einen** Client an; die zweite Verbindung läuft in einen Zeitablauf. Damit ist sie für die Wiederanbindung unbrauchbar. TCP auf `127.0.0.1` ist der Transport.
- **Einen festen Port darf die App nicht wählen.** Auf diesem Wirt scheitert das Belegen von 45555–45561 und 47000, während 12345, 52400 und 55000 gehen; Windows reserviert zudem ganze Bereiche (`netsh int ipv4 show excludedportrange protocol=tcp` zeigt hier zwölf). Die App muss einen freien Port **ermitteln** (`TcpListener` auf `:0` binden, Port ablesen, freigeben) und ihn in der Zustandsdatei vermerken.
- `std::net::TcpListener::bind("127.0.0.1:0")` gelingt in dieser Umgebung. Die Tests dürfen also wirklich verbinden.

## Bestehende Stellen, die dieser Teil anfasst

- `src/app.rs`, `impl Drop for FaroQemApp` — tötet heute jede laufende VM.
- `src/app.rs:813` — `find_qemu_img` steht **im Bildaufbau** der CentralPanel.
- `src/ui/settings.rs:206` — `find_qemu_binary` läuft pro Bild, solange der Dialog offen ist.
- `src/ui/vm_detail.rs:1452` — `find_qemu_img(None)` als Rückfall, pro Bild.
- `src/app.rs`, `process_qemu_events` — beim Verschwinden einer VM aus `running_processes` gehen die letzten Ausgaben und das Beendigungsereignis verloren.
- `src/qemu.rs`, `VmProcess::kill` — ruft `taskkill` und lässt dessen Ausgabe auf die Konsole des Prozesses laufen.

---

## Review Focus

1. **Eine vermerkte Prozess-ID gehört inzwischen einem fremden Programm** — Prozess-IDs werden wiederverwendet. Die App darf diesen Prozess niemals als ihre VM führen und schon gar nicht töten. (Aufgabe 6)
2. **Der Gast hängt und fährt nicht herunter** — der Wartezustand darf nicht unbegrenzt stehen bleiben; nach einer Frist steht der harte Weg sichtbar daneben. (Aufgabe 2)
3. **Die Zustandsdatei ist beschädigt oder von Hand bearbeitet** — die App startet trotzdem, führt keine Geister-VM, und wirft die Datei nicht über eine gültige. (Aufgabe 3)
4. **Zwei Instanzen der App laufen gleichzeitig** — beide schreiben dieselbe Zustandsdatei. Keine darf die Einträge der anderen verlieren. (Aufgabe 3)
5. **Das Protokoll einer lange laufenden VM wächst unbegrenzt** — eine Datei, die den Datenträger füllt, ist ein Schaden, kein Merkmal. (Aufgabe 5)

---

## Dateistruktur

| Datei | Aufgabe |
|---|---|
| `src/qmp.rs` (neu) | QMP über `Read + Write`, Begrüßung, Handschlag, Befehle, freier Port |
| `src/runstate.rs` (neu) | Zustandsdatei: lesen, einen Eintrag setzen, einen entfernen |
| `src/qemu.rs` (ändern) | `-qmp`-Argumente, Protokolldatei neben den Rohren |
| `src/app.rs` (ändern) | Herunterfahren, harter Weg, kein Töten beim Beenden, Wiederanbindung, Arbeit pro Bild |
| `src/ui/vm_detail/*` (ändern) | Wartezustand, zweistufiger Stop, keine Suche pro Bild |
| `src/ui/console.rs` (ändern) | Nur sichtbare Zeilen auslegen, Filter nicht pro Zeile neu kleinschreiben |

---

### Task 1: QMP sprechen

**Files:**
- Create: `src/qmp.rs`, `tests/test_qmp.rs`
- Modify: `src/lib.rs`, `tests/common/mod.rs`

**Interfaces:**
- Consumes: nichts.
- Produces:
  - `pub struct Greeting { pub major: u32, pub minor: u32, pub micro: u32, pub package: String }`
  - `pub struct QmpClient<T: Read + Write> { .. }`
  - `impl<T: Read + Write> QmpClient<T> { pub fn handshake(transport: T) -> Result<(Self, Greeting), String>; pub fn execute(&mut self, command: &str) -> Result<serde_json::Value, String>; pub fn powerdown(&mut self) -> Result<(), String> }`
  - `pub fn connect_tcp(port: u16, timeout: Duration) -> Result<(QmpClient<TcpStream>, Greeting), String>`
  - `pub fn free_local_port() -> Result<u16, String>`
  - `pub fn qmp_args(port: u16) -> Vec<String>`

`handshake` liest die Begrüßungszeile, schickt `qmp_capabilities`, liest die Antwort und gibt erst dann einen brauchbaren Client heraus. Alles andere ist ein Fehler — eine halb geöffnete Verbindung darf nie als Bestätigung gelten.

- [ ] **Step 1: Write the failing tests**

```rust
/// A transport that replays canned lines and records what was written.
struct Wire { to_read: std::io::Cursor<Vec<u8>>, written: Vec<u8> }

const GREETING: &str = r#"{"QMP": {"version": {"qemu": {"micro": 0, "minor": 1, "major": 11}, "package": "v11.1.0-12130-ge470268ff4"}, "capabilities": ["oob"]}}"#;

#[test]
fn test_the_handshake_reads_the_greeting_and_sends_capabilities() {
    let wire = Wire::replaying(&[GREETING, r#"{"return": {}}"#]);
    let (client, greeting) = QmpClient::handshake(wire).unwrap();
    assert_eq!((greeting.major, greeting.minor, greeting.micro), (11, 1, 0));
    assert!(greeting.package.starts_with("v11.1.0"));
    assert!(
        client.wrote().contains("qmp_capabilities"),
        "no command is accepted before capabilities, got: {}", client.wrote()
    );
}

#[test]
fn test_a_transport_that_says_nothing_is_not_a_confirmed_vm() {
    assert!(
        QmpClient::handshake(Wire::replaying(&[])).is_err(),
        "silence must not pass for a handshake"
    );
}

#[test]
fn test_rubbish_instead_of_a_greeting_is_refused() {
    // A foreign program holding the port must not be mistaken for QEMU.
    // This is Review Focus 1 at the protocol level.
    for junk in ["HTTP/1.1 200 OK", "{}", "{\"QMP\": {}}", "not json at all"] {
        assert!(
            QmpClient::handshake(Wire::replaying(&[junk, r#"{"return": {}}"#])).is_err(),
            "accepted {:?} as a QEMU greeting", junk
        );
    }
}

#[test]
fn test_capabilities_refused_means_no_client() {
    let wire = Wire::replaying(&[GREETING, r#"{"error": {"class": "GenericError", "desc": "no"}}"#]);
    assert!(QmpClient::handshake(wire).is_err());
}

#[test]
fn test_a_command_returns_its_value_and_an_error_is_an_error() {
    let wire = Wire::replaying(&[
        GREETING,
        r#"{"return": {}}"#,
        r#"{"return": {"status": "running", "running": true}}"#,
        r#"{"error": {"class": "CommandNotFound", "desc": "no such command"}}"#,
    ]);
    let (mut client, _) = QmpClient::handshake(wire).unwrap();
    assert_eq!(client.execute("query-status").unwrap()["status"], "running");
    let err = client.execute("nonsense").unwrap_err();
    assert!(err.contains("no such command"), "pass the server's words along, got: {}", err);
}

#[test]
fn test_an_event_line_is_not_mistaken_for_a_reply() {
    // QEMU interleaves events with replies; powerdown answers with an event.
    let wire = Wire::replaying(&[
        GREETING,
        r#"{"return": {}}"#,
        r#"{"timestamp": {"seconds": 1, "microseconds": 2}, "event": "POWERDOWN"}"#,
        r#"{"return": {}}"#,
    ]);
    let (mut client, _) = QmpClient::handshake(wire).unwrap();
    assert!(client.powerdown().is_ok());
}

#[test]
fn test_a_free_port_is_actually_free() {
    let port = free_local_port().unwrap();
    assert!(port >= 1024);
    // Binding it right after must succeed: the probe has to let go.
    std::net::TcpListener::bind(("127.0.0.1", port)).expect("the discovered port must be usable");
}

#[test]
fn test_a_port_already_in_use_is_not_offered() {
    let a = free_local_port().unwrap();
    let _hold = std::net::TcpListener::bind(("127.0.0.1", a)).unwrap();
    assert_ne!(free_local_port().unwrap(), a);
}

#[test]
fn test_the_qmp_arguments_are_the_form_qemu_accepts() {
    assert_eq!(
        qmp_args(12345),
        vec!["-qmp".to_string(), "tcp:127.0.0.1:12345,server=on,wait=off".to_string()]
    );
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_qmp`
Expected: FAIL — `src/qmp.rs` existiert nicht.

- [ ] **Step 3: Implement**

Zeilenweises Lesen über `BufReader`, Schreiben mit abschließendem `\n`. `execute` verwirft Zeilen mit einem `event`-Feld, bis eine mit `return` oder `error` kommt. `connect_tcp` setzt Lese- und Schreibfristen, damit ein stummer Gegenüber nicht hängt. `free_local_port` bindet `127.0.0.1:0`, liest den Port und lässt den Lauscher fallen.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test --test test_qmp && cargo test`
Expected: PASS.

- [ ] **Step 5: Add the real end-to-end test**

In `tests/test_qmp.rs`, mit dem Überspringen-Muster der Annahme-Tests:

```rust
#[test]
fn test_a_real_qemu_answers_three_handshakes_in_a_row() {
    // The reattach of Task 6 stands or falls on this: QEMU's QMP server must
    // take a new client after the previous one is gone.
    let Some(bin) = find_qemu_binary(None, Arch::X86_64) else {
        eprintln!("skipped: no qemu-system-x86_64");
        return;
    };
    let port = free_local_port().unwrap();
    let mut child = common::spawn_paused_qemu(&bin, &qmp_args(port));

    for round in 1..=3 {
        let (mut client, greeting) = connect_tcp(port, Duration::from_secs(5))
            .unwrap_or_else(|e| panic!("round {}: {}", round, e));
        assert_eq!(greeting.major, 11, "round {}", round);
        assert_eq!(client.execute("query-status").unwrap()["running"], false);
        drop(client);
    }

    let (mut client, _) = connect_tcp(port, Duration::from_secs(5)).unwrap();
    client.powerdown().unwrap();
    let _ = child.kill();
}
```

`spawn_paused_qemu` ist eine Hilfe in `tests/common/mod.rs`, die QEMU mit `-machine q35 -m 128M -S -display none` plus den übergebenen Argumenten startet.

- [ ] **Step 6: Run it**

Run: `cargo test --test test_qmp`
Expected: PASS. Scheitert die zweite Runde, ist der Transport falsch gewählt und Aufgabe 6 nicht baubar — dann zurück zur Transportfrage, nicht den Test aufweichen.

- [ ] **Step 7: Commit**

```bash
git add src/qmp.rs src/lib.rs tests/test_qmp.rs tests/common/mod.rs
git commit -m "feat(qmp): speak QMP, and prove QEMU takes a second client"
```

---

### Task 2: Herunterfahren statt Töten

**Files:**
- Modify: `src/qemu.rs`, `src/app.rs`, `src/ui/sidebar.rs`, `src/ui/vm_detail/mod.rs`, `assets/locales/*`
- Test: `tests/test_app.rs`, `tests/test_vm_detail.rs`

**Interfaces:**
- Consumes: `qmp_args`, `free_local_port`, `connect_tcp`, `QmpClient::powerdown`.
- Produces:
  - `VmStatus::ShuttingDown { pid: u32, since: SystemTime }` (neue Spielart des vorhandenen Enums)
  - `VmDetailAction::Shutdown(Uuid)` und `VmDetailAction::ForceStop(Uuid)` — `Stop` wird zu `Shutdown`, `ForceStop` ist neu
  - `FaroQemApp::shutdown_vm(&mut self, id: Uuid) -> Result<(), String>`
  - `FaroQemApp::force_stop_vm(&mut self, id: Uuid) -> Result<(), String>` (das heutige `stop_vm`, umbenannt)
  - `pub const SHUTDOWN_GRACE: Duration`
  - `FaroQemApp::is_attached(&self, id: Uuid) -> bool` und `FaroQemApp::drop_qmp_port(&mut self, id: Uuid)` — der laufende Prozess führt den Port ab dieser Aufgabe mit; Aufgabe 3 schreibt ihn dann zusätzlich auf die Platte.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_launching_puts_a_qmp_port_on_the_command_line() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = push_startable_vm(&mut app, dir.path());
    app.launch_vm(id, None).unwrap();
    let cli = app.cli_previews.get(&id).unwrap();
    assert!(cli.contains("-qmp tcp:127.0.0.1:"), "got: {}", cli);
    let _ = app.force_stop_vm(id);
}

#[test]
fn test_shutdown_asks_the_guest_and_shows_that_it_is_waiting() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = push_startable_vm(&mut app, dir.path());
    app.launch_vm(id, None).unwrap();

    app.shutdown_vm(id).expect("the guest is asked to power down");
    assert!(
        matches!(app.status_map.get(&id), Some(VmStatus::ShuttingDown { .. })),
        "got: {:?}", app.status_map.get(&id)
    );
    // A paused guest runs no OS, so nothing acts on the request: the process
    // stays, which is exactly the hanging-guest case of Review Focus 2.
    assert!(app.is_attached(id));
    let _ = app.force_stop_vm(id);
}

#[test]
fn test_a_vm_without_a_qmp_port_is_not_quietly_killed() {
    // Spec §7.1: a button that says shutdown never pulls the plug.
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = push_startable_vm(&mut app, dir.path());
    app.launch_vm(id, None).unwrap();
    app.drop_qmp_port(id);  // what an older state file looks like

    let err = app.shutdown_vm(id).expect_err("without QMP there is no graceful path");
    assert!(!err.is_empty(), "the reason has to reach the user");
    assert!(app.is_attached(id), "the guest must still be running");
    let _ = app.force_stop_vm(id);
}

#[test]
fn test_force_stop_is_the_second_step_and_it_works() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = push_startable_vm(&mut app, dir.path());
    app.launch_vm(id, None).unwrap();
    app.force_stop_vm(id).unwrap();
    assert!(matches!(app.status_map.get(&id), Some(VmStatus::Stopped)));
    assert!(!app.is_attached(id));
}

#[test]
fn test_the_panel_offers_shutdown_first_and_force_only_while_waiting() {
    let i18n = I18n::new("en");
    let drawn = render_detail_with_status(VmStatus::Running { pid: 1 }, true, &i18n);
    assert!(drawn.contains(&i18n.t("shutdown")), "drew:\n{}", drawn);
    assert!(!drawn.contains(&i18n.t("force_stop")), "the hard way is a second step, drew:\n{}", drawn);

    let since = SystemTime::now() - SHUTDOWN_GRACE - Duration::from_secs(1);
    let drawn = render_detail_with_status(VmStatus::ShuttingDown { pid: 1, since }, true, &i18n);
    assert!(drawn.contains(&i18n.t("shutting_down")), "drew:\n{}", drawn);
    assert!(drawn.contains(&i18n.t("force_stop")), "drew:\n{}", drawn);
}

#[test]
fn test_a_vm_that_cannot_shut_down_gracefully_says_so_in_the_panel() {
    let i18n = I18n::new("en");
    let drawn = render_detail_with_status(VmStatus::Running { pid: 1 }, false, &i18n);
    assert!(drawn.contains(&i18n.t("shutdown_unavailable")), "drew:\n{}", drawn);
    assert!(drawn.contains(&i18n.t("force_stop")), "and offers the hard way, drew:\n{}", drawn);
}
```

`push_startable_vm` ist eine Hilfe in `tests/common/mod.rs`, die eine VM mit leerer qcow2 und `display.vga_type = "none"` anlegt, damit kein Fenster erscheint, und sie in die Liste der App schiebt. `render_detail_with_status(status, qmp_available, i18n)` nimmt als zweites Argument, ob ein QMP-Zugang besteht.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_app --test test_vm_detail`
Expected: FAIL — `shutdown_vm`, `ShuttingDown`, `SHUTDOWN_GRACE` fehlen.

- [ ] **Step 3: Implement**

- `launch_vm` ermittelt vor dem Start einen freien Port und hängt `qmp_args(port)` an die Argumente.
- `shutdown_vm` verbindet, schickt `system_powerdown`, setzt `VmStatus::ShuttingDown { pid, since: SystemTime::now() }` und schreibt eine Konsolenzeile. Ohne Port oder bei fehlgeschlagener Verbindung **kein Töten**, sondern `Err` mit dem Grund.
- `force_stop_vm` ist das heutige `stop_vm`, umbenannt; der alte Name bleibt nicht zurück.
- Der Detailbereich zeigt bei `Running` nur „Herunterfahren"; bei `ShuttingDown` den Wartezustand mit der verstrichenen Zeit und — nach `SHUTDOWN_GRACE` — „Hart beenden"; ohne QMP-Zugang `shutdown_unavailable` samt „Hart beenden". `SHUTDOWN_GRACE` ist 20 Sekunden: lang genug, dass ein normal herunterfahrender Gast nicht zum harten Weg einlädt, kurz genug, dass niemand vor einem hängenden Gast ratlos wartet.
- Das `POWERDOWN`-Ereignis wird als Konsolenzeile gezeigt.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A src tests assets/locales
git commit -m "feat: the stop button shuts the guest down; killing is a second, named step"
```

---

### Task 3: Der Laufzeitzustand überdauert den Prozess

**Files:**
- Create: `src/runstate.rs`, `tests/test_runstate.rs`
- Modify: `src/lib.rs`, `src/app.rs`

**Interfaces:**
- Consumes: `write_config_file` aus Teil 1 (atomares Schreiben).
- Produces:
  - `pub struct RunRecord { pub vm_id: Uuid, pub pid: u32, pub qmp_port: u16, pub started_at: u64, pub log: PathBuf }`
  - `pub fn default_runstate_path() -> PathBuf` — `running.json` neben `vms.json`
  - `pub fn load(path: &Path) -> Vec<RunRecord>` — eine beschädigte Datei ergibt eine leere Liste, **nie** einen Abbruch
  - `pub fn put(path: &Path, record: RunRecord) -> Result<(), String>`
  - `pub fn remove(path: &Path, vm_id: Uuid) -> Result<(), String>`
  - `FaroQemApp.runstate_path: PathBuf`

`put` und `remove` lesen, ändern **nur den eigenen Eintrag** und schreiben atomar zurück — damit eine zweite Instanz der App die Einträge der ersten nicht verliert (Review Focus 4).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_a_record_survives_a_write_and_read() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    let rec = RunRecord { vm_id: Uuid::new_v4(), pid: 4242, qmp_port: 52400, started_at: 1_700_000_000, log: dir.path().join("vm.log") };
    put(&path, rec.clone()).unwrap();
    assert_eq!(load(&path), vec![rec]);
}

#[test]
fn test_the_runstate_file_is_not_vms_json() {
    // Spec §7.3: vms.json stays the truth the user maintains.
    assert_eq!(default_runstate_path().file_name().unwrap(), "running.json");
    assert_eq!(default_runstate_path().parent(), default_vms_path().parent());
}

#[test]
fn test_a_corrupt_file_yields_nothing_and_is_not_fatal() {
    // Review Focus 3.
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    for junk in ["", "not json", "{}", "[{\"pid\": \"not a number\"}]"] {
        std::fs::write(&path, junk).unwrap();
        assert!(load(&path).is_empty(), "{:?} produced ghosts", junk);
    }
}

#[test]
fn test_putting_one_entry_leaves_the_others_alone() {
    // Review Focus 4: two app instances share this file.
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    let a = RunRecord { vm_id: Uuid::new_v4(), pid: 1, qmp_port: 1001, started_at: 1, log: PathBuf::from("a.log") };
    let b = RunRecord { vm_id: Uuid::new_v4(), pid: 2, qmp_port: 1002, started_at: 2, log: PathBuf::from("b.log") };
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
    put(&path, RunRecord { vm_id: id, pid: 1, qmp_port: 1001, started_at: 1, log: PathBuf::from("x") }).unwrap();
    put(&path, RunRecord { vm_id: id, pid: 9, qmp_port: 1009, started_at: 9, log: PathBuf::from("x") }).unwrap();
    let loaded = load(&path);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].pid, 9);
}

#[test]
fn test_removing_an_entry_keeps_the_rest_and_a_missing_one_is_no_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("running.json");
    let keep = RunRecord { vm_id: Uuid::new_v4(), pid: 1, qmp_port: 1001, started_at: 1, log: PathBuf::from("k") };
    let drop_me = RunRecord { vm_id: Uuid::new_v4(), pid: 2, qmp_port: 1002, started_at: 2, log: PathBuf::from("d") };
    put(&path, keep.clone()).unwrap();
    put(&path, drop_me.clone()).unwrap();
    remove(&path, drop_me.vm_id).unwrap();
    assert_eq!(load(&path), vec![keep]);
    remove(&path, Uuid::new_v4()).unwrap();
}

#[test]
fn test_launching_and_force_stopping_add_and_drop_the_record() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = push_startable_vm(&mut app, dir.path());
    app.launch_vm(id, None).unwrap();
    assert_eq!(load(&app.runstate_path).len(), 1);
    app.force_stop_vm(id).unwrap();
    assert!(load(&app.runstate_path).is_empty(), "a stopped VM leaves no record");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_runstate`
Expected: FAIL — `src/runstate.rs` existiert nicht.

- [ ] **Step 3: Implement**

`RunRecord` leitet `Serialize`, `Deserialize`, `Clone`, `Debug`, `PartialEq` ab. `load` liest, parst und gibt bei jedem Fehler `Vec::new()` zurück. `put`/`remove` nutzen das atomare `write_config_file` aus Teil 1, damit ein Abbruch mitten im Schreiben keine halbe Liste hinterlässt. `FaroQemApp.runstate_path` wird in `with_paths` aus dem Verzeichnis von `vms_path` gebildet; `launch_vm` legt den Eintrag an, `force_stop_vm` und das Beendigungsereignis entfernen ihn.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/runstate.rs src/lib.rs src/app.rs tests/test_runstate.rs
git commit -m "feat(runstate): what is running, in its own file, written one entry at a time"
```

---

### Task 4: Beim Beenden wird nichts getötet

**Files:**
- Modify: `src/app.rs`, `assets/locales/*`
- Test: `tests/test_app.rs`, `tests/common/mod.rs`

**Interfaces:**
- Consumes: `runstate::{load, put}`.
- Produces: `impl Drop` ohne Tötung; `FaroQemApp::surviving_vm_names(&self) -> Vec<String>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_dropping_the_app_leaves_the_guest_running() {
    // Spec §7.3, the user's own decision. Today Drop kills every VM.
    let dir = tempdir().unwrap();
    let pid = {
        let mut app = app_with_vm_dir(dir.path());
        let id = push_startable_vm(&mut app, dir.path());
        app.launch_vm(id, None).unwrap();
        let pid = match app.status_map.get(&id) {
            Some(VmStatus::Running { pid }) => *pid,
            other => panic!("got {:?}", other),
        };
        drop(app);
        pid
    };
    std::thread::sleep(Duration::from_millis(800));
    assert!(common::pid_is_alive(pid), "the app closing is not a reason to kill a guest");
    let _ = common::kill_pid(pid);
}

#[test]
fn test_dropping_the_app_still_saves_the_configuration() {
    let dir = tempdir().unwrap();
    let vms_path = dir.path().join("vms.json");
    {
        let mut app = FaroQemApp::with_paths(dir.path().join("settings.json"), vms_path.clone(), None);
        app.vms.push(VmConfig::new_default("kept", Arch::X86_64));
    }
    assert!(try_load_vms(&vms_path).unwrap().iter().any(|v| v.name == "kept"));
}

#[test]
fn test_the_record_outlives_the_app() {
    let dir = tempdir().unwrap();
    let runstate = {
        let mut app = app_with_vm_dir(dir.path());
        let id = push_startable_vm(&mut app, dir.path());
        app.launch_vm(id, None).unwrap();
        let path = app.runstate_path.clone();
        drop(app);
        path
    };
    let records = load(&runstate);
    assert_eq!(records.len(), 1, "without this the VM cannot be found again");
    let _ = common::kill_pid(records[0].pid);
}

#[test]
fn test_closing_with_running_vms_names_them() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = push_startable_vm(&mut app, dir.path());
    app.launch_vm(id, None).unwrap();
    assert_eq!(app.surviving_vm_names().len(), 1, "say which guests keep running — without asking");
    let _ = app.force_stop_vm(id);
}
```

`pid_is_alive` und `kill_pid` sind Testhilfen in `tests/common/mod.rs`; `kill_pid` räumt auf, damit kein Gast den Testlauf überlebt.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_app`
Expected: FAIL beim ersten Test — `Drop` tötet noch.

- [ ] **Step 3: Implement**

Aus `Drop` fällt die Tötungsschleife; die Schreibsperren aus Teil 1 bleiben unangetastet. `running_processes` wird geleert, ohne `kill` zu rufen. Beim Schließen zeigt die App eine Meldung mit den Namen aus `surviving_vm_names()` — **ohne Rückfrage**, denn Weiterlaufen ist jetzt das gewollte Verhalten (Spec §7.3).

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A src tests assets/locales
git commit -m "feat: closing the window is not a reason to kill a guest"
```

---

### Task 5: Protokolldatei je VM

**Files:**
- Modify: `src/qemu.rs`, `src/app.rs`
- Test: `tests/test_process_supervisor.rs`, `tests/test_app.rs`

**Interfaces:**
- Consumes: `spawn_qemu`.
- Produces: `pub fn spawn_qemu(bin: &Path, args: &[String], tx: Sender<QemuEvent>, log: Option<&Path>) -> Result<VmProcess, String>` (erweiterte Signatur); `pub const LOG_MAX_BYTES: u64`.

Spec §7.3, Punkt 4: die Rohre gehören dem beendeten Elternprozess. Ohne Protokolldatei ist die Konsole einer wieder eingesammelten VM leer, und „wieder bedienbar" wäre nur die halbe Wahrheit.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_what_the_process_says_lands_in_the_log_file_too() {
    let dir = tempdir().unwrap();
    let log = dir.path().join("vm.log");
    let (tx, rx) = mpsc::channel();
    let mut proc = spawn_echoing_process(tx, Some(&log));
    drain_events(&rx);
    let _ = proc.kill();
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("HELLO FROM THE GUEST"), "got: {:?}", text);
}

#[test]
fn test_a_log_past_the_cap_is_rotated_once_not_endless() {
    // Review Focus 5: a file that fills the disk is damage, not a feature.
    let dir = tempdir().unwrap();
    let log = dir.path().join("vm.log");
    std::fs::write(&log, vec![b'x'; (LOG_MAX_BYTES + 1024) as usize]).unwrap();
    let (tx, _rx) = mpsc::channel();
    let mut proc = spawn_echoing_process(tx, Some(&log));
    std::thread::sleep(Duration::from_millis(300));
    let _ = proc.kill();
    assert!(
        std::fs::metadata(&log).unwrap().len() < LOG_MAX_BYTES,
        "the log must start over, not grow"
    );
    assert!(dir.path().join("vm.log.1").exists(), "the previous log is kept once");
}

#[test]
fn test_a_log_that_cannot_be_written_does_not_stop_the_vm() {
    let dir = tempdir().unwrap();
    let blocker = dir.path().join("as-a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let (tx, rx) = mpsc::channel();
    let mut proc = spawn_echoing_process(tx, Some(&blocker.join("vm.log")));
    assert!(!drain_events(&rx).is_empty(), "the guest matters more than its log");
    let _ = proc.kill();
}

#[test]
fn test_the_last_output_and_the_exit_are_not_lost_when_a_vm_stops() {
    // src/app.rs dropped both when the VM left running_processes.
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = push_startable_vm(&mut app, dir.path());
    app.launch_vm(id, None).unwrap();
    app.force_stop_vm(id).unwrap();
    for _ in 0..40 {
        app.process_qemu_events(None);
        std::thread::sleep(Duration::from_millis(25));
    }
    let said = app.console_logs.get(&id).map(|b| b.all_text()).unwrap_or_default();
    assert!(said.contains("[PROCESS]"), "the console must keep the ending, got:\n{}", said);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_process_supervisor --test test_app`
Expected: FAIL — `spawn_qemu` nimmt keinen Protokollpfad.

- [ ] **Step 3: Implement**

Die Leser-Fäden schreiben jede Zeile zusätzlich in die Protokolldatei (gepuffertes Anhängen). Beim Öffnen: ist die Datei größer als `LOG_MAX_BYTES` (8 MB), wird sie einmal nach `<name>.1` geschoben und neu begonnen — eine Vorgängerdatei, nicht mehr. Der Pfad ist `<VM-Ordner>/console.log`, bei einer VM ohne Ordner `<Konfigurationsverzeichnis>/logs/<uuid>.log`. Ein Fehler beim Protokollieren wird einmal als Konsolenzeile vermerkt und dann nicht wiederholt; die VM läuft weiter. `process_qemu_events` leert den Kanal **vollständig** und verarbeitet das `Exited`-Ereignis, bevor der Eintrag aus den laufenden Prozessen fällt.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A src tests
git commit -m "feat: keep a log per VM, and stop losing the last words of one that exits"
```

---

### Task 6: Wiederanbindung mit Identitätsprüfung

**Files:**
- Modify: `src/app.rs`, `assets/locales/*`
- Test: `tests/test_app.rs`

**Interfaces:**
- Consumes: `runstate::{load, remove}`, `connect_tcp`, `Greeting`, `ConsoleLogBuffer`.
- Produces: `FaroQemApp::reattach_running_vms(&mut self)`, aus `with_paths` gerufen; `enum Attached { Owned { proc: VmProcess, rx: Receiver<QemuEvent>, qmp_port: Option<u16> }, Reattached { pid: u32, qmp_port: u16 } }`.

Spec §7.3, Punkt 3: die Identität wird über einen erfolgreichen QMP-Handschlag auf dem **vermerkten** Zugang bestätigt. Was sich nicht bestätigen lässt, gilt als beendet und wird aufgeräumt.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_a_vm_that_outlived_the_app_is_picked_up_again() {
    let dir = tempdir().unwrap();
    let (settings, vms) = (dir.path().join("settings.json"), dir.path().join("vms.json"));
    let id = {
        let mut app = app_with_paths_and_vm_dir(&settings, &vms, dir.path());
        let id = push_startable_vm(&mut app, dir.path());
        app.launch_vm(id, None).unwrap();
        drop(app);
        id
    };

    let mut app = FaroQemApp::with_paths(settings, vms, None);
    assert!(
        matches!(app.status_map.get(&id), Some(VmStatus::Running { .. })),
        "got: {:?}", app.status_map.get(&id)
    );
    app.shutdown_vm(id).expect("a reattached VM takes a shutdown request");
    let _ = app.force_stop_vm(id);
}

#[test]
fn test_a_reattached_vm_has_a_console_again() {
    // Spec §7.3, point 4: the pipes belonged to the dead parent.
    let dir = tempdir().unwrap();
    let (settings, vms) = (dir.path().join("settings.json"), dir.path().join("vms.json"));
    let id = {
        let mut app = app_with_paths_and_vm_dir(&settings, &vms, dir.path());
        let id = push_startable_vm(&mut app, dir.path());
        app.launch_vm(id, None).unwrap();
        std::thread::sleep(Duration::from_millis(600));
        drop(app);
        id
    };

    let mut app = FaroQemApp::with_paths(settings, vms, None);
    let said = app.console_logs.get(&id).map(|b| b.all_text()).unwrap_or_default();
    assert!(!said.trim().is_empty(), "a reattached VM with an empty console is half the truth");
    let _ = app.force_stop_vm(id);
}

#[test]
fn test_a_recycled_pid_is_never_adopted() {
    // Review Focus 1 and Spec §7.3: PIDs are reused. This record points at a
    // live process that is not our VM and answers no QMP.
    let dir = tempdir().unwrap();
    let (settings, vms) = (dir.path().join("settings.json"), dir.path().join("vms.json"));
    let stranger = common::spawn_long_sleep();
    let vm = VmConfig::new_default("ghost", Arch::X86_64);
    let id = vm.id;
    save_vms(&vms, &[vm]).unwrap();
    let runstate = dir.path().join("running.json");
    put(&runstate, RunRecord {
        vm_id: id,
        pid: stranger.id(),
        qmp_port: free_local_port().unwrap(),  // nothing listens there
        started_at: 0,
        log: dir.path().join("ghost.log"),
    }).unwrap();

    let app = FaroQemApp::with_paths(settings, vms, None);
    assert!(
        !matches!(app.status_map.get(&id), Some(VmStatus::Running { .. })),
        "a live PID is not proof: got {:?}", app.status_map.get(&id)
    );
    assert!(load(&runstate).is_empty(), "an unconfirmable record is cleaned up");
    assert!(common::pid_is_alive(stranger.id()), "and the stranger is left alone");
    let _ = common::kill_pid(stranger.id());
}

#[test]
fn test_a_record_whose_process_is_gone_is_cleaned_up() {
    let dir = tempdir().unwrap();
    let (settings, vms) = (dir.path().join("settings.json"), dir.path().join("vms.json"));
    let vm = VmConfig::new_default("gone", Arch::X86_64);
    let id = vm.id;
    save_vms(&vms, &[vm]).unwrap();
    let runstate = dir.path().join("running.json");
    put(&runstate, RunRecord { vm_id: id, pid: 1, qmp_port: 1, started_at: 0, log: dir.path().join("x.log") }).unwrap();

    let app = FaroQemApp::with_paths(settings, vms, None);
    assert!(!matches!(app.status_map.get(&id), Some(VmStatus::Running { .. })));
    assert!(load(&runstate).is_empty());
}

#[test]
fn test_a_record_for_a_vm_that_no_longer_exists_is_dropped() {
    let dir = tempdir().unwrap();
    let runstate = dir.path().join("running.json");
    put(&runstate, RunRecord { vm_id: Uuid::new_v4(), pid: 1, qmp_port: 1, started_at: 0, log: PathBuf::from("x") }).unwrap();
    let _ = FaroQemApp::with_paths(dir.path().join("settings.json"), dir.path().join("vms.json"), None);
    assert!(load(&runstate).is_empty());
}
```

`spawn_long_sleep` ist eine Testhilfe, die irgendeinen langlebigen Prozess startet.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_app`
Expected: FAIL — `reattach_running_vms` existiert nicht.

- [ ] **Step 3: Implement**

`with_paths` ruft `reattach_running_vms` nach dem Laden der VM-Liste. Je Eintrag: gehört die `vm_id` zu einer bekannten VM? Antwortet `connect_tcp(port, kurze Frist)` mit einer gültigen Begrüßung? Nur dann gilt die VM als laufend — `status_map` auf `Running { pid }`, ein `Attached::Reattached`-Eintrag, und das Protokoll in die Konsole eingelesen, mit einer Zeile davor, die sagt, dass es aus der Datei kommt. Sonst: Eintrag über `remove` fallen lassen und **nichts töten**.

Eine wieder eingesammelte VM hat keinen `VmProcess` und keinen Ereigniskanal — darum die `Attached`-Spielart: `force_stop_vm` bedient beide Fälle, `process_qemu_events` nur `Owned`.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A src tests assets/locales
git commit -m "feat: pick up the VMs that outlived the app, by handshake not by PID"
```

---

### Task 7: Arbeit pro Bild beseitigen

**Files:**
- Modify: `src/app.rs`, `src/ui/settings.rs`, `src/ui/vm_detail/*`, `src/ui/console.rs`
- Test: `tests/test_app.rs`, `tests/test_console.rs`

**Interfaces:**
- Consumes: `refresh_capabilities` (aus Teil 2).
- Produces: `FaroQemApp.qemu_img: Option<PathBuf>` und `FaroQemApp.qemu_binary: Option<PathBuf>`, beide neben `capabilities` aufgelöst; `FaroQemApp::start_disk_creation(..)` und `poll_disk_creation()`; `ConsoleLogBuffer::visible_slice(first, count)` und `matching(needle)`.

Spec §7.2 und §3.5. Belegte Fundstellen: `src/app.rs:813`, `src/ui/settings.rs:206`, `src/ui/vm_detail.rs:1452`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_no_qemu_search_happens_while_drawing() {
    // The three call sites named in Spec §7.2 are gone from the draw path.
    for file in ["src/app.rs", "src/ui/settings.rs"] {
        let text = std::fs::read_to_string(file).unwrap();
        let drawing = text.split("fn update(").nth(1).unwrap_or("");
        assert!(!drawing.contains("find_qemu_img("), "{} searches while drawing", file);
        assert!(!drawing.contains("find_qemu_binary("), "{} searches while drawing", file);
    }
    let detail = common::concat_sources("src/ui/vm_detail");
    assert!(!detail.contains("find_qemu_img(None)"), "the per-frame fallback is still there");
}

#[test]
fn test_the_resolved_qemu_img_is_re_resolved_only_on_demand() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let before = app.qemu_img.clone();
    app.settings.qemu_dir = Some(dir.path().join("empty-on-purpose"));
    std::fs::create_dir_all(dir.path().join("empty-on-purpose")).unwrap();
    app.refresh_capabilities();
    assert_ne!(app.qemu_img, before, "changing the directory is what re-resolves it");
}

#[test]
fn test_creating_a_disk_does_not_block_the_caller() {
    // create_disk_image ran on the frame thread; a 64 GB image froze the UI.
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let target = dir.path().join("big.qcow2");
    let started = Instant::now();
    app.start_disk_creation(target.clone(), DriveFormat::Qcow2, 64);
    assert!(started.elapsed() < Duration::from_millis(250), "the call must return at once");
    for _ in 0..200 {
        app.poll_disk_creation();
        if target.is_file() { break; }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(target.is_file());
}

#[test]
fn test_the_console_lays_out_only_what_is_visible() {
    let mut buf = ConsoleLogBuffer::new();
    for n in 0..5000 {
        buf.push_stdout(format!("line {}", n));
    }
    let shown = buf.visible_slice(4900, 50);
    assert_eq!(shown.len(), 50, "a 5000-line buffer must not lay out 5000 lines per frame");
    assert!(shown[0].text.contains("4900"));
}

#[test]
fn test_the_filter_is_not_lowercased_per_line_per_frame() {
    let mut buf = ConsoleLogBuffer::new();
    for n in 0..2000 {
        buf.push_stdout(format!("Line {} WARNING", n));
    }
    let started = Instant::now();
    for _ in 0..20 {
        assert_eq!(buf.matching("warning").len(), 2000);
    }
    assert!(started.elapsed() < Duration::from_millis(400), "took {:?}", started.elapsed());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test`
Expected: FAIL — die Suchen stehen noch im Bildpfad, `start_disk_creation`, `visible_slice` und `matching` fehlen.

- [ ] **Step 3: Implement**

- `FaroQemApp` bekommt `qemu_img` und `qemu_binary`, beide in `with_paths` und `refresh_capabilities` gesetzt. Die drei Bildstellen lesen diese Felder.
- `start_disk_creation` schickt die Arbeit in einen Faden und berichtet über einen Kanal; `poll_disk_creation` leert ihn mit `try_recv`. Das Modal zeigt „läuft" statt zu blockieren.
- `ConsoleLogBuffer` speichert die kleingeschriebene Fassung jeder Zeile **beim Anlegen** des Eintrags; `matching` vergleicht gegen diese. Die Konsole nutzt `egui::ScrollArea::show_rows` und `visible_slice`.
- `common::concat_sources(dir)` ist eine Testhilfe, die jede `.rs`-Datei unter `dir` aneinanderhängt — dieselbe Aufgabe wie `sources_text` aus Teil 3a, Aufgabe 8, nur für einen Teilbaum. Beide gehören in `tests/common/mod.rs`, damit es nur eine gibt.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test && cargo fmt --all --check && cargo clippy --all-targets`
Expected: PASS, keine Beanstandung.

- [ ] **Step 5: Commit**

```bash
git add -A src tests
git commit -m "perf: nothing expensive happens while drawing a frame"
```

---

## Selbstprüfung

**Spec-Abdeckung:** §7.1 Herunterfahren, harter Weg als zweiter Schritt, kein stiller Rückfall → Aufgabe 2. §7.2 alle vier Punkte → Aufgabe 7, der vierte (verlorene Ausgaben) zusätzlich in Aufgabe 5. §7.3 Punkt 1 kein Töten → Aufgabe 4. Punkt 2 Zustand überdauert → Aufgabe 3. Punkt 3 Handschlag statt PID → Aufgabe 6. Punkt 4 Konsole überbrückt die Lücke → Aufgaben 5 und 6. Meldung beim Schließen ohne Rückfrage → Aufgabe 4. §3.5 Arbeit pro Bild → Aufgabe 7.

**Vom Umsetzer entschieden, wie die Spec es zulässt:** Transport ist TCP auf `127.0.0.1` mit ermitteltem Port — die benannte Pipe nimmt nur einen Client, ausgeführt und belegt. Zustandsdatei ist `running.json` neben `vms.json`. `SHUTDOWN_GRACE` ist 20 Sekunden. Das Protokoll liegt im VM-Ordner und wird bei 8 MB einmal gedreht. Weitere QMP-Fähigkeiten (Pause, Fortsetzen, Kennzahlen) sind nicht in diesem Umfang, aber `QmpClient::execute` verbaut sie nicht.

**Bewusst offen gelassen:** Gestalt des Wartezustands über die Pflichtangaben hinaus, Puffergröße der Protokollschreiber, ob die Meldung beim Schließen ein Band oder ein Dialog ist.

**Nicht in diesem Plan:** die Windows-11-Installation von Hand (Spec §10, beim Auftraggeber).
