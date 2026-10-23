# Teil 2 — Argumentbau und Firmware: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Jede in der Oberfläche wählbare Einstellung erzeugt eine Befehlszeile, die QEMU annimmt — und UEFI funktioniert so, dass Windows nach der Installation auch wieder bootet.

**Architecture:** `src/qemu.rs` bekommt eine Fähigkeitsabfrage (`QemuCapabilities`, einmal pro QEMU-Programm erhoben und vom Aufrufer gehalten) und einen Argumentbau, der Laufwerke über ausdrückliche Geräte statt über `if=` anbindet. `src/config.rs` bekommt ein `Firmware`-Enum, das BIOS und UEFI samt VM-eigener NVRAM-Datei ausdrücken kann und das alte `bios_uefi`-Feld weiterliest. Ein neues Test-Hilfsmodul `tests/common/mod.rs` startet QEMU mit den gebauten Argumenten und prüft, dass es sie annimmt — das ist der Test, der alle acht bekannten Fehler verhindert hätte.

**Tech Stack:** Rust 2021, `eframe`/`egui` 0.29.1, `serde`/`serde_json`, `shell-words`, `tempfile` (Tests). **Keine neuen Abhängigkeiten in diesem Teil.**

**Spec:** `docs/superpowers/specs/2026-10-23-wizard-und-haertung-design.md`, §5.1 und §5.2 (mit den Tatsachen aus §3.1 und §3.3)

## Global Constraints

- Rust 2021, `eframe`/`egui` 0.29.1 — kein Framework-Wechsel (Spec §2).
- Keine neuen Abhängigkeiten.
- **Freitextfelder bleiben Freitext** (`machine_type`, `cpu_model`, eigene NIC-Modelle) — sie sind der Fluchtweg für Fortgeschrittene und werden nicht in der Oberfläche beschnitten (Spec §5.1).
- **Nichts tut still etwas anderes, als es sagt** (Spec §1, Anti-Ziel). Eine Option, die das laufende QEMU nicht kann, wird ausgeblendet oder als ungeprüft gekennzeichnet — nicht stillschweigend ersetzt.
- **Altwerte werden nie still ersetzt** (Spec §11.6): `HAXM`, `bridge`, `if=nvme`, `if=sata`, `if=scsi` in gespeicherten Dateien bleiben stehen und werden gekennzeichnet; die Startprüfung aus Teil 3 blockiert später.
- Bestehende `vms.json` müssen weiter laden; altes `bios_uefi` gilt als „BIOS mit dieser Datei" (Spec §5.2).
- Die Fähigkeitsabfrage darf **nicht pro Bild** laufen (Spec §5.1, §7.2).
- **Kein Secure Boot** (Spec §11.2): die Windows-Vorlage nutzt `edk2-x86_64-code.fd` und heißt schlicht UEFI.
- Neue Benutzertexte in alle drei Kataloge `assets/locales/{en,de,fr}.json`.
- Code und Kommentare auf Englisch. `cargo fmt --check` und `cargo clippy --all-targets` ohne Beanstandung. Die 113 Tests aus Teil 1 bleiben grün; Zusicherungen in `tests/test_qemu_args.rs`, die §5.1 absichtlich korrigiert, werden auf die neue richtige Ausgabe gezogen.

## Gegen QEMU 11.1.0 ausgeführte Tatsachen

Alles hier wurde ausgeführt, nicht nachgelesen. Der Umsetzer kann es als gesichert behandeln.

**Abgelehnt — die Oberfläche bietet es heute an:**

| Angebot | QEMU-Antwort |
|---|---|
| `-drive …,if=sata` | `unsupported bus type 'sata'` |
| `-drive …,if=nvme` | `unsupported bus type 'nvme'` |
| `-drive …,if=scsi` auf q35 | `machine type does not support if=scsi,bus=0,unit=0` |
| zwei `-cdrom` | `drive with bus=2, unit=0 (index=2) exists` |
| `hostfwd=` an `-netdev tap` | `Invalid parameter 'hostfwd'` |
| `-netdev bridge` | `network backend 'bridge' is not compiled into this binary` |
| `-machine pc-i440fx` | `unsupported machine type: "pc-i440fx"` |
| `-device nvme` ohne `serial=` | `serial property not set` |

**Angenommen — die Ersatzkonstruktionen:**

```
SATA:  -device ahci,id=ahci0
       -drive if=none,id=d0,file=…,format=qcow2
       -device ide-hd,drive=d0,bus=ahci0.0        (bus=ahci0.1 für die zweite Platte)
NVMe:  -drive if=none,id=d0,file=…,format=qcow2
       -device nvme,drive=d0,serial=<pflicht>
SCSI:  -device virtio-scsi-pci,id=scsi0
       -drive if=none,id=d0,file=…,format=qcow2
       -device scsi-hd,drive=d0,bus=scsi0.0
ISOs:  -drive media=cdrom,index=2,file=a.iso
       -drive media=cdrom,index=3,file=b.iso      (mehrere gleichzeitig, auch neben Platten)
UEFI:  -drive if=pflash,format=raw,unit=0,readonly=on,file=<share>/edk2-x86_64-code.fd
       -drive if=pflash,format=raw,unit=1,file=<VM-eigene Kopie von edk2-i386-vars.fd>
```

