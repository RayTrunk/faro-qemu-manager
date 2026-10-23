# Teil 3a — Ablageort, Vorlagen, Wizard, Startprüfung: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine Windows-11-VM entsteht in zwei Klicks aus einer Vorlage, mit Festplatte und NVRAM an einem festen Ort — und was den Start verhindern würde, sagt die Oberfläche vorher statt die Konsole hinterher.

**Architecture:** Drei neue Einheiten mit je einer Aufgabe: `src/templates.rs` hält die sechs Vorlagen als Daten im Code mit `all()`-Zugang nach dem Muster von `Arch`; `src/preflight.rs` prüft eine VM gegen Wirt, Dateisystem und das laufende QEMU und gibt eine Liste von Befunden zurück, ohne irgendetwas anzuzeigen; `src/ui/wizard.rs` ist der Dialog aus Variante C. `src/config.rs` bekommt das VM-Verzeichnis, die Betriebsart und das Schema-Merkmal aus §11.7. `src/ui/vm_detail.rs` wird vorher aufgeteilt, weil der Wizard sonst in eine Datei mit 1496 Zeilen hineinwächst.

**Tech Stack:** Rust 2021, `eframe`/`egui` 0.29.1, `serde`/`serde_json`, `uuid`, `rfd`, `tempfile` (Tests). **Keine neuen Abhängigkeiten in diesem Teil** — Kernzahl kommt aus `std::thread::available_parallelism`, Wirtsspeicher aus plattformeigenen Aufrufen (§11.8).

**Spec:** `docs/superpowers/specs/2026-10-23-wizard-und-haertung-design.md`, §5.3–§5.7, §8, mit den Festlegungen §11.1, §11.2, §11.5, §11.6, §11.7

## Global Constraints

- Rust 2021, `eframe`/`egui` 0.29.1 — kein Framework-Wechsel (Spec §2).
- Keine neuen Abhängigkeiten in diesem Teil (Spec §11.8 erlaubt sie erst für Teil 3b).
- **Nichts tut still etwas anderes, als es sagt** (Spec §1, Anti-Ziel).
- **Eine Vorlage ist eine Startbelegung, kein Band** (Spec §5.5): nach dem Anlegen ist jedes Feld frei änderbar, und keine Vorlage schreibt sich später wieder durch.
- **Die Vorlagen setzen kein `bootindex`** (Spec §11.1). Bei UEFI startet die leere Platte von der CD, nach der Installation hat der NVRAM-Eintrag Vorrang.
- **Kein Secure Boot** (Spec §11.2): `edk2-x86_64-code.fd`, die Vorlage heißt schlicht UEFI.
- **Altwerte werden nie still ersetzt** (Spec §11.6): gespeicherte `HAXM`, `bridge`, `if=nvme` bleiben stehen; die Startprüfung blockiert und **nennt den Ersatz**.
- **Zwei VMs dürfen nie auf dieselbe Abbilddatei zeigen** (Spec §5.7) — zwei QEMU-Prozesse auf einem qcow2 zerstören es.
- **Löschen fragt immer**, bevor ein Ordner mitgenommen wird (Spec §5.7).
- Neue Benutzertexte in alle drei Kataloge `assets/locales/{en,de,fr}.json`, von Anfang an (Spec §8).
- Code und Kommentare auf Englisch. `cargo fmt --check` und `cargo clippy --all-targets` ohne Beanstandung. Die 175 Tests aus Teilen 1 und 2 bleiben grün; kein Test wird gelöscht, um grün zu werden (Spec §11.3).

## Gegen QEMU 11.1.0 und diesen Wirt ausgeführte Tatsachen

Ausgeführt, nicht nachgelesen. Der Umsetzer kann es als gesichert behandeln.

- Die Argument- und Firmware-Arbeit aus Teil 2 liegt auf `master`: `build_qemu_args(&vm, uefi) -> Result<Vec<String>, String>`, `nvram_path_for(&vm) -> PathBuf`, `prepare_nvram(template, target)`, `FaroQemApp::ensure_nvram(&vm)`, `find_uefi_firmware(custom_dir)`, `QemuCapabilities` mit `was_probed()`.
- SATA heißt `-device ahci` plus `-device ide-hd,bus=faro-ahci.N` und wird von QEMU angenommen; virtio heißt `-device virtio-blk-pci`. Beides durch `tests/test_qemu_acceptance.rs` belegt.
- Der AHCI-Controller hat **sechs Ports**. Die Windows-Vorlagen legen eine Platte an, also unkritisch — aber die Startprüfung soll es sagen, wenn jemand sieben anhängt (`docs/superpowers/notes/2026-10-23-zurueckgestellt.md`, Punkt 1).
- `-vga` gibt es nur auf x86; alle sechs Vorlagen sind x86_64, also bleibt `virtio-vga` richtig.
- `std::thread::available_parallelism()` liefert die Kernzahl. Gesamtspeicher gibt es in `std` nicht.

---

## Review Focus

1. **Ein VM-Name, der kein Ordnername sein kann** — `Mein VM: C:\test?`, ein reservierter Windows-Name wie `CON`, ein leerer Name, ein Name aus nur Punkten. Der Benutzer erwartet eine angelegte VM, keinen Fehler und keinen Absturz. (Aufgabe 1)
2. **Zwei VMs mit demselben Namen** — die zweite darf nicht im Ordner der ersten landen, sonst schreiben zwei QEMU-Prozesse dasselbe qcow2 und zerstören es. (Aufgabe 1)
3. **Das VM-Verzeichnis ist nicht beschreibbar oder liegt auf einem fehlenden Laufwerk** — der Wizard muss das sagen und darf keine halbfertige VM in der Liste zurücklassen. (Aufgabe 5)
4. **Das Anlegen der Festplatte scheitert mitten im Vorgang** (kein Platz, `qemu-img` fehlt) — es darf keine VM entstehen, die auf eine nicht vorhandene Datei zeigt, ohne dass es jemand sagt. (Aufgabe 5)
5. **Eine gespeicherte VM mit veralteten Werten** (`HAXM`, `bridge`, ein Maschinentyp, den dieses QEMU nicht kennt) — die Startprüfung blockiert und nennt den Ersatz, statt vage zu warnen (§11.6). (Aufgabe 6)

---

## Dateistruktur

| Datei | Aufgabe |
|---|---|
| `src/config.rs` (ändern) | `AppSettings.vm_dir`, `AppSettings.creation_mode`, `AppSettings.schema`, `VmConfig.template`, `default_vm_dir()`, `vm_folder_name()`, `claim_vm_folder()` |
| `src/templates.rs` (neu) | Die sechs Vorlagen als Daten, `VmTemplate::all()`, `instantiate()` |
| `src/preflight.rs` (neu) | `Severity`, `Finding`, `HostFacts`, `check_vm()`, `blocks()` |
| `src/ui/wizard.rs` (neu) | Der Dialog aus Variante C |
| `src/ui/vm_detail/` (aufteilen) | `mod.rs`, `offers.rs`, `system.rs`, `storage.rs`, `display.rs`, `network.rs` |
| `src/ui/settings.rs` (ändern) | VM-Verzeichnis wählen, Betriebsart umschalten |
| `src/app.rs` (ändern) | Wizard verdrahten, Startprüfung vor dem Start, Duplizieren kopiert, Löschen fragt |

