# FARO QEMU MANAGER — Vorlagen-Wizard & Härtung

**Datum:** 2026-10-23
**Status:** Entwurf, vom Auftraggeber freizugeben
**Betrifft:** Prioritäten 1–4 aus dem Code-Review vom 2026-10-23

Dieses Dokument ergänzt `2026-10-22-faro-qem-design.md` (Architektur des
Ist-Zustands). Jenes bleibt gültig und unverändert.

---

## 1. Absicht und Begründung

FARO QEMU MANAGER verspricht, virtuelle Maschinen *schnell* zu erstellen. Der
Weg dorthin ist bereits kurz — der Auftraggeber beschreibt ihn ausdrücklich als
nicht mühsam. Was fehlt, ist nicht ein kürzerer Weg, sondern dass das Ergebnis
**von Anfang an richtig eingestellt ist**.

Heute entsteht eine leere Hülle, bei der Chipsatz, Firmware, Plattenanbindung,
Netzwerkkarte und Bootreihenfolge selbst zusammengesucht werden müssen. Für
moderne Gäste — Windows 11 vor allem — ist das eine Kette von einem Dutzend
Entscheidungen, von denen jede einzelne die VM unbootbar machen kann. Parallel
stehen vier nachgewiesene Fehler im Argumentbau, die QEMU hart abbrechen
lassen, und ein Datenverlustpfad, der sämtliche VM-Konfigurationen
stillschweigend vernichtet.

Die Arbeit hat damit zwei Hälften, die zusammengehören: **Vertrauen** (nichts
geht verloren, nichts bricht ab) und **Vorbelegung** (die Vorlage weiß, was ein
Betriebssystem braucht). Die Vorlagen hängen technisch an den Bugfixes — die
Windows-Vorlage braucht SATA, und SATA ist einer der vier Fehler.

### Erfolg sieht so aus

- Eine kaputte `vms.json` kostet keine einzige VM-Konfiguration.
- **Jede** in der Oberfläche wählbare Einstellung führt zu einer QEMU-Zeile, die
  QEMU akzeptiert — nicht nur die sechs schon bekannten Fehler (§5.1).
- „Neue VM → Windows 11 → ISO wählen → Erstellen → Starten" führt zu einem
  laufenden Windows-11-Setup, ohne dass irgendein Feld angefasst werden muss.
- Der Stop-Knopf fährt den Gast regulär herunter.
- Eine laufende VM übersteht das Schließen der App und ist beim nächsten Start
  wieder bedienbar.

### Nicht-Ziele

- **Kein WSL2-Ausführungspfad.** Ausdrücklich verworfen. Folge: kein
  emuliertes TPM (Begründung in §3.2).
- **Keine Signaturprüfung** von Abbildern — nur Prüfsummen, mit der in §6
  benannten Einschränkung.
- **Kein echtes Secure Boot** (Begründung in §11.2).
- Keine Snapshot-Verwaltung, kein Shared Folder, kein USB-Passthrough, kein
  SPICE/VNC, kein Audio. Alles sinnvoll, alles später.
- Kein Umbau der Oberflächenästhetik. Das Erscheinungsbild aus
  `src/ui/theme.rs` bleibt, wie es ist.
- Keine Automatisierung des Windows-ISO-Downloads (Begründung in §6).

### Anti-Ziele — damit wäre die Arbeit gescheitert, auch wenn sie funktioniert

- Der direkte Weg zur leeren VM verschwindet oder wird umständlicher. Der
  Auftraggeber mag den heutigen Ablauf; der Wizard tritt daneben, nicht an
  seine Stelle.
- Der Wizard zwingt durch Seiten, die man nicht braucht.
- Eine Vorlage setzt still etwas, das sich später nicht mehr ändern lässt.
- **Irgendetwas tut still etwas anderes, als es sagt.** Das ist der Fehler, den
  `Accelerator::HAXM` heute macht (§5.1), und er darf nirgendwo neu entstehen —
  nicht beim Stop-Knopf (§7.1), nicht bei Secure Boot (§11.2), nicht bei
  Altwerten (§11.6).
- Die App wird langsam oder schreibt beim Beenden Konfigurationen kaputt.

---

## 2. Randbedingungen

- **Sprache und Rahmen:** Rust 2021, `eframe`/`egui` 0.29.1. Bestehender Stack,
  kein Framework-Wechsel.
- **Zielplattform:** Windows 11 ist der Hauptfall (Entwicklungs- und
  Nutzungsmaschine). Linux/macOS dürfen nicht brechen, sind aber nicht der
  Prüfmaßstab.