**Weiterhin gültig und unangetastet:** `if=virtio`, `if=ide`, `bootindex` an `if=virtio`, Maschinen `q35` und `microvm`, alle NIC-Modelle (`virtio-net-pci`, `e1000`, `rtl8139`, `e1000e`, `vmxnet3`, `pcnet`), alle VGA-Typen (`virtio`, `std`, `qxl`, `cirrus`, `vmware`, `none`), alle Anzeige-Backends (`default`, `gtk`, `sdl`, `none`). Alle fünf `qemu-system-*`-Programme sind auf der Referenzmaschine vorhanden.

**Fähigkeitsabfragen:** `-machine help`, `-device help`, `-netdev help`, `-accel help`. `-netdev help` listet auf dieser Installation `socket stream dgram hubport tap user` — ohne `bridge`, was die Ausblendung begründet.

## Review Focus

Fünf Eingaben, die die Spec voraussetzt, ohne sie zu nennen. Jede bekommt ihren Test in der Aufgabe, die den Code besitzt:

1. **Drei Platten desselben Interfaces.** Der Controller darf nur **einmal** entstehen, Bus-Adressen und `id`s müssen eindeutig bleiben. → Aufgabe 3.
2. **Mischung aller Interfaces in einer VM** (virtio + sata + nvme + scsi + ide gleichzeitig). Zwei Controller, kein Namenskonflikt. → Aufgabe 3.
3. **Pfade mit Leerzeichen und Komma.** In `-drive file=C:\My VMs\d,1.qcow2` trennt das Komma für QEMU Optionen und zerreißt die Angabe. → Aufgabe 3.
4. **UEFI-VM ohne gefundene Firmware.** Die Dateien liegen nicht, wo erwartet: kein Absturz, und keine stillschweigend als BIOS gestartete VM. → Aufgabe 6.
5. **Zwei VMs mit UEFI.** Jede braucht ihre **eigene** NVRAM-Datei; teilen sie eine, überschreiben sich ihre Booteinträge. → Aufgabe 6.

---

## File Structure

- `tests/common/mod.rs` — **neu.** Das Annahme-Prüfwerkzeug: startet QEMU mit gebauten Argumenten und sagt, ob es sie annimmt. Eigene Datei, weil mehrere Testdateien es brauchen.
- `src/qemu.rs` — Fähigkeitsabfrage und Argumentbau. Wächst erheblich; `build_qemu_args` wird in Teilfunktionen je Abschnitt zerlegt.
- `src/config.rs` — `Firmware`-Enum, Migration von `bios_uefi`. `DriveInterface` behält alle fünf Varianten, weil sie jetzt alle richtig gebaut werden.
- `src/ui/vm_detail.rs` — Auswahllisten aus den Fähigkeiten statt aus festen Listen; Firmware-Bedienelemente.
- `assets/locales/{en,de,fr}.json` — Texte für Firmware und ungeprüfte Optionen.
- `tests/test_qemu_capabilities.rs` — **neu.** Fähigkeitsabfrage.
- `tests/test_qemu_args.rs` — bestehend, Zusicherungen werden gezogen.
- `tests/test_qemu_acceptance.rs` — **neu.** Die Tabelle über die gesamte Auswahlfläche.

---

## Task 1: Annahme-Prüfwerkzeug

Ohne dieses Werkzeug sind alle folgenden Aufgaben Behauptungen.

**Files:**
- Create: `tests/common/mod.rs`
- Test: `tests/test_qemu_acceptance.rs` (create, zunächst nur Selbsttest)

**Interfaces:**
- Consumes: `faro_qem::qemu::{build_qemu_args, create_disk_image, find_qemu_binary, find_qemu_img}`.
- Produces:
  - `pub enum Acceptance { Accepted, Rejected { first_line: String }, NoQemu }`
  - `pub fn qemu_accepts(arch: Arch, args: &[String]) -> Acceptance` — findet das passende Programm, startet es mit den Argumenten plus `-S -display none`, wartet bis zu zwei Sekunden, beendet es und meldet. Fehlt QEMU, `NoQemu`.
  - `pub fn assert_accepted(label: &str, arch: Arch, args: &[String])` — bricht mit Beschriftung und erster Fehlerzeile ab; bei `NoQemu` überspringt es still, dem Muster von `test_create_disk_image_with_system_qemu_img_if_present` folgend.
  - `pub fn make_disk_image(path: &Path, format: DriveFormat)` — legt über `qemu-img` ein 1-MB-Abbild im angegebenen Format an, damit Laufwerke auf etwas zeigen. Fehlt `qemu-img`, tut es nichts; der Annahme-Test überspringt dann ohnehin.
  - `pub fn make_empty_qcow2(path: &Path)` — Kurzform für `make_disk_image(path, DriveFormat::Qcow2)`.
  - `pub fn rendered_text(output: &egui::FullOutput) -> String` — aus `tests/test_app_resilience.rs` hierher gezogen, weil Aufgabe 7 ihn ebenfalls braucht. Die Zieldatei importiert ihn dann von hier.

- [ ] **Step 1: Write the failing self-test**

