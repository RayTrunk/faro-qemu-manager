# Zurückgestellte Befunde

**Datum:** 2026-10-23
**Herkunft:** Abschluss-Prüfungen von Teil 1 (`905f35c`) und Teil 2 (`2822a46`)
der Arbeit aus `docs/superpowers/specs/2026-10-23-wizard-und-haertung-design.md`

Diese Punkte sind geprüft und echt, aber bewusst nicht in den jeweiligen
Fix-Durchgang aufgenommen worden. Sie stehen hier, damit sie bei der Planung
von Teil 3 und Teil 4 vorliegen — und nicht nur im Arbeitsverzeichnis eines
abgeschlossenen Plans.

---

## Aus Teil 2 (Argumentbau)

### 1. Mehr als sechs SATA-Platten scheitern

`src/qemu.rs`, SATA-Zweig von `build_drive_args`. Der `ahci`-Controller hat
sechs Ports; die siebte Platte ergibt `Bus 'faro-ahci.6' not found`. Die
Oberfläche begrenzt die Anzahl nicht.

**Mögliche Behebung:** ab der siebten Platte einen zweiten AHCI-Controller
anlegen, oder die Oberfläche begrenzen und es sagen.

### 2. Die Display-Liste ist fest verdrahtet

`src/ui/vm_detail.rs`, Anzeige-Tab. Die Liste der Display-Hintergründe
(`gtk`, `sdl`, `none`, `default`) steht als Literal im Code, obwohl
`QemuCapabilities::has_display` vorliegt. Der Test wiederholt dieselben
Literale, beide können also auseinanderlaufen, ohne dass es auffällt.

Die VGA-Liste folgt seit Teil 2 der Architektur (`offered_vga_types`), ist
aber ebenfalls nicht fähigkeitsgeprüft.

**Mögliche Behebung:** `offered_display_backends(caps)` nach dem Muster von
`offered_netdev_backends`.

### 3. Fähigkeiten werden nur für eine Architektur erhoben

`src/app.rs`, `refresh_capabilities`. Geprüft wird das Programm für
`settings.default_arch`, angewandt wird das Ergebnis auf jede VM. Für eine
aarch64-VM filtert `offered_machine_types(Aarch64, x86_caps)` deshalb `virt`
weg, und der Benutzer bekommt gar keine Vorschläge mehr. Die VM startet
dennoch, weil ein leerer Maschinentyp auf `arch.default_machine()` fällt.

**Mögliche Behebung:** die Architektur neben dem Fähigkeitssatz merken und
nur filtern, wenn sie zur VM passt; sonst `None` übergeben, was alles erlaubt.

### 4. Die Firmware-Migration ist einseitig — gehört in eine Release-Notiz

`src/config.rs`, `migrate_firmware`. Das alte Feld `bios_uefi` wird gelesen
und nach `Firmware::Bios { rom }` übernommen, ist aber `skip_serializing`:
beim ersten Speichern verschwindet es aus der Datei. Das ist gewollt und
verliert keinen Wert — wer aber auf einen Vor-Teil-2-Build zurückgeht,
verliert den ROM-Pfad.

---

## Für Teil 4 vorgemerkt

### Das harte Beenden stützt sich auf `taskkill`

`VmProcess::kill`. Im Testlauf erscheinen `Der Prozess "<pid>" wurde nicht
gefunden` und `Dieser Vorgang wird nicht unterstützt` auf der Konsole: der
Rückgabewert wird zwar ausgewertet, die Ausgabe des Werkzeugs aber nicht
abgefangen. Gehört zum sauberen Herunterfahren über QMP, das Teil 4 baut
(erst herunterfahren, hartes Töten als zweiter ausdrücklicher Schritt).