- **Referenz-QEMU:** 11.1.0, installiert unter `C:\Program Files\qemu`. Alle
  technischen Aussagen in §3 wurden gegen diese Installation **ausgeführt**,
  nicht aus Dokumentation abgeleitet.
- **Bestehende Tests:** 86 Tests. Die Sammlung läuft nach der Arbeit grün
  durch; einzelne Zusicherungen dürfen sich ändern, wo §5.1 die Ausgabe
  absichtlich korrigiert (§11.3).
- **Neue Abhängigkeiten:** erlaubt sind HTTP, SHA-256 und Wirtsspeicher-Abfrage
  (§11.8). Mehr nicht ohne Rückfrage.

---

## 3. Nachgewiesene technische Tatsachen

Gegen QEMU 11.1.0 ausgeführt. Der Umsetzer kann sie als gesichert behandeln.

### 3.1 Ungültige Werte, die die Oberfläche heute anbietet

| Befund | QEMU-Antwort |
|---|---|
| `-drive …,if=nvme` | `unsupported bus type 'nvme'` |
| `-drive …,if=sata` | `unsupported bus type 'sata'` |
| Zwei `-cdrom`-Angaben | `drive with bus=2, unit=0 (index=2) exists` |
| `hostfwd=` an `-netdev tap` | `Invalid parameter 'hostfwd'` |
| `-netdev bridge` | `network backend 'bridge' is not compiled into this binary` |

`if=` kennt ausschließlich `ide`, `scsi`, `sd`, `mtd`, `floppy`, `pflash`,
`virtio`, `none`. `bootindex` an einem `if=virtio`-Laufwerk ist dagegen gültig
(geprüft). `hostfwd` gehört allein zum `user`-Backend.

Ebenfalls geprüft und **gültig**: `-vga` mit `none`, `std`, `cirrus`, `vmware`,
`qxl`, `virtio`; die heutige Umschreibung `virtio-vga → -vga virtio` ist
richtig. `-display` kennt in diesem Build `none`, `gtk`, `sdl`,
`egl-headless`, `curses`, `spice-app`, `dbus` — und zusätzlich das in
`-display help` nicht aufgeführte `default`, das die App heute verwendet und
das QEMU annimmt. Die vollständige Durchmusterung der restlichen Auswahlfläche
steht noch aus und ist Teil der Arbeit (§5.1).

### 3.2 TPM ist auf nativem Windows unmöglich

`-tpmdev` existiert in diesem Build nicht (`invalid option`), `tpm-tis` fehlt in
`-device help`, `swtpm` ist nicht installiert. Das ist **keine Eigenart des
Pakets**: QEMU schaltet das TPM-Subsystem ab, sobald der Host Windows ist, weil
das einzige Emulator-Backend `swtpm` Unix-Domain-Sockets braucht. Microsofts
eigene QEMU-Q35-Dokumentation nennt Linux oder WSL als Voraussetzung und
nativen Windows-Host als nicht unterstützt. Es existiert also kein
Windows-QEMU-Build mit TPM, den man herunterladen könnte.

**Folge für Windows 11:** Die Hardwareprüfung des Setups muss übersprungen
werden. Das betrifft ausschließlich die TPM-/Secure-Boot-/RAM-Prüfung des
Installers, nicht Lizenzierung oder Aktivierung; Updates laufen anschließend
normal. Die App führt den Bypass **nicht selbst aus** — sie erklärt ihn an der
Stelle, an der er gebraucht wird (§5.4).

### 3.3 UEFI braucht pflash, nicht `-bios`

Vorhanden unter `C:\Program Files\qemu\share`:

- `edk2-x86_64-code.fd` — UEFI ohne Secure Boot
- `edk2-x86_64-secure-code.fd` — Secure-Boot-fähige Firmware
- `edk2-i386-vars.fd` — NVRAM-Vorlage; gilt auch für x86_64, eine Datei
  `edk2-x86_64-vars.fd` existiert nicht und wird nicht gebraucht

Die App kennt heute nur `-bios <datei>` (`src/qemu.rs:235`). Damit startet der
Installer, aber **das NVRAM wird nicht erhalten** — und Windows legt keinen
`\EFI\BOOT\BOOTX64.EFI`-Rückfallpfad an, sondern nur einen NVRAM-Booteintrag.
Ergebnis: Installation läuft durch, danach bootet die VM nicht mehr.

Korrekt ist das pflash-Paar, mit **einer eigenen NVRAM-Kopie pro VM**:

```
-drive if=pflash,format=raw,unit=0,readonly=on,file=<code.fd>
-drive if=pflash,format=raw,unit=1,file=<VM-eigene Kopie von vars.fd>
```

**Was genau geprüft wurde:** diese Zeile mit `edk2-x86_64-secure-code.fd` und
einer Kopie von `edk2-i386-vars.fd`, auf `q35,accel=whpx,kernel-irqchip=off`,
**ohne** `smm=on`. QEMU nahm sie an und startete (nur eine harmlose
CPUID-Warnung zu `svm`). Dass die Firmware startet, heißt **nicht**, dass
Secure Boot wirksam ist — siehe §11.2.

### 3.4 Datenverlust — mit Test belegt

`load_vms` (`src/config.rs:395`) wandelt jeden JSON-Parse-Fehler per
`unwrap_or_default()` in eine leere Liste. `FaroQemApp::with_paths`
(`src/app.rs:73`) deutet „leer" als „Erststart" und **überschreibt die Datei
sofort** mit einer Standard-VM. Ein einziges fehlendes Komma genügt, um alle
Konfigurationen unwiederbringlich zu verlieren. Verstärkend:
`write_config_file` (`src/config.rs:426`) schreibt ohne temp+rename und erzeugt
bei einem Abbruch genau solche Torsos; alle Aufrufer verwerfen Fehler mit
`let _ =`.

### 3.5 Arbeit pro Bild

- `find_qemu_img()` steht in `render_ui` (`src/app.rs:526`) und
  `find_qemu_binary()` im Einstellungsdialog (`src/ui/settings.rs:206`) — beide
  durchsuchen jedes Bild erneut PATH und Standardverzeichnisse mit
  `is_file()`-Aufrufen.
- Die Konsole legt bis zu 5000 Einträge pro Bild aus (`src/ui/console.rs:551`);
  mit aktivem Filter kommt `to_lowercase()` zweimal je Zeile hinzu (Zähler und
  Darstellung).

---

## 4. Teil 1 — Datenverlust schließen

Höchste Priorität, weil es der einzige irreversible Schaden ist.

**Entschieden:**

- Schreiben wird atomar: in eine Nachbardatei schreiben, dann über das Ziel
  umbenennen.
- Eine nicht lesbare Konfiguration wird **niemals überschrieben**. Sie wird
  beiseitegelegt (zeitgestempelter Name daneben), und die App sagt sichtbar,
  dass und wohin.
- „Datei leer/fehlt" und „Datei kaputt" werden auseinandergehalten. Nur der
  erste Fall darf zur Standard-VM führen.
- Schreibfehler werden nicht mehr verschluckt, sondern dem Benutzer gemeldet.

**Dem Umsetzer überlassen:** Namensschema der Beiseitelegung, Form der Meldung
(Dialog oder Statusleiste), ob `settings.json` dieselbe Behandlung bekommt
(empfohlen: ja, dieselbe Mechanik).

---

## 5. Teil 2 + 3 — Argumentbau und Vorlagen

Diese beiden Teile gehören zusammen: Die Windows-Vorlagen brauchen SATA, und
SATA ist einer der Fehler.

### 5.1 Argumentbau richtigstellen — vollständig

**Vom Auftraggeber entschieden:** nicht nur die sechs bekannten Fehler, sondern
**die komplette Auswahlfläche einmal systematisch durch**. Begründung: mit dem
Annahme-Test aus §10 ist das kaum teurer, und genau diese fehlende
Durchmusterung hat die bekannten Fehler entstehen lassen.

Bekannt fehlerhaft und zu beheben:

- **`Sata`** → über AHCI-Controller plus angehängtes Laufwerk statt `if=sata`.
- **`Nvme`** → über NVMe-Gerät statt `if=nvme`.
- **Mehrere ISOs** → nicht mehrfach `-cdrom`, sondern eigenständige
  CD-ROM-Laufwerke mit eindeutigem Index. Mehrere ISOs müssen gleichzeitig
  funktionieren; das braucht die virtio-Treiber-ISO neben der
  Installations-ISO.
- **`hostfwd`** nur noch beim `user`-Backend. Bei anderen Backends dürfen die
  Regeln erhalten bleiben, aber nicht in die Befehlszeile wandern; die
  Oberfläche sagt, dass sie dort nicht greifen.
- **`bridge`** verschwindet aus der Auswahl, wenn das laufende QEMU es nicht
  mitbringt — statt es anzubieten und scheitern zu lassen.
- **`Accelerator::HAXM`** verschwindet unter Windows aus der Auswahl, statt
  still auf WHPX umzubiegen (`src/qemu.rs:28`).