```rust
mod common;
use common::{qemu_accepts, Acceptance};
use faro_qem::config::Arch;

#[test]
fn test_the_probe_can_tell_good_arguments_from_bad() {
    match qemu_accepts(Arch::X86_64, &["-machine".into(), "q35".into()]) {
        Acceptance::NoQemu => eprintln!("skipped: no qemu-system-x86_64 on this machine"),
        Acceptance::Accepted => {}
        Acceptance::Rejected { first_line } => panic!("q35 must be accepted: {}", first_line),
    }

    // A bus type QEMU does not have: this is one of the eight bugs.
    match qemu_accepts(
        Arch::X86_64,
        &[
            "-machine".into(),
            "q35".into(),
            "-drive".into(),
            "if=sata,file=nonexistent.qcow2".into(),
        ],
    ) {
        Acceptance::NoQemu => {}
        Acceptance::Rejected { first_line } => assert!(
            first_line.contains("sata"),
            "the rejection must name the offending option, got: {}",
            first_line
        ),
        Acceptance::Accepted => panic!("if=sata must be rejected — the probe has no teeth"),
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --test test_qemu_acceptance`
Expected: FAIL — `file not found for module common` / unresolved import

- [ ] **Step 3: Implement `tests/common/mod.rs`**

Starten mit `Stdio::piped()` auf stderr und `CREATE_NO_WINDOW` unter Windows, wie `spawn_qemu` es tut. Warten in kurzen Schritten bis zwei Sekunden: beendet sich der Prozess mit Fehlercode, gilt das als `Rejected` samt erster stderr-Zeile; läuft er noch, gilt das als `Accepted` und er wird beendet. **Wichtig:** `-S` hält den Gast angehalten, damit kein Gastcode läuft; `-display none` verhindert ein Fenster. Beide gehören an das Ende der Argumente, damit sie nichts aus der Konfiguration überschreiben.

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test --test test_qemu_acceptance -- --nocapture`
Expected: PASS, und bei fehlendem QEMU eine Übersprungen-Meldung statt eines Fehlschlags

- [ ] **Step 5: Commit**

```bash
git add tests/common/mod.rs tests/test_qemu_acceptance.rs
git commit -m "test: add a probe that asks QEMU whether it accepts our arguments"
```

---

## Task 2: Fähigkeitsabfrage

**Files:**
- Modify: `src/qemu.rs`
- Test: `tests/test_qemu_capabilities.rs` (create)

**Interfaces:**
- Consumes: `find_qemu_binary`.
- Produces:
  - `pub struct QemuCapabilities { pub machines: Vec<String>, pub devices: Vec<String>, pub netdev_backends: Vec<String>, pub accelerators: Vec<String> }` — `Debug, Clone, Default`
  - `impl QemuCapabilities { pub fn has_device(&self, name: &str) -> bool; pub fn has_netdev(&self, name: &str) -> bool; pub fn has_machine(&self, name: &str) -> bool; pub fn has_accelerator(&self, name: &str) -> bool; pub fn was_probed(&self) -> bool }`
  - `pub fn probe_capabilities(binary: &Path) -> QemuCapabilities` — ruft `-machine help`, `-device help`, `-netdev help`, `-accel help` **je einmal** und liest die Namen heraus. Schlägt ein Aufruf fehl, bleibt die betreffende Liste leer.

  Eine leere Liste heißt **„nicht abgefragt", nicht „nicht vorhanden"** — die Oberfläche darf daraus nie auf Abwesenheit schließen (Spec §11.6: ohne QEMU werden Optionen als ungeprüft gekennzeichnet, nicht ausgeblendet). `has_*` gibt bei leerer Liste darum `true` zurück, und `was_probed` sagt, ob überhaupt etwas erhoben wurde.

- [ ] **Step 1: Write the failing tests**

```rust
use faro_qem::config::Arch;
use faro_qem::qemu::{find_qemu_binary, probe_capabilities, QemuCapabilities};

#[test]
fn test_capabilities_report_what_this_qemu_can_do() {
    let Some(bin) = find_qemu_binary(None, Arch::X86_64) else {
        eprintln!("skipped: no qemu-system-x86_64 on this machine");
        return;
    };
    let caps = probe_capabilities(&bin);

    assert!(caps.was_probed());
    assert!(caps.has_machine("q35"));
    assert!(caps.has_netdev("user"));
    assert!(caps.has_device("ahci"), "SATA needs the AHCI controller");
    assert!(caps.has_device("nvme"));
    assert!(caps.has_device("virtio-scsi-pci"), "SCSI needs this controller");
    assert!(
        !caps.has_machine("pc-i440fx"),
        "the bare i440fx name is not a machine type; only versioned ones are"
    );
}