---

### Task 1: VM-Verzeichnis und Ordner je VM

**Files:**
- Modify: `src/config.rs`
- Test: `tests/test_config.rs`

**Interfaces:**
- Consumes: nichts aus späteren Aufgaben.
- Produces: `pub fn default_vm_dir() -> PathBuf`; `AppSettings.vm_dir: Option<PathBuf>` mit `pub fn vm_dir_or_default(&self) -> PathBuf`; `pub fn vm_folder_name(name: &str, id: Uuid) -> String`; `pub fn claim_vm_folder(root: &Path, name: &str, id: Uuid) -> Result<PathBuf, String>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_default_vm_dir_sits_in_the_user_profile() {
    let dir = default_vm_dir();
    assert!(dir.ends_with("FARO-VMs"), "got: {}", dir.display());
    assert!(dir.is_absolute(), "a relative default would land in the launch directory");
}

#[test]
fn test_a_name_that_cannot_be_a_folder_still_yields_one() {
    let id = Uuid::new_v4();
    for raw in ["Mein VM: C:\\test?", "", "...", "CON", "   ", "a/b\\c|d*e"] {
        let folder = vm_folder_name(raw, id);
        assert!(!folder.is_empty(), "{:?} yielded nothing", raw);
        assert!(
            !folder.contains(['/', '\\', ':', '?', '*', '|', '"', '<', '>']),
            "{:?} yielded {:?}", raw, folder
        );
        assert_ne!(folder.trim_matches('.'), "", "{:?} yielded only dots", raw);
        assert!(
            !matches!(folder.to_ascii_uppercase().as_str(), "CON" | "PRN" | "AUX" | "NUL"),
            "{:?} yielded a name Windows reserves", raw
        );
    }
}

#[test]
fn test_two_vms_with_one_name_get_two_folders() {
    let dir = tempdir().unwrap();
    let a = claim_vm_folder(dir.path(), "Windows 11", Uuid::new_v4()).unwrap();
    let b = claim_vm_folder(dir.path(), "Windows 11", Uuid::new_v4()).unwrap();
    assert_ne!(a, b, "two QEMU processes on one qcow2 destroy it");
    assert!(a.is_dir() && b.is_dir());
}

#[test]
fn test_claiming_the_same_folder_twice_for_one_vm_returns_the_same_path() {
    let dir = tempdir().unwrap();
    let id = Uuid::new_v4();
    let first = claim_vm_folder(dir.path(), "Debian", id).unwrap();
    let again = claim_vm_folder(dir.path(), "Debian", id).unwrap();
    assert_eq!(first, again, "re-opening a VM must not create a second folder");
}

#[test]
fn test_an_unusable_root_is_reported_not_panicked() {
    let dir = tempdir().unwrap();
    let blocker = dir.path().join("root");
    std::fs::write(&blocker, b"a file where the directory must go").unwrap();
    let err = claim_vm_folder(&blocker, "Alpine", Uuid::new_v4())
        .expect_err("an unusable VM directory must be reported");
    assert!(err.contains("root"), "name the path, got: {}", err);
}

#[test]
fn test_settings_without_a_vm_dir_fall_back_to_the_default() {
    let settings = AppSettings::default();
    assert_eq!(settings.vm_dir, None, "unset means 'use the default', not a frozen path");
    assert_eq!(settings.vm_dir_or_default(), default_vm_dir());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_config`
Expected: FAIL — `default_vm_dir`, `vm_folder_name`, `claim_vm_folder`, `vm_dir_or_default` existieren nicht.

- [ ] **Step 3: Implement in `src/config.rs`**

- `default_vm_dir()` — `%USERPROFILE%\FARO-VMs` auf Windows, `$HOME/FARO-VMs` sonst, Rückfall `default_config_dir().join("FARO-VMs")`, damit das Ergebnis absolut bleibt (Spec §5.7).
- `AppSettings` bekommt `#[serde(default, skip_serializing_if = "Option::is_none")] pub vm_dir: Option<PathBuf>`.
- `vm_folder_name` — Namensschema: alles außer `[A-Za-z0-9 ._-]` wird zu `-`, Folgen von `-` werden zusammengezogen, führende und schließende `.`, `-` und Leerzeichen fallen weg; bleibt nichts übrig oder ist das Ergebnis ein von Windows reservierter Name, lautet es `vm-<erste 8 Zeichen der id>`.
- `claim_vm_folder` — nimmt `root/<vm_folder_name>`, falls dieser Ordner noch nicht existiert oder bereits dieser VM gehört; sonst `root/<name>-<erste 8 der id>`, was für diese `id` eindeutig ist und damit bei einem zweiten Aufruf denselben Pfad liefert. Legt den Ordner mit `create_dir_all` an und gibt jeden `io::Error` mit dem Pfad im Text weiter.

Zugehörigkeit erkennt der Umsetzer an einer Markierungsdatei `vm-id` im Ordner, die `claim_vm_folder` beim Anlegen schreibt — ein Ordnername allein sagt nicht, wessen er ist.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test --test test_config`
Expected: PASS, die vorhandenen Konfigurationstests bleiben grün.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs tests/test_config.rs
git commit -m "feat(config): a VM directory, and one folder per VM that a name cannot break"
```

---

### Task 2: `vm_detail.rs` aufteilen, bevor der Wizard hineinwächst

**Files:**
- Create: `src/ui/vm_detail/mod.rs`, `offers.rs`, `system.rs`, `storage.rs`, `display.rs`, `network.rs`
- Delete: `src/ui/vm_detail.rs`
- Test: keiner neu — `tests/test_vm_detail.rs` ist der Test.

**Interfaces:**
- Consumes: nichts.
- Produces: **unveränderte** öffentliche Schnittstelle. `faro_qem::ui::vm_detail::*` und `faro_qem::ui::*` exportieren weiterhin genau `DetailTab`, `VmDetailAction`, `VmDetailView`, `CreateDiskState`, `offered_machine_types`, `offered_netdev_backends`, `offered_accelerators`, `offered_vga_types`.

Reiner Umbau (REFACTOR-Phase): kein Verhalten ändert sich, keine Zusicherung wird angefasst. Der Beweis ist, dass die Sammlung ohne eine Änderung an `tests/test_vm_detail.rs` durchläuft.

- [ ] **Step 1: Run the suite and record the number**