Zusätzlich zu prüfen und, wo nötig, zu beheben oder auszublenden: `ide` und
`scsi` auf q35, alle VGA-Typen und Anzeige-Backends im jeweils laufenden Build,
alle NIC-Modelle, und die Architekturen, deren QEMU-Programm nicht installiert
ist.

**Freitextfelder** (`machine_type`, `cpu_model`, eigene NIC-Modelle) bleiben
Freitext — sie sind der Fluchtweg für Fortgeschrittene. Sie werden nicht in der
Oberfläche beschnitten, sondern von der Startprüfung (§5.6) abgefangen.

**Dem Umsetzer überlassen:** die genauen Gerätezeilen und Bus-Benennungen, und
wie die Verfügbarkeitsabfrage von QEMU zwischengespeichert wird (sie darf nicht
pro Bild laufen, §7.2).

### 5.2 Firmware als eigenes Konzept

`bios_uefi: Option<PathBuf>` reicht nicht mehr. Die Konfiguration muss zwei
Fälle ausdrücken können — **altes BIOS** und **UEFI** — und im UEFI-Fall den
Pfad der VM-eigenen NVRAM-Datei festhalten. (Ein dritter Fall „UEFI mit Secure
Boot" entfällt, §11.2.)

Beim Anlegen einer UEFI-VM legt die App die NVRAM-Kopie an. Die Code-Datei wird
im QEMU-Verzeichnis selbst gefunden, damit keine Pfade getippt werden müssen.

Bestehende `vms.json`-Dateien müssen weiter laden. Ein altes `bios_uefi` wird
als „BIOS mit dieser Datei" gedeutet.

**Dem Umsetzer überlassen:** Feldnamen und Serialisierungsform, Verhalten wenn
die NVRAM-Datei fehlt (Vorschlag: neu aus der Vorlage anlegen und vermerken).

### 5.3 Zwei Betriebsarten beim Anlegen

**Entschieden:** Beide Wege existieren, umschaltbar in den Einstellungen.

- **Direkt** — genau das heutige Verhalten: Klick, leere Konfiguration
  erscheint, alles selbst einstellen. Verhält sich unverändert.
- **Wizard** — die Vorlagenauswahl aus §5.4.

Welcher Weg vorbelegt ist und wie Alt- von Neuinstallation unterschieden wird:
§11.7.

### 5.4 Der Wizard — Variante C

Vom Auftraggeber aus drei gezeigten Entwürfen gewählt. Ein einziger Dialog:

1. **Kachelgalerie** der Vorlagen, dazu eine Kachel „Leer / manuell".
2. **Name** und **Installations-ISO** — die beiden einzigen Felder, die immer
   sichtbar sind. Am ISO-Feld hängt bei den vier Linux-Vorlagen „Herunterladen"
   (§11.5).
3. **Zwei zugeklappte Blöcke**: „Hardware & Firmware" und „Netzwerk &
   Portweiterleitung". Zugeklappt zeigen sie ihre Kernwerte als Zeile, damit
   man sieht, was gesetzt ist, ohne zu klicken. Aufgeklappt sind alle Werte
   änderbar.
4. **Hinweisfeld** für vorlagenspezifische Warnungen — bei den
   Windows-Vorlagen der TPM-Hinweis mit dem, was im Setup zu tun ist.
5. **Erstellen** legt Festplatte, NVRAM-Datei und VM an.

Zwei Klicks im Normalfall. Nichts ist erzwungen, nichts ist versteckt.

### 5.5 Die Vorlagen

Sechs Vorlagen, vom Auftraggeber benannt: **Windows 11, Windows 10, Debian,
Ubuntu, Arch, Alpine**.

Die Plattenanbindung bei Windows ist **vom Auftraggeber entschieden**: SATA,
damit das Setup die Platte ohne Treibernachladen sieht. Bei den Linux-Vorlagen
ist virtio richtig, weil die Treiber im Kernel stecken.

Die übrigen Zahlen sind **meine Festlegung** und ausdrücklich zum Korrigieren:

| Vorlage | Firmware | Platte | vCPU | RAM | Größe | NIC |
|---|---|---|---|---|---|---|
| Windows 11 | UEFI | SATA/AHCI | 4 | 8192 MB | 64 GB | e1000e |
| Windows 10 | UEFI | SATA/AHCI | 2 | 4096 MB | 64 GB | e1000e |
| Debian | UEFI | virtio | 2 | 4096 MB | 32 GB | virtio-net-pci |
| Ubuntu | UEFI | virtio | 4 | 4096 MB | 40 GB | virtio-net-pci |
| Arch | UEFI | virtio | 2 | 4096 MB | 32 GB | virtio-net-pci |
| Alpine | BIOS | virtio | 1 | 1024 MB | 8 GB | virtio-net-pci |

Durchweg: q35, `usb-tablet` für einen sichtbaren Mauszeiger, NAT-Netzwerk,
keine ausdrückliche Bootreihenfolge (§11.1). Zusätzlich bei beiden
Windows-Vorlagen `rtc base=localtime`, sonst zeigt der Gast die falsche
Uhrzeit.

Eine Vorlage ist eine **Startbelegung, kein Band**: nach dem Anlegen ist die VM
eine gewöhnliche VM, jedes Feld frei änderbar.

**Dem Umsetzer überlassen:** ob Vorlagen fest im Code stehen oder als
mitgelieferte Daten (Vorschlag: im Code, mit `all()`-Zugang wie bei `Arch`),
und ob die Vorlagenherkunft in der VM vermerkt wird (Vorschlag: ja, als
beschreibendes Feld — nützlich für spätere Hinweise, ohne Verhalten zu
steuern).

### 5.6 Startprüfung

Vor dem Start prüft die App, was sich prüfen lässt, und sagt es in der
Oberfläche statt den Benutzer in die Konsole schauen zu lassen: fehlende
Festplatten-, ISO- und Firmware-Dateien, noch nicht fertige Downloads,
Speicher über dem des Wirts, Kerne über denen des Wirts, Regeln die beim
gewählten Netzwerk-Backend nicht greifen, und Freitextwerte, die das laufende
QEMU nicht kennt.

**Dem Umsetzer überlassen:** welche Befunde den Start verhindern und welche nur
warnen. Richtschnur: nur Fehlendes blockiert, Überdimensioniertes warnt.

### 5.7 Ablageort der VM-Dateien

Damit „keine Pfade tippen" wahr ist, braucht es einen Ort, an den der Wizard
legen kann. **Meine Festlegung:** eine neue Einstellung „VM-Verzeichnis" mit
Vorbelegung `%USERPROFILE%\FARO-VMs` unter Windows und `~/FARO-VMs` sonst,
darin ein Unterordner je VM. Festplatte, NVRAM-Datei und Downloads landen dort.

Ohne diesen Ort würde der Wizard 64-GB-Abbilder in das Arbeitsverzeichnis
legen, aus dem die App gestartet wurde — im Projektverzeichnis liegt genau so
ein verirrtes `debian_12.qcow2`.

Zwei Folgen, die mitbedacht werden müssen: **Duplizieren darf nicht zwei VMs
auf dieselbe Abbilddatei zeigen lassen** (zwei QEMU-Prozesse, die dasselbe
qcow2 beschreiben, zerstören es), und **Löschen** soll anbieten, den Ordner
mitzunehmen, aber niemals ohne Rückfrage löschen.

**Dem Umsetzer überlassen:** ob Duplizieren die Abbilddatei kopiert oder den
Verweis leert, Namensschema der Unterordner, Verhalten bei einem bereits
belegten Ordnernamen.

---

## 6. Teil 3b — Download

**Entschieden:** Herunterladen gibt es für alles mit stabiler, dokumentierter,
prüfbarer Adresse:

- **virtio-win-Treiber-ISO** — gebraucht, sobald bei einem Windows-Gast von
  SATA auf virtio gewechselt werden soll.
- **Installations-ISOs für Debian, Ubuntu, Arch, Alpine** — alle vier haben
  saubere Spiegel mit SHA256-Summen.

Jeder Download wird gegen seine SHA-256-Summe geprüft. Eine Datei, die nicht
passt, wird verworfen, nicht eingehängt.

**Ehrliche Grenze, die im Dokument und in der Oberfläche stehen muss:** Eine
Prüfsumme, die vom selben Spiegel kommt wie das Abbild, belegt
**Unverfälschtheit der Übertragung, nicht Echtheit der Herkunft**. Wer den
Spiegel kontrolliert, kontrolliert beides. Echte Echtheitsprüfung bräuchte
Signaturprüfung und ist ausdrücklich Nicht-Ziel (§1). Die Prüfsummen werden
über HTTPS von der Prüfsummendatei des jeweiligen Herausgebers geholt; das ist
die Grenze dessen, was dieser Umfang leistet, und sie wird nicht als mehr
verkauft.

**Entschieden, dass es Windows-ISOs nicht gibt:** Microsoft vergibt
sitzungsgebundene Links über eine Formularseite. Das zu automatisieren wäre
fragil und bewegt sich außerhalb der vorgesehenen Nutzung. Stattdessen öffnet
die App die offizielle Seite im Browser; die Datei holt der Benutzer selbst.

**Dem Umsetzer überlassen:** HTTP-Bibliothek, Fortschrittsdarstellung,
Abbrechbarkeit, Fortsetzbarkeit, und wie die Versionsangaben der Abbilder
gepflegt werden. Zwei harte Vorgaben: **die Oberfläche darf dabei nicht
einfrieren**, und **jeder Download wird geprüft**.

---

## 7. Teil 4 — Laufzeitverhalten

Dieser Teil ist durch die Entscheidung in §7.3 deutlich größer als im
ursprünglichen Review geschätzt. Das ist bewusst so gewählt.

### 7.1 Herunterfahren statt Töten

**Entschieden:** Der Stop-Knopf fährt den Gast künftig regulär herunter. Hartes
Töten bleibt erreichbar, aber als **zweiter, eigener Schritt**, wenn das
Herunterfahren nicht greift.

Dafür braucht die App einen QMP-Zugang zum laufenden QEMU, über den sie
`system_powerdown` schicken kann — im Gast wirkt das wie ein Druck auf den
Einschaltknopf, Windows und Linux fahren regulär herunter. Das dauert Sekunden
bis Minuten und kann bei einem hängenden Gast ausbleiben; die Oberfläche muss
diesen Wartezustand zeigen und den harten Weg dort anbieten.

**Meine Festlegung:** Lässt sich für eine VM kein QMP-Zugang herstellen, fällt
der Stop-Knopf **nicht still** auf hartes Töten zurück. Er sagt, dass
reguläres Herunterfahren für diese VM nicht verfügbar ist, und bietet
ausdrücklich den harten Weg an. Ein Knopf, der „Herunterfahren" sagt und den
Stecker zieht, wäre genau das Anti-Ziel aus §1.

### 7.2 Arbeit pro Bild beseitigen

- Suche nach QEMU-Programmen aus dem Bildaufbau herausnehmen; Ergebnis merken
  und nur erneut suchen, wenn sich das Verzeichnis ändert oder jemand es
  anstößt.
- Konsole nur noch sichtbare Zeilen auslegen, Filter nicht pro Bild und pro
  Zeile neu kleinschreiben.
- Festplatten anlegen und herunterladen dürfen die Oberfläche nicht blockieren
  (`create_disk_image` läuft heute im Bild-Thread).
- Beim Anhalten einer VM gehen heute die letzten Ausgaben und das
  Beendigungsereignis verloren (`src/app.rs:241`). Sie gehören in die Konsole.

### 7.3 Die VM überlebt die App

**Vom Auftraggeber entschieden.** Heute tötet `Drop` (`src/app.rs:613`) beim
Schließen alle laufenden VMs ohne Rückfrage. Künftig laufen sie weiter und sind
beim nächsten Start der App wieder bedienbar.

Das verlangt vier Dinge:

1. **Kein Töten beim Beenden.** Der `Drop`-Pfad verliert das Killen. Die App
   darf beim Schließen die Konfiguration speichern, aber keine Gäste
   umbringen.
2. **Laufzeitzustand überdauert den Prozess.** Je laufender VM werden
   Prozess-ID, QMP-Zugangsdaten und Startzeitpunkt festgehalten — in einer
   eigenen Zustandsdatei, nicht in `vms.json`, weil das flüchtige Angaben sind
   und `vms.json` die vom Benutzer gepflegte Wahrheit bleibt.
3. **Wiederanbindung mit Identitätsprüfung.** Beim Start wird jede vermerkte
   VM wieder eingesammelt. Eine nackte Prüfung „läuft diese PID noch?" genügt
   **nicht** — Prozess-IDs werden wiederverwendet, und die App würde sich an
   einen fremden Prozess hängen und ihn auf Knopfdruck töten. Die Identität
   wird darum über einen erfolgreichen QMP-Handschlag auf dem vermerkten Zugang
   bestätigt. Was sich nicht bestätigen lässt, gilt als beendet und wird
   aufgeräumt.
4. **Die Konsole muss die Lücke überbrücken.** Die stdout/stderr-Rohre gehören
   dem beendeten Elternprozess; sie sind nach einem Neustart der App
   unwiederbringlich weg. Damit die Konsole einer wieder eingesammelten VM
   nicht leer ist, schreibt die App die QEMU-Ausgabe **zusätzlich in eine
   Protokolldatei je VM** und liest diese beim Wiederanbinden ein. Ohne das
   wäre „wieder bedienbar" nur die halbe Wahrheit.

Beim Schließen der App mit laufenden VMs wird einmal gesagt, welche
weiterlaufen — ohne zu fragen, denn Weiterlaufen ist jetzt das gewollte
Verhalten.

**Dem Umsetzer überlassen:** Transportform des QMP-Zugangs auf Windows, Ort und
Format der Zustands- und Protokolldateien, Wartefrist bis der harte Weg
angeboten wird, Gestalt des Wartezustands, Obergrenze und Rotation der
Protokolldateien. Ob weitere QMP-Fähigkeiten (Pause, Fortsetzen, Kennzahlen)
mitgenommen werden: nicht in diesem Umfang, aber der Zugang soll sie nicht
verbauen.

---

## 8. Mitlaufende Pflege

Klein, aber Teil der Arbeit, weil es sonst nie passiert:

- **21 fest eingebaute englische Zeichenketten** in die Übersetzungskataloge
  überführen (u. a. `src/ui/vm_detail.rs:731,829,1056,1108`,
  `src/ui/sidebar.rs:392`). Deutsch- und Französisch-Nutzer sehen heute mitten
  in der Oberfläche Englisch. Alle neuen Texte dieser Arbeit — Vorlagennamen,
  Warnungen, Prüfmeldungen, TPM-Hinweis, Download-Grenze — gehören von Anfang
  an in alle drei Kataloge.
- **16 ungenutzte Katalogschlüssel** entfernen oder benutzen.
- `cargo fmt` auf `build.rs:10` anwenden; `cargo fmt --check` muss durchlaufen.
- Toten Code entfernen (`SettingsDialog::with_logo` wird nie aufgerufen).
- `src/ui/vm_detail.rs` hat 1350 Zeilen und bekommt mit dem Wizard weitere.
  Aufteilen, bevor weiteres hineinwächst.
- `.superpowers/` in `.gitignore` aufnehmen.

---

## 9. Reihenfolge

Vom Auftraggeber so bestimmt, mit einer technischen Abhängigkeit darin:

1. **Datenverlust** (§4) — zuerst, weil es der einzige irreversible Schaden ist
   und nichts davon abhängt.
2. **Argumentbau** (§5.1) und **Firmware** (§5.2) — Voraussetzung für die
   Vorlagen. Ohne SATA keine Windows-Vorlage.
3. **Ablageort** (§5.7), **Wizard und Vorlagen** (§5.3–5.6), dann **Download**
   (§6).
4. **Laufzeitverhalten** (§7) — darin §7.3 als größter Einzelposten.

Pflege (§8) läuft mit, nicht hinterher.

---

## 10. Prüfmaßstab

Für Argumentbau, Konfigurationsbehandlung und Vorlagen gilt der bestehende Weg:
Tests zuerst, dann Code.

**Annahme-Test:** Für jede der sechs Vorlagen und für jeden Wert der
Auswahlfläche (§5.1) gehört die erzeugte Befehlszeile in einen Test, der sie
gegen das vorliegende QEMU auf Annahme prüft. QEMU hat keinen Trockenlauf; die
Annahme wird durch tatsächliches Starten geprüft (angehalten und ohne Anzeige,
danach beendet). Wo kein QEMU vorhanden ist, überspringt der Test sich selbst —
dem Muster von `test_create_disk_image_with_system_qemu_img_if_present`
folgend. Die vier Fehler aus §3.1 wären mit diesem Test nie entstanden.

**Von Hand, vom Auftraggeber:** Windows 11 muss aus der Vorlage heraus
tatsächlich installieren und anschließend neu starten. Genau dort sitzt der
UEFI-Fehler aus §3.3, und nur ein echter Durchlauf zeigt, ob das NVRAM hält.
Der Auftraggeber hat diese Prüfung übernommen. Die Arbeit gilt damit als
**„fertig bis auf diese Bestätigung"**: alles andere ist abgeschlossen und
getestet, der Windows-11-Durchlauf ist der letzte offene Punkt und wird
übergeben. Für die anderen fünf Vorlagen genügt es, dass der Installer startet.

---

## 11. Entscheidungen aus dem Prüfer-Durchgang

Ein frischer Prüfer hat das Dokument gegengelesen und elf offene Punkte
gemeldet. Drei hat der Auftraggeber entschieden (§5.1, §7.3, §10). Die
folgenden acht sind **meine Festlegungen** — ein vernünftiger Umsetzer hätte
sie genauso treffen können, aber sie sollen nachlesbar sein.

### 11.1 Keine ausdrückliche Bootreihenfolge

Die Vorlagen setzen **kein** `bootindex`. Bei UEFI mit leerer Festplatte
startet die Firmware von der CD; sobald Windows installiert ist, hat sein
eigener NVRAM-Eintrag Vorrang vor der CD, und der Neustart landet auf der
Platte. Damit braucht es weder automatisches Auswerfen der ISO noch
Zustandshaltung für „Installation fertig". Dieser Mechanismus ist genau das,
was der Durchlauf in §10 bestätigen muss.

### 11.2 Secure Boot wird nicht behauptet

Die mitgelieferte `edk2-i386-vars.fd` enthält **keine eingetragenen
Microsoft-Schlüssel**, und wirksamer Schutz der Variablen bräuchte zusätzlich
`-machine q35,smm=on` samt der zugehörigen pflash-Eigenschaft. Mit
Standard-Variablen und ohne SMM bekäme man Firmware, die
„Secure-Boot-fähig" *heißt*, während Secure Boot praktisch aus ist — das
Anti-Ziel aus §1. Darum: Die Windows-11-Vorlage verwendet
`edk2-x86_64-code.fd` und heißt schlicht **UEFI**. Windows 11 braucht Secure
Boot ohnehin nicht, sobald die Prüfung übersprungen wird (§3.2). Der Fall
„UEFI mit Secure Boot" entfällt damit aus §5.2.

### 11.3 „86 Tests bleiben grün" heißt: die Sammlung läuft durch

Nicht: die Zusicherungen sind unantastbar. `tests/test_qemu_args.rs` prüft
heute auf `-cdrom …` und `-bios …` — beides korrigiert §5.1 absichtlich. Diese
Zusicherungen werden auf die neue, richtige Ausgabe gezogen. Kein Test wird
gelöscht, um grün zu werden.

### 11.4 Ablageort der VM-Dateien

Siehe §5.7 — neue Einstellung mit Vorbelegung, Unterordner je VM.

### 11.5 Download sitzt im Wizard am ISO-Feld

Bei den vier Linux-Vorlagen hängt „Herunterladen" direkt am ISO-Feld. Die VM
wird **sofort** angelegt, mit dem Ziel-Pfad als ISO und einer Markierung
„Download läuft"; der Download läuft im Hintergrund weiter, und die
Startprüfung (§5.6) hält den Start auf, bis die Datei da und geprüft ist. Nur
so bleibt das Zwei-Klick-Versprechen auch für jemanden wahr, der noch keine ISO
hat.

### 11.6 Altwerte werden nie still ersetzt

Eine gespeicherte Konfiguration kann `HAXM`, `bridge`, `if=nvme` oder
`if=sata` enthalten. Behandlung: Der Wert **bleibt stehen** und wird in der
Oberfläche als veraltet oder ungültig gekennzeichnet; die Startprüfung hält den
Start auf und nennt den Ersatz. Nicht: beim Laden stillschweigend umschreiben
(das ändert die Konfiguration des Benutzers ohne Zutun), und erst recht nicht:
beim Start stillschweigend etwas anderes starten (das ist der HAXM-Fehler, den
§5.1 beseitigt). Einzige Ausnahme ist das alte `bios_uefi`, dessen Deutung als
„BIOS mit dieser Datei" eindeutig ist. Findet sich überhaupt kein
QEMU-Programm, werden fähigkeitsabhängige Optionen angezeigt, aber als
ungeprüft gekennzeichnet — statt sie auszublenden und den Benutzer rätseln zu
lassen.

### 11.7 Alt- und Neuinstallation unterscheiden

`settings.json` bekommt ein Schema-Merkmal. Fehlt es und existiert bereits eine
`vms.json`, ist es eine Altinstallation: **direkter Weg** bleibt vorbelegt.
Fehlt es und gibt es keine `vms.json`, ist es eine Neuinstallation:
**Wizard** ist vorbelegt. Ein einfaches `#[serde(default)]`-Feld würde den
Wizard auch bestehenden Benutzern aufdrängen und damit die Absicht aus §5.3
verletzen.

### 11.8 Abhängigkeiten

Erlaubt: eine HTTP-Bibliothek (§6), eine SHA-256-Implementierung, und für die
Wirtsspeicher-Abfrage der Startprüfung entweder eine kleine
System-Info-Bibliothek oder plattformeigene Aufrufe — die Wahl liegt beim
Umsetzer. Kernzahl ist in `std` vorhanden, Gesamtspeicher nicht.
**Signaturprüfung ist Nicht-Ziel** (§1); die Grenze dessen, was die
Prüfsummen leisten, steht in §6 und muss auch in der Oberfläche stehen.