#[test]
fn test_unprobed_capabilities_claim_nothing_is_missing() {
    let caps = QemuCapabilities::default();
    assert!(!caps.was_probed());
    assert!(
        caps.has_device("ahci") && caps.has_netdev("bridge"),
        "without a probe we must not claim an option is unavailable"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test test_qemu_capabilities`
Expected: FAIL — unresolved imports `probe_capabilities`, `QemuCapabilities`

- [ ] **Step 3: Implement `QemuCapabilities` and `probe_capabilities` in `src/qemu.rs`**

Die vier Hilfeausgaben haben unterschiedliche Formen: `-device help` gruppiert nach Bus und schreibt `name "ahci", bus PCI, desc "…"`; `-machine help` schreibt Name und Beschreibung, `-netdev help` nur Namen. Je Abfrage ein kleiner Auswerter; ein Name in Anführungszeichen gewinnt, sonst das erste Wort der Zeile. Kopfzeilen wie `Available netdev backend types:` enthalten keinen Namen und fallen weg.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_qemu_capabilities`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/qemu.rs tests/test_qemu_capabilities.rs
git commit -m "feat(qemu): ask the installed QEMU what it can actually do"
```

---

## Task 3: Laufwerke über ausdrückliche Geräte

**Files:**
- Modify: `src/qemu.rs` (`build_qemu_args`, Abschnitt 4)
- Test: `tests/test_qemu_args.rs`, `tests/test_qemu_acceptance.rs`

**Interfaces:**
- Consumes: `DriveConfig`, `DriveInterface`, `DriveFormat`.
- Produces: `pub fn build_drive_args(drives: &[DriveConfig]) -> Vec<String>` — öffentlich, damit die Tabelle in Aufgabe 8 sie einzeln prüfen kann. `build_qemu_args` ruft sie.

  Regeln: `VirtIO` und `Ide` behalten den kurzen `if=`-Weg (geprüft gültig). `Sata`, `Nvme`, `Scsi` werden als `if=none,id=faro-drive-<n>` angelegt und mit einem Gerät verbunden. Controller entstehen **höchstens einmal je Art** und nur, wenn ein Laufwerk sie braucht: `-device ahci,id=faro-ahci` bzw. `-device virtio-scsi-pci,id=faro-scsi`. Bus-Adressen zählen je Controller von 0 hoch. `nvme` bekommt `serial=faro-<n>`, weil QEMU es ohne verweigert.

- [ ] **Step 1: Write the failing tests**

```rust
use faro_qem::qemu::build_drive_args;

fn drive(path: &str, iface: DriveInterface) -> DriveConfig {
    DriveConfig {
        path: PathBuf::from(path),
        format: DriveFormat::Qcow2,
        interface: iface,
        boot_index: None,
    }
}

#[test]
fn test_sata_drive_uses_an_ahci_controller() {
    let args = build_drive_args(&[drive("d.qcow2", DriveInterface::Sata)]).join(" ");
    assert!(args.contains("-device ahci,id=faro-ahci"));
    assert!(args.contains("if=none,id=faro-drive-0"));
    assert!(args.contains("-device ide-hd,drive=faro-drive-0,bus=faro-ahci.0"));
    assert!(!args.contains("if=sata"), "if=sata is not a bus type QEMU has");
}

#[test]
fn test_nvme_drive_gets_the_serial_qemu_demands() {
    let args = build_drive_args(&[drive("d.qcow2", DriveInterface::Nvme)]).join(" ");
    assert!(args.contains("-device nvme,drive=faro-drive-0,serial="));
    assert!(!args.contains("if=nvme"));
}

#[test]
fn test_scsi_drive_uses_a_virtio_scsi_controller() {
    let args = build_drive_args(&[drive("d.qcow2", DriveInterface::Scsi)]).join(" ");
    assert!(args.contains("-device virtio-scsi-pci,id=faro-scsi"));
    assert!(args.contains("-device scsi-hd,drive=faro-drive-0,bus=faro-scsi.0"));
}

#[test]
fn test_virtio_and_ide_keep_the_short_form() {
    let args = build_drive_args(&[
        drive("a.qcow2", DriveInterface::VirtIO),
        drive("b.qcow2", DriveInterface::Ide),
    ])
    .join(" ");
    assert!(args.contains("if=virtio"));
    assert!(args.contains("if=ide"));
    assert!(!args.contains("faro-ahci"), "no controller is needed for these");
}

// Review Focus 1
#[test]
fn test_three_sata_drives_share_one_controller_with_distinct_buses() {
    let args = build_drive_args(&[
        drive("a.qcow2", DriveInterface::Sata),
        drive("b.qcow2", DriveInterface::Sata),
        drive("c.qcow2", DriveInterface::Sata),
    ])
    .join(" ");
    assert_eq!(args.matches("-device ahci,id=faro-ahci").count(), 1);
    for bus in ["faro-ahci.0", "faro-ahci.1", "faro-ahci.2"] {
        assert!(args.contains(bus), "missing {} in {}", bus, args);
    }
}

// Review Focus 2
#[test]
fn test_all_five_interfaces_in_one_vm_do_not_collide() {
    let args = build_drive_args(&[
        drive("a.qcow2", DriveInterface::VirtIO),
        drive("b.qcow2", DriveInterface::Sata),
        drive("c.qcow2", DriveInterface::Nvme),
        drive("d.qcow2", DriveInterface::Scsi),
        drive("e.qcow2", DriveInterface::Ide),
    ]);
    let joined = args.join(" ");
    assert_eq!(joined.matches("-device ahci,").count(), 1);
    assert_eq!(joined.matches("-device virtio-scsi-pci,").count(), 1);

    let ids: Vec<&String> = args.iter().filter(|a| a.contains("id=faro-drive-")).collect();
    let unique: std::collections::HashSet<&&String> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "drive ids must be unique: {:?}", ids);
}

// Review Focus 3
#[test]
fn test_a_path_with_a_comma_does_not_split_the_drive_option() {
    let args = build_drive_args(&[drive(r"C:\My VMs\disk,1.qcow2", DriveInterface::VirtIO)]);
    let spec = args
        .iter()
        .find(|a| a.contains("disk"))
        .expect("the drive spec is there");
    assert!(
        spec.contains("disk,,1.qcow2"),
        "a comma in a path must be escaped by doubling it, got: {}",
        spec
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test test_qemu_args`
Expected: FAIL — `build_drive_args` not found; once it exists, the `Sata`/`Nvme`/`Scsi`/comma tests fail against today's `if=`-only construction

- [ ] **Step 3: Implement `build_drive_args` in `src/qemu.rs` and call it from `build_qemu_args`**

Kommas in Pfaden werden durch Verdoppeln geschützt — das ist QEMUs eigene Fluchtregel für Optionswerte. Das betrifft `file=` bei Laufwerken, CD-ROMs und pflash gleichermaßen, gehört also in eine gemeinsame kleine Hilfe.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_qemu_args`
Expected: PASS

- [ ] **Step 5: Add acceptance tests for all five interfaces**

```rust
#[test]
fn test_qemu_accepts_every_drive_interface() {
    let dir = tempfile::tempdir().unwrap();
    for iface in DriveInterface::all() {
        let disk = dir.path().join(format!("{}.qcow2", iface.as_str()));
        common::make_empty_qcow2(&disk);
        let mut vm = VmConfig::new_default("probe", Arch::X86_64);
        vm.drives.push(DriveConfig {
            path: disk,
            format: DriveFormat::Qcow2,
            interface: *iface,
            boot_index: None,
        });
        common::assert_accepted(iface.as_str(), Arch::X86_64, &build_qemu_args(&vm));
    }
}
```

- [ ] **Step 6: Run the acceptance tests**

Run: `cargo test --test test_qemu_acceptance -- --nocapture`
Expected: PASS für alle fünf. Schlägt eines fehl, nennt die Meldung das Interface und QEMUs erste Fehlerzeile.

- [ ] **Step 7: Commit**

```bash
git add src/qemu.rs tests/test_qemu_args.rs tests/test_qemu_acceptance.rs tests/common/mod.rs
git commit -m "fix(qemu): attach SATA, NVMe and SCSI drives through real devices"
```

---

## Task 4: Mehrere ISOs

**Files:**
- Modify: `src/qemu.rs` (Abschnitt 5)
- Test: `tests/test_qemu_args.rs`, `tests/test_qemu_acceptance.rs`

**Interfaces:**
- Produces: `pub fn build_cdrom_args(cdroms: &[PathBuf]) -> Vec<String>` — je ISO ein `-drive media=cdrom,index=<n>,file=<pfad>`. Der Index beginnt bei 2, damit er nicht mit den automatisch belegten Plätzen 0 und 1 kollidiert.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_two_isos_become_two_cdrom_drives() {
    let args = build_cdrom_args(&[PathBuf::from("a.iso"), PathBuf::from("b.iso")]).join(" ");
    assert_eq!(args.matches("media=cdrom").count(), 2);
    assert!(args.contains("index=2"));
    assert!(args.contains("index=3"));
    assert!(
        !args.contains("-cdrom"),
        "two -cdrom flags make QEMU refuse to start"
    );
}

#[test]
fn test_empty_iso_paths_are_skipped() {
    assert!(build_cdrom_args(&[PathBuf::new()]).is_empty());
}
```

Dazu ein Annahme-Test: eine VM mit zwei ISOs **und** einer Platte wird von QEMU angenommen.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test test_qemu_args`
Expected: FAIL — `build_cdrom_args` not found; today's code emits `-cdrom` twice

- [ ] **Step 3: Implement `build_cdrom_args` and call it**

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_qemu_args && cargo test --test test_qemu_acceptance`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/qemu.rs tests/test_qemu_args.rs tests/test_qemu_acceptance.rs
git commit -m "fix(qemu): allow more than one ISO to be attached at once"
```

---

## Task 5: Netzwerk

**Files:**
- Modify: `src/qemu.rs` (Abschnitt 8)
- Test: `tests/test_qemu_args.rs`, `tests/test_qemu_acceptance.rs`

**Interfaces:**
- Produces: `pub fn build_network_args(net: &NetworkConfig) -> Vec<String>` — Weiterleitungen nur beim `user`-Backend; die gespeicherten Regeln bleiben bei anderen Backends unangetastet, wandern aber nicht in die Befehlszeile.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_port_forwards_only_reach_the_user_backend() {
    let mut net = NetworkConfig::default();
    net.port_forwards.push(PortForwardRule {
        host_port: 2222,
        guest_port: 22,
        proto: "tcp".to_string(),
    });

    let as_user = build_network_args(&net).join(" ");
    assert!(as_user.contains("hostfwd=tcp::2222-:22"));

    net.backend = "tap".to_string();
    let as_tap = build_network_args(&net).join(" ");
    assert!(
        !as_tap.contains("hostfwd"),
        "hostfwd is a user-backend option; QEMU refuses it on tap"
    );
    assert!(as_tap.contains("-netdev tap,id=net0"));
}

#[test]
fn test_backend_none_disables_networking() {
    let net = NetworkConfig {
        backend: "none".to_string(),
        ..Default::default()
    };
    assert_eq!(build_network_args(&net), vec!["-nic", "none"]);
}
```

Dazu ein Annahme-Test: `user` mit zwei Weiterleitungen wird angenommen.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test test_qemu_args`
Expected: FAIL — `build_network_args` not found; today's code appends `hostfwd` to every backend

- [ ] **Step 3: Implement `build_network_args` and call it**

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test test_qemu_args`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/qemu.rs tests/test_qemu_args.rs tests/test_qemu_acceptance.rs
git commit -m "fix(qemu): send port forwards only to the backend that has them"
```

---

## Task 6: Firmware

Der Teil, an dem Windows 11 hängt.

**Files:**
- Modify: `src/config.rs` (`Firmware`, Migration), `src/qemu.rs` (pflash-Argumente, Firmware-Suche)
- Test: `tests/test_config.rs`, `tests/test_qemu_args.rs`, `tests/test_qemu_acceptance.rs`

**Interfaces:**
- Produces in `config`:
  - `pub enum Firmware { Bios { rom: Option<PathBuf> }, Uefi { nvram: PathBuf } }` — `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`, `#[serde(tag = "kind")]`, Standard ist `Bios { rom: None }`.
  - `VmConfig` bekommt `#[serde(default)] pub firmware: Firmware` und behält `bios_uefi` als `#[serde(default, skip_serializing)] pub bios_uefi: Option<PathBuf>`, damit alte Dateien laden.
  - `pub fn migrate_firmware(vm: &mut VmConfig)` — ist `bios_uefi` gesetzt und `firmware` noch Standard, wird daraus `Bios { rom: Some(pfad) }` und `bios_uefi` geleert. Wird beim Laden für jede VM aufgerufen.
- Produces in `qemu`:
  - `pub struct UefiFirmware { pub code: PathBuf, pub vars_template: PathBuf }`
  - `pub fn find_uefi_firmware(custom_dir: Option<&Path>) -> Option<UefiFirmware>` — sucht `edk2-x86_64-code.fd` und `edk2-i386-vars.fd` im `share`-Verzeichnis neben dem QEMU-Programm sowie in den üblichen Installationsorten. **Nicht** `edk2-x86_64-secure-code.fd`: ohne eingetragene Schlüssel und ohne `smm=on` wäre das ein Versprechen, das nicht gehalten wird (Spec §11.2).
  - `pub fn prepare_nvram(template: &Path, target: &Path) -> Result<(), String>` — legt die VM-eigene Kopie an, **nur wenn sie noch nicht existiert**; eine bestehende Datei enthält die Booteinträge des Gastes und darf nie überschrieben werden.
  - `pub fn build_firmware_args(fw: &Firmware, uefi: Option<&UefiFirmware>) -> Vec<String>`

- [ ] **Step 1: Write the failing tests**

```rust
// tests/test_config.rs
#[test]
fn test_old_bios_uefi_field_still_loads() {
    let json = r#"[{"id":"550e8400-e29b-41d4-a716-446655440000","name":"Old","arch":"x86_64",
                    "cpu_cores":2,"memory_mb":2048,"bios_uefi":"C:\\fw\\OVMF.fd"}]"#;
    let dir = tempdir().unwrap();
    let path = dir.path().join("vms.json");
    std::fs::write(&path, json).unwrap();

    let vms = try_load_vms(&path).expect("an old file still loads");
    assert_eq!(
        vms[0].firmware,
        Firmware::Bios {
            rom: Some(PathBuf::from(r"C:\fw\OVMF.fd"))
        },
        "an old bios_uefi path means BIOS with that ROM"
    );
}

#[test]
fn test_firmware_round_trips() {
    let fw = Firmware::Uefi {
        nvram: PathBuf::from("vm/nvram.fd"),
    };
    let json = serde_json::to_string(&fw).unwrap();
    assert_eq!(serde_json::from_str::<Firmware>(&json).unwrap(), fw);
}
```

```rust
// tests/test_qemu_args.rs
#[test]
fn test_uefi_emits_the_pflash_pair_with_a_per_vm_nvram() {
    let uefi = UefiFirmware {
        code: PathBuf::from("share/edk2-x86_64-code.fd"),
        vars_template: PathBuf::from("share/edk2-i386-vars.fd"),
    };
    let fw = Firmware::Uefi {
        nvram: PathBuf::from("vms/win11/nvram.fd"),
    };
    let args = build_firmware_args(&fw, Some(&uefi)).join(" ");

    assert!(args.contains("if=pflash,format=raw,unit=0,readonly=on"));
    assert!(args.contains("edk2-x86_64-code.fd"));
    assert!(args.contains("if=pflash,format=raw,unit=1"));
    assert!(args.contains("nvram.fd"));
    assert!(
        !args.contains("-bios"),
        "-bios loses the NVRAM, which is why Windows will not boot after install"
    );
    assert!(
        !args.contains("secure-code"),
        "we do not claim Secure Boot we cannot enforce"
    );
}

#[test]
fn test_bios_without_a_rom_adds_nothing() {
    assert!(build_firmware_args(&Firmware::Bios { rom: None }, None).is_empty());
}

#[test]
fn test_bios_with_a_rom_uses_minus_bios() {
    let args = build_firmware_args(
        &Firmware::Bios {
            rom: Some(PathBuf::from("my.rom")),
        },
        None,
    );
    assert_eq!(args, vec!["-bios", "my.rom"]);
}

// Review Focus 4
#[test]
fn test_uefi_without_discovered_firmware_emits_nothing_rather_than_falling_back() {
    let fw = Firmware::Uefi {
        nvram: PathBuf::from("n.fd"),
    };
    assert!(
        build_firmware_args(&fw, None).is_empty(),
        "a UEFI VM must not quietly start as BIOS; the start check reports the missing firmware"
    );
}
```

```rust
// tests/test_qemu_acceptance.rs — Review Focus 5
#[test]
fn test_two_uefi_vms_get_separate_nvram_files() {
    let dir = tempfile::tempdir().unwrap();
    let Some(uefi) = find_uefi_firmware(None) else {
        eprintln!("skipped: no edk2 firmware on this machine");
        return;
    };

    let mut built = Vec::new();
    for name in ["one", "two"] {
        let nvram = dir.path().join(name).join("nvram.fd");
        prepare_nvram(&uefi.vars_template, &nvram).expect("nvram copy");
        built.push(nvram);
    }
    assert_ne!(built[0], built[1]);
    assert!(built.iter().all(|p| p.is_file()));

    // A second call must not wipe the boot entries the guest wrote.
    std::fs::write(&built[0], b"GUEST BOOT ENTRIES").unwrap();
    prepare_nvram(&uefi.vars_template, &built[0]).unwrap();
    assert_eq!(
        std::fs::read(&built[0]).unwrap(),
        b"GUEST BOOT ENTRIES",
        "an existing NVRAM file is the guest's, not ours to replace"
    );
}
```

Dazu ein Annahme-Test: eine UEFI-VM mit vorbereitetem NVRAM wird von QEMU angenommen.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test test_config --test test_qemu_args --test test_qemu_acceptance`
Expected: FAIL — `Firmware`, `UefiFirmware`, `build_firmware_args`, `find_uefi_firmware`, `prepare_nvram` not found

- [ ] **Step 3: Implement firmware in `src/config.rs` and `src/qemu.rs`**

Die bestehende Zusicherung in `tests/test_qemu_args.rs`, die `-bios /path/to/bios.bin` erwartet, wird auf `Firmware::Bios { rom: Some(...) }` gezogen — die Ausgabe bleibt dieselbe, nur der Weg dorthin ist neu.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS, die 113 Tests aus Teil 1 eingeschlossen

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/qemu.rs tests/
git commit -m "feat(qemu): real UEFI via the pflash pair with a per-VM NVRAM copy"
```

---

## Task 7: Die Oberfläche bietet nur an, was geht

**Files:**
- Modify: `src/ui/vm_detail.rs` (Auswahllisten, Firmware-Bedienelemente), `src/app.rs` (Fähigkeiten halten)
- Modify: `assets/locales/{en,de,fr}.json`
- Test: `tests/test_vm_detail.rs`

**Interfaces:**
- Consumes: `QemuCapabilities` (Aufgabe 2), `Firmware` (Aufgabe 6).
- Produces: `FaroQemApp` bekommt `pub capabilities: QemuCapabilities`, **einmal** beim Start und nach einer Änderung des QEMU-Verzeichnisses erhoben — nicht pro Bild. `VmDetailView` bekommt `pub fn with_capabilities(self, caps: &'a QemuCapabilities) -> Self`.

  Regeln: `bridge` erscheint nur, wenn `has_netdev("bridge")`. `HAXM` erscheint unter Windows nicht. Maschinentyp-Vorschläge kommen aus `capabilities.machines` statt aus der festen Liste mit dem ungültigen `pc-i440fx`. Wurde nichts erhoben (`!was_probed()`), bleiben alle Optionen sichtbar und tragen einen Hinweis „ungeprüft" — nicht ausblenden, was man nicht weiß.

- [ ] **Step 1: Add the catalogue keys** `firmware`, `firmware_bios`, `firmware_uefi`, `option_unverified`, `firmware_missing` in alle drei Kataloge.

- [ ] **Step 2: Write the failing tests**

```rust
#[test]
fn test_bridge_is_offered_only_when_qemu_has_it() {
    let without = QemuCapabilities {
        netdev_backends: vec!["user".into(), "tap".into()],
        ..Default::default()
    };
    assert!(!without.has_netdev("bridge"));

    let with = QemuCapabilities {
        netdev_backends: vec!["user".into(), "bridge".into()],
        ..Default::default()
    };
    assert!(with.has_netdev("bridge"));
}

#[test]
fn test_network_tab_does_not_draw_bridge_when_unavailable() {
    let caps = QemuCapabilities {
        netdev_backends: vec!["user".into(), "tap".into(), "none".into()],
        ..Default::default()
    };
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    let mut tab = DetailTab::Network;
    let mut modal = CreateDiskState::default();
    let i18n = I18n::new("en");

    let ctx = Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            VmDetailView::new(&mut vm, false, &i18n, &mut tab, None, &mut modal)
                .with_capabilities(&caps)
                .show(ui);
        });
    });
    let output = ctx.run(Default::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            VmDetailView::new(&mut vm, false, &i18n, &mut tab, None, &mut modal)
                .with_capabilities(&caps)
                .show(ui);
        });
    });

    assert!(
        !rendered_text(&output).contains("bridge"),
        "an option this QEMU cannot do must not be on offer"
    );
    assert!(
        rendered_text(&output).contains("user"),
        "precondition: the backend selector is actually on screen"
    );
}
```

`rendered_text` ist der Helfer aus `tests/test_app_resilience.rs`; er wandert nach `tests/common/mod.rs` und wird dort von beiden Dateien genutzt.

- [ ] **Step 3: Implement the gating**

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui/vm_detail.rs src/app.rs assets/locales tests/test_vm_detail.rs
git commit -m "fix(ui): offer only the options the installed QEMU actually has"
```

---

## Task 8: Die Tabelle über die gesamte Auswahlfläche

Der Test, der die acht Fehler verhindert hätte.

**Files:**
- Modify: `tests/test_qemu_acceptance.rs`

**Interfaces:**
- Consumes: alles aus den Aufgaben 1–6.
- Produces: nichts Neues.

- [ ] **Step 1: Write the table test**

Eine Dimension je Test, damit ein Fehlschlag sofort sagt, wo er sitzt. Jeder Wert wird auf eine ansonsten unveränderte Standard-VM gesetzt und einzeln geprüft:

```rust
/// Build a baseline VM with one working disk, then let the caller vary one field.
fn probe_vm(dir: &std::path::Path, tweak: impl FnOnce(&mut VmConfig)) -> VmConfig {
    let disk = dir.join("probe.qcow2");
    common::make_empty_qcow2(&disk);
    let mut vm = VmConfig::new_default("probe", Arch::X86_64);
    vm.drives.push(DriveConfig {
        path: disk,
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: None,
    });
    tweak(&mut vm);
    vm
}

#[test]
fn test_qemu_accepts_every_vga_type() {
    let dir = tempfile::tempdir().unwrap();
    for vga in ["virtio-vga", "std", "qxl", "cirrus", "none"] {
        let vm = probe_vm(dir.path(), |vm| vm.display.vga_type = vga.to_string());
        common::assert_accepted(vga, Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_every_display_backend() {
    let dir = tempfile::tempdir().unwrap();
    for backend in ["default", "gtk", "sdl", "none"] {
        let vm = probe_vm(dir.path(), |vm| {
            vm.display.display_backend = backend.to_string()
        });
        common::assert_accepted(backend, Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_every_nic_model() {
    let dir = tempfile::tempdir().unwrap();
    for nic in ["virtio-net-pci", "e1000", "rtl8139", "e1000e", "vmxnet3"] {
        let vm = probe_vm(dir.path(), |vm| vm.network.nic_model = nic.to_string());
        common::assert_accepted(nic, Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_every_offered_network_backend() {
    let dir = tempfile::tempdir().unwrap();
    let Some(bin) = find_qemu_binary(None, Arch::X86_64) else {
        return;
    };
    let caps = probe_capabilities(&bin);

    // Only what the UI will actually offer after Task 7's gating.
    for backend in ["user", "tap", "none"] {
        if backend != "none" && !caps.has_netdev(backend) {
            continue;
        }
        let vm = probe_vm(dir.path(), |vm| vm.network.backend = backend.to_string());
        common::assert_accepted(backend, Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_every_drive_format() {
    let dir = tempfile::tempdir().unwrap();
    for format in DriveFormat::all() {
        let disk = dir.path().join(format!("f.{}", format.as_str()));
        common::make_disk_image(&disk, *format);
        let mut vm = VmConfig::new_default("probe", Arch::X86_64);
        vm.drives.push(DriveConfig {
            path: disk,
            format: *format,
            interface: DriveInterface::VirtIO,
            boot_index: None,
        });
        common::assert_accepted(format.as_str(), Arch::X86_64, &build_qemu_args(&vm));
    }
}

#[test]
fn test_qemu_accepts_both_firmware_kinds() {
    let dir = tempfile::tempdir().unwrap();

    let bios = probe_vm(dir.path(), |vm| vm.firmware = Firmware::Bios { rom: None });
    common::assert_accepted("bios", Arch::X86_64, &build_qemu_args(&bios));

    let Some(uefi) = find_uefi_firmware(None) else {
        eprintln!("skipped uefi: no edk2 firmware on this machine");
        return;
    };
    let nvram = dir.path().join("nvram.fd");
    prepare_nvram(&uefi.vars_template, &nvram).unwrap();
    let vm = probe_vm(dir.path(), |vm| {
        vm.firmware = Firmware::Uefi {
            nvram: nvram.clone(),
        }
    });
    common::assert_accepted("uefi", Arch::X86_64, &build_qemu_args(&vm));
}
```

`make_disk_image(path, format)` ist die allgemeine Form von `make_empty_qcow2` und gehört ebenfalls zu `tests/common/mod.rs`. Die Maschinentypen kommen aus `caps.machines`; geprüft werden die, die die Oberfläche nach Aufgabe 7 vorschlägt.

- [ ] **Step 2: Run it**

Run: `cargo test --test test_qemu_acceptance -- --nocapture`
Expected: PASS für jeden Wert. Was durchfällt, wird in der Oberfläche ausgeblendet (Aufgabe 7) oder richtig gebaut — nicht im Test entschuldigt.

- [ ] **Step 3: Commit**

```bash
git add tests/test_qemu_acceptance.rs
git commit -m "test: check every selectable option against the installed QEMU"
```

---

## Abschluss von Teil 2

- [ ] `cargo test` — die 113 Tests aus Teil 1 plus die neuen laufen grün
- [ ] `cargo fmt --check` und `cargo clippy --all-targets` ohne Beanstandung
- [ ] Alle acht in §3.1 nachgewiesenen Fehler sind durch je einen Test gedeckt
- [ ] Eine bestehende `vms.json` aus Teil 1 lädt unverändert und behält ihre Laufwerke
- [ ] Von Hand: eine UEFI-VM startet und zeigt die EDK2-Oberfläche