Run: `cargo test --test test_vm_detail`
Expected: PASS mit 24 Tests. Diese Zahl ist der Maßstab für Schritt 3.

- [ ] **Step 2: Move the code into the new module**

`mod.rs` nimmt `DetailTab`, `VmDetailAction`, die `VmDetailView`-Struktur samt `new`/`with_*`/`show`, und `pub use` für die Untermodule. Die vier Tab-Methoden ziehen in gleichnamige Dateien, je als eigener `impl<'a> VmDetailView<'a>`-Block mit `use super::VmDetailView;`. Die reinen `offered_*`-Funktionen ziehen nach `offers.rs`, `CreateDiskState` samt Modal nach `storage.rs`.

- [ ] **Step 3: Run the suite and compare**

Run: `cargo test`
Expected: PASS, 175 Tests, `tests/test_vm_detail.rs` unverändert. Jede nötige Änderung an einer Testdatei heißt, dass die Schnittstelle sich doch verschoben hat — dann zurückziehen, nicht den Test anpassen.

- [ ] **Step 4: Commit**

```bash
git add -A src/ui
git commit -m "refactor(ui): split the 1496-line detail panel before the wizard grows it"
```

---

### Task 3: Die sechs Vorlagen

**Files:**
- Create: `src/templates.rs`, `tests/test_templates.rs`
- Modify: `src/lib.rs`, `src/config.rs`, `tests/test_qemu_acceptance.rs`, `assets/locales/{en,de,fr}.json`

**Interfaces:**
- Consumes: `nvram_path_for` und `Firmware` aus Teil 2.
- Produces:
  - `pub struct VmTemplate { pub key: &'static str, pub name: &'static str, pub arch: Arch, pub uefi: bool, pub interface: DriveInterface, pub cpu_cores: u32, pub memory_mb: u32, pub disk_gb: u32, pub nic_model: &'static str, pub extra_args: &'static str, pub notice_key: Option<&'static str>, pub iso_source: Option<&'static str> }`
  - `impl VmTemplate { pub fn all() -> &'static [VmTemplate]; pub fn by_key(key: &str) -> Option<&'static VmTemplate>; pub fn instantiate(&self, name: &str, folder: &Path) -> VmConfig }`
  - `VmConfig.template: Option<String>`

`iso_source` ist der Schlüssel in den Download-Katalog aus Teil 3b und bleibt hier bei den vier Linux-Vorlagen gesetzt, aber unbenutzt.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_all_six_templates_are_there_with_the_decided_numbers() {
    let want = [
        // key, uefi, interface, cores, memory, disk_gb, nic
        ("windows-11", true,  DriveInterface::Sata,   4, 8192, 64, "e1000e"),
        ("windows-10", true,  DriveInterface::Sata,   2, 4096, 64, "e1000e"),
        ("debian",     true,  DriveInterface::VirtIO, 2, 4096, 32, "virtio-net-pci"),
        ("ubuntu",     true,  DriveInterface::VirtIO, 4, 4096, 40, "virtio-net-pci"),
        ("arch",       true,  DriveInterface::VirtIO, 2, 4096, 32, "virtio-net-pci"),
        ("alpine",     false, DriveInterface::VirtIO, 1, 1024,  8, "virtio-net-pci"),
    ];
    assert_eq!(VmTemplate::all().len(), want.len());
    for (key, uefi, iface, cores, mem, disk, nic) in want {
        let t = VmTemplate::by_key(key).unwrap_or_else(|| panic!("missing {}", key));
        assert_eq!(
            (t.uefi, t.interface, t.cpu_cores, t.memory_mb, t.disk_gb, t.nic_model),
            (uefi, iface, cores, mem, disk, nic),
            "{}", key
        );
        assert_eq!(t.arch, Arch::X86_64, "{}", key);
    }
}

#[test]
fn test_windows_templates_keep_the_host_clock_and_warn_about_tpm() {
    for key in ["windows-11", "windows-10"] {
        let t = VmTemplate::by_key(key).unwrap();
        assert!(t.extra_args.contains("-rtc base=localtime"), "{} shows the wrong time without it", key);
        assert!(t.notice_key.is_some(), "{} needs the TPM hint", key);
        assert_eq!(t.iso_source, None, "Microsoft hands out session-bound links");
    }
}

#[test]
fn test_the_four_linux_templates_name_a_download_source() {
    for key in ["debian", "ubuntu", "arch", "alpine"] {
        assert!(VmTemplate::by_key(key).unwrap().iso_source.is_some(), "{}", key);
    }
}

#[test]
fn test_no_template_sets_a_boot_index() {
    // Spec §11.1: UEFI boots the CD while the disk is empty, and the guest's
    // own NVRAM entry wins afterwards. An explicit index would fight that.
    let dir = tempdir().unwrap();
    for t in VmTemplate::all() {
        let vm = t.instantiate("probe", dir.path());
        assert!(vm.drives.iter().all(|d| d.boot_index.is_none()), "{}", t.key);
    }
}

#[test]
fn test_instantiating_puts_disk_and_nvram_in_the_vm_folder() {
    let dir = tempdir().unwrap();
    let vm = VmTemplate::by_key("windows-11").unwrap().instantiate("Win 11", dir.path());
    assert_eq!(vm.name, "Win 11");
    assert_eq!(vm.template.as_deref(), Some("windows-11"));
    assert_eq!(vm.drives.len(), 1);
    assert!(vm.drives[0].path.starts_with(dir.path()), "got: {}", vm.drives[0].path.display());
    match &vm.firmware {
        Firmware::Uefi { nvram } => assert!(nvram.starts_with(dir.path())),
        other => panic!("windows-11 is a UEFI template, got {:?}", other),
    }
    assert_eq!(vm.machine_type, "q35");
}

#[test]
fn test_instantiating_creates_no_files() {
    // Creating files is Task 5's job, under an error path that can undo them.
    let dir = tempdir().unwrap();
    let vm = VmTemplate::by_key("debian").unwrap().instantiate("Debian", dir.path());
    assert!(!vm.drives[0].path.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn test_alpine_is_the_one_bios_template() {
    let dir = tempdir().unwrap();
    let vm = VmTemplate::by_key("alpine").unwrap().instantiate("Alpine", dir.path());
    assert_eq!(vm.firmware, Firmware::Bios { rom: None });
}

#[test]
fn test_a_template_is_a_starting_assignment_not_a_band() {
    let dir = tempdir().unwrap();
    let mut vm = VmTemplate::by_key("debian").unwrap().instantiate("Debian", dir.path());
    vm.memory_mb = 123;
    vm.firmware = Firmware::Bios { rom: None };
    let back: VmConfig = serde_json::from_str(&serde_json::to_string(&vm).unwrap()).unwrap();
    assert_eq!(back.memory_mb, 123);
    assert_eq!(back.firmware, Firmware::Bios { rom: None });
    assert_eq!(back.template.as_deref(), Some("debian"), "the key is descriptive, not binding");
}

#[test]
fn test_every_template_notice_is_in_all_three_catalogues() {
    for lang in ["en", "de", "fr"] {
        let i18n = I18n::new(lang);
        for t in VmTemplate::all() {
            if let Some(key) = t.notice_key {
                assert_ne!(i18n.t(key), key, "{} missing {} in {}", t.key, key, lang);
            }
        }
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_templates`
Expected: FAIL — `src/templates.rs` existiert nicht.

- [ ] **Step 3: Implement**

- `src/templates.rs` mit `static TEMPLATES: [VmTemplate; 6]` und `all()`/`by_key()` nach dem Muster von `Arch::all()`; `src/lib.rs` bekommt `pub mod templates;`.
- `instantiate` baut auf `VmConfig::new_default(name, self.arch)`, setzt `machine_type = "q35"`, Kerne, Speicher, `display.vga_type = "virtio-vga"`, `network.nic_model`, `extra_args`, `template = Some(self.key.to_string())`, eine `DriveConfig` auf `folder.join("disk.qcow2")` mit `boot_index: None`, und bei `uefi` die `Firmware::Uefi { nvram: nvram_path_for(&vm) }`.
- `VmConfig` bekommt `#[serde(default, skip_serializing_if = "Option::is_none")] pub template: Option<String>`.
- Der TPM-Hinweis (`template_notice_windows`) sagt in allen drei Sprachen, was im Setup zu tun ist: bei „Dieser PC kann Windows 11 nicht ausführen" mit Umschalt+F10 die Eingabeaufforderung öffnen und unter `HKLM\SYSTEM\Setup\LabConfig` die Werte `BypassTPMCheck` und `BypassSecureBootCheck` auf 1 setzen. Dazu der Satz aus Spec §3.2, dass vTPM auf nativem Windows nicht möglich ist, damit niemand nach der Einstellung sucht.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test --test test_templates && cargo test`
Expected: PASS, die 175 bestehenden Tests bleiben grün.

- [ ] **Step 5: Add the acceptance test Spec §10 requires**

Jede der sechs Vorlagen muss eine Befehlszeile ergeben, die das vorliegende QEMU annimmt. In `tests/test_qemu_acceptance.rs`, mit `PROBE_MEMORY_MB` statt der Vorlagenzahl, weil parallele Proben sonst den Wirtsspeicher erschöpfen:

```rust
#[test]
fn test_qemu_accepts_the_command_line_of_every_template() {
    let dir = tempfile::tempdir().unwrap();
    for t in faro_qem::templates::VmTemplate::all() {
        let folder = dir.path().join(t.key);
        std::fs::create_dir_all(&folder).unwrap();
        let mut vm = t.instantiate(t.key, &folder);
        vm.memory_mb = PROBE_MEMORY_MB;
        common::make_empty_qcow2(&vm.drives[0].path);
        if let Firmware::Uefi { nvram } = &vm.firmware {
            let Some(uefi) = find_uefi_firmware(None) else { continue };
            prepare_nvram(&uefi.vars_template, nvram).unwrap();
        }
        common::assert_accepted(t.key, vm.arch, &args_for(&vm));
    }
}
```

- [ ] **Step 6: Run it**

Run: `cargo test --test test_qemu_acceptance`
Expected: PASS. Eine Ablehnung ist ein Befund an der Vorlage, nicht am Test.

- [ ] **Step 7: Commit**

```bash
git add src/templates.rs src/lib.rs src/config.rs tests assets/locales
git commit -m "feat: the six templates, and QEMU's word that each one starts"
```

---

### Task 4: Der Wizard — Variante C

**Files:**
- Create: `src/ui/wizard.rs`, `tests/test_wizard.rs`
- Modify: `src/ui/mod.rs`, `assets/locales/{en,de,fr}.json`

**Interfaces:**
- Consumes: `VmTemplate::all()`, `VmTemplate::instantiate`, die `offered_*`-Funktionen aus `vm_detail::offers`.
- Produces:
  - `pub struct WizardState { pub open: bool, pub selected: Option<&'static str>, pub name: String, pub iso: String, pub hardware_open: bool, pub network_open: bool, pub draft: Option<VmConfig> }` mit `Default`
  - `impl WizardState { pub fn select(&mut self, key: &str); pub fn select_empty(&mut self, arch: Arch); pub fn why_not_create(&self) -> Option<String> }`
  - `pub enum WizardAction { Create(VmConfig), Cancel }`
  - `pub struct WizardDialog<'a>` mit `new(state: &'a mut WizardState, i18n: &'a I18n) -> Self`, `with_capabilities(&'a QemuCapabilities)`, `with_firmware(bool)`, `show(&mut self, ctx: &egui::Context) -> Option<WizardAction>`

Der Dialog **legt nichts an**. Er gibt den Entwurf heraus; Aufgabe 5 schreibt Dateien.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_the_gallery_shows_all_six_templates_and_the_empty_tile() {
    let i18n = I18n::new("en");
    let drawn = render_wizard(&mut WizardState { open: true, ..Default::default() });
    for t in VmTemplate::all() {
        assert!(drawn.contains(t.name), "missing tile {}, drew:\n{}", t.name, drawn);
    }
    assert!(drawn.contains(&i18n.t("wizard_empty_tile")), "drew:\n{}", drawn);
}

#[test]
fn test_name_and_iso_are_the_only_always_visible_fields() {
    let i18n = I18n::new("en");
    let mut state = WizardState { open: true, ..Default::default() };
    state.select("debian");
    let drawn = render_wizard(&mut state);
    assert!(drawn.contains(&i18n.t("name")));
    assert!(drawn.contains(&i18n.t("install_iso")));
    assert!(drawn.contains(&i18n.t("wizard_hardware_block")));
    assert!(drawn.contains(&i18n.t("wizard_network_block")));
    assert!(!drawn.contains(&i18n.t("accel")), "a collapsed block must not draw its fields");
}

#[test]
fn test_a_collapsed_block_shows_its_core_values_as_a_line() {
    // Spec §5.4: one sees what is set without clicking.
    let mut state = WizardState { open: true, ..Default::default() };
    state.select("windows-11");
    let drawn = render_wizard(&mut state);
    for needle in ["4", "8192", "64", "UEFI", "SATA"] {
        assert!(drawn.contains(needle), "missing {}, drew:\n{}", needle, drawn);
    }
}

#[test]
fn test_choosing_windows_shows_the_tpm_hint() {
    let i18n = I18n::new("en");
    let mut state = WizardState { open: true, ..Default::default() };
    state.select("windows-11");
    let drawn = render_wizard(&mut state);
    assert!(drawn.contains(&i18n.t("template_notice_windows")), "drew:\n{}", drawn);
}

#[test]
fn test_choosing_a_template_fills_the_draft_and_another_replaces_it() {
    let mut state = WizardState { open: true, ..Default::default() };
    state.select("windows-11");
    assert_eq!(state.draft.as_ref().unwrap().memory_mb, 8192);
    state.select("alpine");
    assert_eq!(state.draft.as_ref().unwrap().memory_mb, 1024);
}

#[test]
fn test_an_edit_survives_until_a_template_is_chosen_again() {
    // A template is a starting assignment: editing is allowed, and picking a
    // template starts over rather than merging two assignments.
    let mut state = WizardState { open: true, ..Default::default() };
    state.select("debian");
    state.draft.as_mut().unwrap().memory_mb = 777;
    assert_eq!(state.draft.as_ref().unwrap().memory_mb, 777);
    state.select("debian");
    assert_eq!(state.draft.as_ref().unwrap().memory_mb, 4096);
}

#[test]
fn test_create_is_refused_without_a_name_and_without_a_choice() {
    let mut state = WizardState { open: true, ..Default::default() };
    assert!(state.why_not_create().is_some(), "nothing is chosen yet");
    state.select("debian");
    state.name = "  ".to_string();
    assert!(state.why_not_create().is_some(), "a VM without a name cannot get a folder");
    state.name = "Debian 13".to_string();
    assert_eq!(state.why_not_create(), None);
}

#[test]
fn test_the_empty_tile_promises_nothing() {
    let mut state = WizardState { open: true, name: "Blank".to_string(), ..Default::default() };
    state.select_empty(Arch::X86_64);
    let draft = state.draft.as_ref().unwrap();
    assert_eq!(draft.template, None);
    assert!(draft.drives.is_empty());
    assert_eq!(state.why_not_create(), None);
}
```

`render_wizard` ist eine Testhilfe in `tests/test_wizard.rs`, die den Dialog zweimal durch einen `egui::Context::default()` laufen lässt und `common::rendered_text` zurückgibt — dasselbe Muster wie in `tests/test_vm_detail.rs`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_wizard`
Expected: FAIL — `src/ui/wizard.rs` existiert nicht.

- [ ] **Step 3: Implement**

Ein `egui::Window` mit `.collapsible(false).resizable(false)` und genau der Reihenfolge aus Spec §5.4: Kachelgalerie (sechs Vorlagen plus „Leer / manuell"), Name, Installations-ISO mit Durchsuchen-Knopf, zwei `egui::CollapsingHeader` deren zugeklappte Kopfzeile die Kernwerte als Text trägt, Hinweisfeld, und unten „Erstellen" (gesperrt solange `why_not_create()` etwas liefert, mit dem Grund als Beschriftung daneben) und „Abbrechen". Die Felder in den aufgeklappten Blöcken schreiben direkt in `state.draft`; die Auswahllisten kommen aus `offers.rs`, damit Wizard und Detailbereich nicht auseinanderlaufen.

`select(key)` ersetzt den Entwurf durch `VmTemplate::by_key(key).instantiate(&self.name, Path::new(""))` — der Ordner ist zur Auswahlzeit noch nicht beansprucht, die Pfade setzt Aufgabe 5. `select_empty(arch)` setzt `VmConfig::new_default(&self.name, arch)` ohne Laufwerke.

Am ISO-Feld bleibt Platz für die Download-Schaltfläche aus Teil 3b; dieser Teil zeigt sie nicht.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test --test test_wizard && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/wizard.rs src/ui/mod.rs tests/test_wizard.rs assets/locales
git commit -m "feat(ui): the template wizard, variant C"
```

---

### Task 5: Anlegen — Ordner, Festplatte, NVRAM, und die Betriebsart

**Files:**
- Modify: `src/app.rs`, `src/config.rs`, `src/ui/settings.rs`, `assets/locales/*`
- Test: `tests/test_app.rs`, `tests/test_sidebar_settings.rs`

**Interfaces:**
- Consumes: `claim_vm_folder`, `VmTemplate::instantiate`, `WizardAction`, `FaroQemApp::ensure_nvram`, `create_disk_image`, `nvram_path_for`.
- Produces: `pub enum CreationMode { Wizard, Direct }`; `AppSettings.creation_mode: CreationMode`; `AppSettings.schema: Option<u32>`; `FaroQemApp.wizard: WizardState`; `FaroQemApp::create_from_wizard(&mut self, draft: VmConfig) -> Result<Uuid, String>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_a_fresh_installation_starts_in_the_wizard_and_an_existing_one_does_not() {
    // Spec §11.7: the schema marker tells the two apart. A plain serde default
    // would push the wizard onto existing users.
    let dir = tempdir().unwrap();
    let settings = dir.path().join("settings.json");
    let vms = dir.path().join("vms.json");

    let fresh = FaroQemApp::with_paths(settings.clone(), vms.clone(), None);
    assert_eq!(fresh.settings.creation_mode, CreationMode::Wizard);
    drop(fresh);

    std::fs::write(&settings, br#"{"language":"de","theme":"dark"}"#).unwrap();
    std::fs::write(&vms, b"[]").unwrap();
    let legacy = FaroQemApp::with_paths(settings, vms, None);
    assert_eq!(legacy.settings.creation_mode, CreationMode::Direct);
}

#[test]
fn test_a_saved_settings_file_carries_the_marker_from_then_on() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    save_settings(&path, &AppSettings::default()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"schema\""), "without it every start looks fresh again: {}", text);
}

#[test]
fn test_creating_from_the_wizard_makes_the_folder_the_disk_and_the_nvram() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let draft = VmTemplate::by_key("windows-11").unwrap().instantiate("Win 11", Path::new(""));
    let id = app.create_from_wizard(draft).expect("the VM is created");

    let vm = app.vms.iter().find(|v| v.id == id).unwrap();
    assert!(vm.drives[0].path.is_file(), "no disk at {}", vm.drives[0].path.display());
    assert!(vm.drives[0].path.starts_with(dir.path()));
    match &vm.firmware {
        Firmware::Uefi { nvram } => assert!(nvram.is_file(), "no store at {}", nvram.display()),
        other => panic!("got {:?}", other),
    }
}

#[test]
fn test_an_unwritable_vm_directory_leaves_no_half_made_vm() {
    let dir = tempdir().unwrap();
    let blocker = dir.path().join("vms-root");
    std::fs::write(&blocker, b"a file where the directory must go").unwrap();
    let mut app = app_with_vm_dir(&blocker);
    let before = app.vms.len();
    let draft = VmTemplate::by_key("debian").unwrap().instantiate("Debian", Path::new(""));

    let err = app.create_from_wizard(draft).expect_err("an unusable directory must be reported");
    assert!(err.contains("vms-root"), "name the path, got: {}", err);
    assert_eq!(app.vms.len(), before, "a failed creation must not leave a VM in the list");
}

#[test]
fn test_a_failed_disk_leaves_no_vm_pointing_at_nothing() {
    // The VM directory points at a drive letter that is not mounted, so
    // claiming the folder works out to a path qemu-img cannot write.
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(Path::new("Z:\no-such-drive\vms"));
    let _ = dir;  // the real directory stays unused on purpose
    let draft = VmTemplate::by_key("alpine").unwrap().instantiate("Alpine", Path::new(""));
    let before = app.vms.len();
    assert!(app.create_from_wizard(draft).is_err());
    assert_eq!(app.vms.len(), before, "a failed creation must not leave a VM in the list");
}

#[test]
fn test_the_settings_dialog_offers_the_vm_directory_and_the_creation_mode() {
    let i18n = I18n::new("en");
    let drawn = render_settings(&mut AppSettings::default(), &i18n);
    assert!(drawn.contains(&i18n.t("vm_dir")), "drew:\n{}", drawn);
    assert!(drawn.contains(&i18n.t("creation_mode")), "drew:\n{}", drawn);
}
```

Der Fehlschlag beim Anlegen der Platte wird über ein nicht eingebundenes Laufwerk erzwungen. Scheitert das auf dem Zielwirt nicht (unter Linux etwa), nimmt der Umsetzer ein Mittel, das dort wirklich scheitert — ein Verzeichnis ohne Schreibrecht etwa — und benennt den Test entsprechend. Wichtig ist, dass der Fehler **vor** dem `push` auftritt.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_app --test test_sidebar_settings`
Expected: FAIL — `CreationMode`, `create_from_wizard` und die Dialogfelder fehlen.

- [ ] **Step 3: Implement**

- `pub enum CreationMode { Wizard, Direct }` in `src/config.rs`, mit `Serialize`/`Deserialize`/`PartialEq` und Vorbelegung `Wizard`.
- `AppSettings.schema: Option<u32>`, beim Speichern immer `Some(1)`. Die Unterscheidung aus §11.7 braucht die Kenntnis, ob `vms.json` existiert, und gehört darum in `FaroQemApp::with_paths`, nicht in `try_load_settings`: fehlt `schema` und existiert `vms_path`, wird `creation_mode = Direct` gesetzt.
- `create_from_wizard` in dieser Reihenfolge, damit ein Fehlschlag nichts zurücklässt: Ordner beanspruchen → Pfade im Entwurf auf diesen Ordner ziehen (`disk.qcow2`, `nvram_path_for`) → Festplatte anlegen → NVRAM anlegen → **erst dann** `self.vms.push(..)` und speichern. Jeder Fehler vorher gibt `Err` zurück und hinterlässt keine VM in der Liste.
- `SidebarAction::NewVm` liest `settings.creation_mode`: `Wizard` öffnet `self.wizard`, `Direct` bleibt genau beim heutigen Verhalten.
- `src/ui/settings.rs` bekommt einen Verzeichnis-Wähler über `rfd::FileDialog::pick_folder` und einen Zweiwege-Schalter für die Betriebsart.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A src tests assets/locales
git commit -m "feat: create a VM from a template, folder and disk and store first"
```

---

### Task 6: Startprüfung

**Files:**
- Create: `src/preflight.rs`, `tests/test_preflight.rs`
- Modify: `src/lib.rs`, `src/app.rs`, `src/ui/vm_detail/mod.rs`, `assets/locales/*`

**Interfaces:**
- Consumes: `QemuCapabilities`, `UefiFirmware`, `offered_accelerators`, `offered_netdev_backends`, `Firmware`, `I18n`.
- Produces:
  - `pub enum Severity { Blocking, Warning }`
  - `pub struct Finding { pub severity: Severity, pub text: String }`
  - `pub struct HostFacts { pub cores: u32, pub memory_mb: u64 }` mit `pub fn probe() -> HostFacts`
  - `pub fn check_vm(vm: &VmConfig, host: &HostFacts, caps: Option<&QemuCapabilities>, uefi: Option<&UefiFirmware>, i18n: &I18n) -> Vec<Finding>`
  - `pub fn blocks(findings: &[Finding]) -> bool`

Richtschnur aus Spec §5.6: **nur Fehlendes blockiert, Überdimensioniertes warnt.**

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_a_missing_disk_blocks_and_names_the_file() {
    let vm = vm_with_disk(PathBuf::from("C:\\nope\\disk.qcow2"));
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    let blocking: Vec<_> = findings.iter().filter(|f| f.severity == Severity::Blocking).collect();
    assert_eq!(blocking.len(), 1, "got: {:?}", findings);
    assert!(blocking[0].text.contains("disk.qcow2"), "got: {}", blocking[0].text);
}

#[test]
fn test_a_missing_iso_and_a_missing_store_block_too() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.cdroms.push(PathBuf::from("C:\\nope\\install.iso"));
    vm.firmware = Firmware::Uefi { nvram: PathBuf::from("C:\\nope\\nvram.fd") };
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.iter().filter(|f| f.severity == Severity::Blocking).count() >= 2, "got: {:?}", findings);
}

#[test]
fn test_more_memory_than_the_host_has_warns_but_does_not_block() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.memory_mb = 1_000_000;
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.iter().any(|f| f.severity == Severity::Warning), "got: {:?}", findings);
    assert!(!blocks(&findings), "oversizing is the user's call, not a refusal");
}

#[test]
fn test_more_cores_than_the_host_has_warns() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.cpu_cores = host().cores * 4 + 4;
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.iter().any(|f| f.severity == Severity::Warning));
    assert!(!blocks(&findings));
}

#[test]
fn test_a_stored_value_this_build_cannot_use_blocks_and_names_the_replacement() {
    // Spec §11.6: never rewritten in silence, and the start is held.
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.accel = Accelerator::HAXM;
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    let blocking: Vec<_> = findings.iter().filter(|f| f.severity == Severity::Blocking).collect();
    assert_eq!(blocking.len(), 1, "got: {:?}", findings);
    assert!(blocking[0].text.contains("HAXM"), "name the stored value, got: {}", blocking[0].text);
    assert!(
        blocking[0].text.contains("WHPX") || blocking[0].text.contains("TCG"),
        "name the replacement, got: {}", blocking[0].text
    );
}

#[test]
fn test_a_machine_type_the_running_qemu_does_not_know_blocks() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.machine_type = "pc-i440fx".to_string();  // a name QEMU rejects outright
    let Some(caps) = probed_caps() else { return };  // skips without QEMU
    let findings = check_vm(&vm, &host(), Some(&caps), None, &I18n::new("en"));
    assert!(blocks(&findings), "got: {:?}", findings);
}

#[test]
fn test_without_a_probe_a_free_text_value_is_not_held_against_the_user() {
    // Spec §11.6: with no QEMU found, options are shown but marked unverified.
    // Blocking here would hold a start that would have worked.
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.machine_type = "something-exotic".to_string();
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(!blocks(&findings), "got: {:?}", findings);
}

#[test]
fn test_rules_that_the_backend_cannot_apply_warn() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    vm.network.backend = "none".to_string();
    vm.network.port_forwards.push(PortForwardRule { host_port: 2222, guest_port: 22, proto: "tcp".into() });
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.iter().any(|f| f.severity == Severity::Warning));
    assert!(!blocks(&findings));
}

#[test]
fn test_more_than_six_sata_disks_warns_because_ahci_has_six_ports() {
    let dir = tempdir().unwrap();
    let mut vm = healthy_vm(dir.path());
    for n in 0..7 {
        let p = dir.path().join(format!("extra-{}.qcow2", n));
        std::fs::write(&p, b"").unwrap();
        vm.drives.push(DriveConfig { path: p, format: DriveFormat::Qcow2, interface: DriveInterface::Sata, boot_index: None });
    }
    let findings = check_vm(&vm, &host(), None, None, &I18n::new("en"));
    assert!(findings.iter().any(|f| f.text.to_lowercase().contains("ahci")), "got: {:?}", findings);
}

#[test]
fn test_a_healthy_vm_has_nothing_to_report() {
    let dir = tempdir().unwrap();
    let findings = check_vm(&healthy_vm(dir.path()), &host(), None, None, &I18n::new("en"));
    assert!(findings.is_empty(), "got: {:?}", findings);
}

#[test]
fn test_the_host_probe_answers_with_plausible_numbers() {
    let h = HostFacts::probe();
    assert!(h.cores >= 1);
    assert!(h.memory_mb >= 256, "a host with less than 256 MB cannot be running this");
}

#[test]
fn test_launching_a_vm_with_a_blocking_finding_does_not_start_qemu() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let mut vm = VmConfig::new_default("broken", Arch::X86_64);
    vm.drives.push(DriveConfig { path: PathBuf::from("C:\\nope\\disk.qcow2"), format: DriveFormat::Qcow2, interface: DriveInterface::VirtIO, boot_index: None });
    let id = vm.id;
    app.vms.push(vm);
    let err = app.launch_vm(id, None).expect_err("a missing disk holds the start");
    assert!(err.contains("disk.qcow2"), "got: {}", err);
    assert!(app.running_processes.is_empty());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_preflight`
Expected: FAIL — `src/preflight.rs` existiert nicht.

- [ ] **Step 3: Implement**

- `HostFacts::probe()` — Kerne aus `std::thread::available_parallelism()`; Gesamtspeicher auf Windows über eine eigene `extern "system"`-Erklärung von `GlobalMemoryStatusEx` (keine neue Abhängigkeit, §11.8), auf Linux aus `MemTotal` in `/proc/meminfo`, sonst ein großzügiger Rückfall, der nie warnt.
- `check_vm` sammelt die Befunde aus Spec §5.6 in dieser Einteilung. **Blockierend:** fehlende Festplatten-, ISO-, Firmware- und NVRAM-Dateien; ein gespeicherter Wert, den dieses Programm nicht ausführen kann (Beschleuniger nicht in `offered_accelerators()`, Netzwerk-Backend nicht in `offered_netdev_backends(caps)`); und — **nur wenn `caps.was_probed()`** — ein Maschinentyp oder Gerät, das die Abfrage nicht kennt. **Warnend:** Speicher und Kerne über denen des Wirts, Regeln unter einem Backend, das sie nicht anwendet, und mehr als sechs SATA-Platten. Jeder blockierende Befund zu einem Altwert nennt den Ersatz (§11.6).
- Die Texte kommen aus `i18n` mit eingesetzten Werten, damit sie in allen drei Sprachen vorliegen.
- `launch_vm` ruft `check_vm` vor `ensure_nvram` und bricht bei `blocks()` mit der Liste als Fehlertext ab, setzt `VmStatus::Error` und schreibt die Befunde in die Konsole.
- Der Detailbereich zeigt die Befunde über dem Start-Knopf: blockierende in `COLOR_ERROR`, warnende in `COLOR_WARNING`.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A src tests assets/locales
git commit -m "feat: say before the start what would stop it"
```

---

### Task 7: Duplizieren kopiert das Abbild, Löschen fragt nach dem Ordner

**Files:**
- Modify: `src/app.rs`, `assets/locales/*`
- Test: `tests/test_app.rs`

**Interfaces:**
- Consumes: `claim_vm_folder`, `rebase_nvram` (aus Teil 2), `AppSettings::vm_dir_or_default`.
- Produces: `FaroQemApp::delete_vm(&mut self, id: Uuid, with_folder: bool)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_duplicating_gives_the_copy_its_own_disk_file() {
    // Spec §5.7: two QEMU processes writing one qcow2 destroy it.
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = app.create_from_wizard(
        VmTemplate::by_key("alpine").unwrap().instantiate("Alpine", Path::new("")),
    ).unwrap();
    let original = app.vms.iter().find(|v| v.id == id).unwrap().drives[0].path.clone();
    std::fs::write(&original, b"ORIGINAL IMAGE").unwrap();

    app.handle_sidebar_action(SidebarAction::DuplicateVm(id), None);
    let copy = app.vms.iter().find(|v| v.id != id).unwrap();
    assert_ne!(copy.drives[0].path, original);
    assert_eq!(std::fs::read(&copy.drives[0].path).unwrap(), b"ORIGINAL IMAGE");
    assert_eq!(std::fs::read(&original).unwrap(), b"ORIGINAL IMAGE");
}

#[test]
fn test_deleting_without_consent_keeps_the_folder() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = app.create_from_wizard(
        VmTemplate::by_key("alpine").unwrap().instantiate("Alpine", Path::new("")),
    ).unwrap();
    let folder = app.vms.iter().find(|v| v.id == id).unwrap()
        .drives[0].path.parent().unwrap().to_path_buf();

    app.delete_vm(id, false);
    assert!(app.vms.iter().all(|v| v.id != id));
    assert!(folder.is_dir(), "the folder is the user's data, not ours to take unasked");
}

#[test]
fn test_deleting_with_consent_takes_the_folder() {
    let dir = tempdir().unwrap();
    let mut app = app_with_vm_dir(dir.path());
    let id = app.create_from_wizard(
        VmTemplate::by_key("alpine").unwrap().instantiate("Alpine", Path::new("")),
    ).unwrap();
    let folder = app.vms.iter().find(|v| v.id == id).unwrap()
        .drives[0].path.parent().unwrap().to_path_buf();

    app.delete_vm(id, true);
    assert!(!folder.exists(), "consent was given");
}

#[test]
fn test_deleting_never_reaches_outside_the_vm_directory() {
    // A VM whose disk the user pointed somewhere else must not drag that
    // directory away with it.
    let dir = tempdir().unwrap();
    let outside = dir.path().join("my-documents");
    std::fs::create_dir_all(&outside).unwrap();
    let disk = outside.join("important.qcow2");
    std::fs::write(&disk, b"KEEP").unwrap();

    let mut app = app_with_vm_dir(&dir.path().join("vms"));
    let mut vm = VmConfig::new_default("hand-made", Arch::X86_64);
    vm.drives.push(DriveConfig { path: disk.clone(), format: DriveFormat::Qcow2, interface: DriveInterface::VirtIO, boot_index: None });
    let id = vm.id;
    app.vms.push(vm);

    app.delete_vm(id, true);
    assert!(disk.is_file(), "only a folder under the VM directory may be taken");
}

#[test]
fn test_duplicating_a_hand_made_vm_says_the_image_stays_shared() {
    // An image outside the VM directory is not ours to copy; the user hears
    // that the two machines share it rather than finding out by corruption.
    let dir = tempdir().unwrap();
    let outside = dir.path().join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    let disk = outside.join("shared.qcow2");
    std::fs::write(&disk, b"SHARED").unwrap();

    let mut app = app_with_vm_dir(&dir.path().join("vms"));
    let mut vm = VmConfig::new_default("hand-made", Arch::X86_64);
    vm.drives.push(DriveConfig { path: disk.clone(), format: DriveFormat::Qcow2, interface: DriveInterface::VirtIO, boot_index: None });
    let id = vm.id;
    app.vms.push(vm);

    app.handle_sidebar_action(SidebarAction::DuplicateVm(id), None);
    assert!(!app.notices.is_empty(), "a shared image must be said out loud");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_app`
Expected: FAIL — `delete_vm` existiert nicht, und das Duplizieren teilt noch die Abbilddatei.

- [ ] **Step 3: Implement**

- `DuplicateVm` beansprucht einen Ordner für die Kopie, kopiert jede Abbilddatei, die **unter dem VM-Verzeichnis** liegt, dorthin und zieht die Pfade nach. Eine Datei außerhalb bleibt ein geteilter Verweis und wird über `push_notice` als solcher genannt. Der NVRAM-Teil erledigt das vorhandene `rebase_nvram`.
- `delete_vm(id, with_folder)` entfernt die VM und — nur bei `with_folder` **und** nur wenn der Ordner ein echtes Kind von `settings.vm_dir_or_default()` ist — den Ordner.
- Der Löschdialog bekommt ein Kästchen „Ordner und Dateien mitnehmen", **nicht** vorangekreuzt, mit dem Pfad im Text.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A src tests assets/locales
git commit -m "feat: a copy gets its own image, and deleting asks before it takes a folder"
```

---

### Task 8: Pflege, die sonst nie passiert

**Files:**
- Modify: `assets/locales/{en,de,fr}.json`, `src/ui/**`, `build.rs`
- Test: `tests/test_i18n.rs`

**Interfaces:**
- Consumes: nichts.
- Produces: nichts Neues — nur, dass kein fest eingebauter englischer Text mehr in der Oberfläche steht.

Dies ist Spec §8 ohne den Aufteilungs-Punkt (Aufgabe 2) und den `.gitignore`-Punkt (in Teil 2 erledigt).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn test_the_three_catalogues_hold_the_same_keys() {
    let en = keys_of("assets/locales/en.json");
    for lang in ["de", "fr"] {
        assert_eq!(en, keys_of(&format!("assets/locales/{}.json", lang)), "{} differs from en", lang);
    }
}

#[test]
fn test_no_catalogue_key_is_unused() {
    // 16 keys sat unused; two of them were requirements nobody had wired.
    let sources = common::concat_sources("src");
    let unused: Vec<String> = keys_of("assets/locales/en.json")
        .into_iter()
        .filter(|k| !sources.contains(&format!("\"{}\"", k)))
        .collect();
    assert!(unused.is_empty(), "unused keys: {:?}", unused);
}

#[test]
fn test_the_interface_holds_no_hardcoded_english_hint() {
    // German and French users read English mid-screen today.
    let sources = common::concat_sources("src");
    for needle in [
        "(SLIRP NAT user-mode network with Internet access)",
        "(Host bridged interface)",
        "(No network connectivity)",
        "(Auto: WHPX on Windows)",
    ] {
        assert!(!sources.contains(needle), "still hardcoded: {}", needle);
    }
}

#[test]
fn test_nothing_in_the_ui_is_dead() {
    assert!(!common::concat_sources("src").contains("fn with_logo"), "SettingsDialog::with_logo has no caller");
}
```

`keys_of` ist eine Testhilfe in `tests/test_i18n.rs`; `sources_text()` gehört nach `tests/common/mod.rs` als `concat_sources("src")`, weil Teil 4 dieselbe Hilfe für einen Teilbaum braucht.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --test test_i18n`
Expected: FAIL — ungenutzte Schlüssel, fest eingebaute Texte, toter Code.

- [ ] **Step 3: Implement**

Jeden fest eingebauten englischen Text durch einen Katalogschlüssel ersetzen und in alle drei Sprachen eintragen; jeden Schlüssel, den niemand benutzt, entweder verdrahten oder entfernen — die Entscheidung je Schlüssel gehört als `Ruling:` ins Ledger; `SettingsDialog::with_logo` entfernen; `cargo fmt` auf `build.rs` anwenden.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test && cargo fmt --all --check && cargo clippy --all-targets`
Expected: PASS, keine Beanstandung.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: the interface speaks the chosen language everywhere"
```

---

## Selbstprüfung

**Spec-Abdeckung:** §5.3 → Aufgabe 5 (Betriebsart, §11.7). §5.4 → Aufgabe 4. §5.5 → Aufgabe 3, mit allen sechs Zahlenreihen als Zusicherung, §11.1 als eigener Test und §11.2 im TPM-Hinweis. §5.6 → Aufgabe 6. §5.7 → Aufgaben 1, 5, 7. §8 → Aufgaben 2 und 8. §10 (Annahme-Test je Vorlage) → Aufgabe 3, Schritt 5. §11.5 (Download am ISO-Feld) → Platz in Aufgabe 4, umgesetzt in Teil 3b. §11.6 → Aufgabe 6. §11.8 → keine neue Abhängigkeit, Wirtsspeicher plattformeigen.

**Bewusst offen gelassen:** Ob ein ungenutzter Katalogschlüssel verdrahtet oder entfernt wird (Aufgabe 8) — das hängt am einzelnen Schlüssel. Das Raster der Kachelgalerie — Spec §5.4 nennt die Reihenfolge, nicht die Spaltenzahl. Das Mittel, mit dem Aufgabe 5 einen fehlgeschlagenen Plattenanlauf erzwingt — es muss auf diesem Wirt wirklich scheitern, und was das ist, zeigt erst der Versuch.

**Nicht in diesem Plan:** Download (Teil 3b), Laufzeitverhalten (Teil 4).
