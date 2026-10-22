# FARO QEM (C) R.Trunk 2026 - Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a native, modern, responsive desktop GUI for QEMU in Rust titled **FARO QEM (C) R.Trunk 2026**, featuring VM profile management, hardware & storage configuration, native file pickers, multilingual localization (EN, DE, FR), modern typography, and real-time process log streaming.

**Architecture:** Master-detail UI built on `eframe`/`egui`. Dedicated background threads monitor QEMU child processes (`std::process::Command`) and pipe real-time stdout/stderr lines through `mpsc` channels directly into the GUI console with `ctx.request_repaint()`. Configurations and localizations are persisted as JSON via `serde_json`.

**Tech Stack:** Rust (2021 Edition), `eframe` / `egui` 0.29, `serde` / `serde_json`, `uuid`, `rfd` (native file dialogs), `shell-words`, `directories`.

**Spec:** [`docs/superpowers/specs/2026-10-22-faro-qem-design.md`](file:///E:/gemini/quemugui/docs/superpowers/specs/2026-10-22-faro-qem-design.md)

## Global Constraints
- App Title: `FARO QEM (C) R.Trunk 2026`
- Languages: English (`en`), German (`de`), French (`fr`) using JSON files
- UI Layout: Master-detail (Sidebar VM list on left, tabbed configuration & live console on right)
- Storage & Execution: Robust cross-platform paths using `std::path::PathBuf`
- Process Streaming: Zero GUI freeze, background threads with channel events

---

### Task 1: Cargo Project Scaffolding & Dependencies

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `assets/locales/en.json`
- Create: `assets/locales/de.json`
- Create: `assets/locales/fr.json`
- Create: `src/main.rs` (minimal placeholder)

**Interfaces:**
- Produces: Base Cargo workspace compile-ready with dependencies.

- [ ] **Step 1: Write Cargo.toml with all required dependencies**

```toml
[package]
name = "faro-qem"
version = "0.1.0"
edition = "2021"
authors = ["R.Trunk"]
description = "FARO QEM (C) R.Trunk 2026 - Modern QEMU GUI in Rust"

[dependencies]
eframe = { version = "0.29.1", default-features = false, features = ["default_fonts", "glow", "wayland", "x11"] }
egui = "0.29.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.10", features = ["v4", "serde"] }
rfd = "0.15"
shell-words = "1.1"
directories = "5.0"

[dev-dependencies]
tempfile = "3.12"
```

- [ ] **Step 2: Create .gitignore**

```text
/target
Cargo.lock
*.tmp
vms.json
settings.json
```

- [ ] **Step 3: Create initial minimal locale files in assets/locales/**

Create `assets/locales/en.json`:
```json
{
  "app_title": "FARO QEM",
  "app_copyright": "(C) R.Trunk 2026",
  "new_vm": "New VM",
  "duplicate": "Duplicate",
  "delete": "Delete",
  "settings": "Settings",
  "launch": "Launch VM",
  "stop": "Force Stop",
  "save": "Save Profile",
  "status_stopped": "Stopped",
  "status_running": "Running",
  "tab_system": "System & Hardware",
  "tab_storage": "Storage & Media",
  "tab_display": "Display & Graphics",
  "tab_network": "Network & Advanced",
  "tab_console": "Live Console"
}
```

Create `assets/locales/de.json`:
```json
{
  "app_title": "FARO QEM",
  "app_copyright": "(C) R.Trunk 2026",
  "new_vm": "Neue VM",
  "duplicate": "Duplizieren",
  "delete": "Löschen",
  "settings": "Einstellungen",
  "launch": "VM Starten",
  "stop": "Stoppen erzwingen",
  "save": "Profil speichern",
  "status_stopped": "Gestoppt",
  "status_running": "Aktiv",
  "tab_system": "System & Hardware",
  "tab_storage": "Speicher & Medien",
  "tab_display": "Anzeige & Grafik",
  "tab_network": "Netzwerk & Erweitert",
  "tab_console": "Echtzeit-Konsole"
}
```

Create `assets/locales/fr.json`:
```json
{
  "app_title": "FARO QEM",
  "app_copyright": "(C) R.Trunk 2026",
  "new_vm": "Nouvelle MV",
  "duplicate": "Dupliquer",
  "delete": "Supprimer",
  "settings": "Paramètres",
  "launch": "Lancer MV",
  "stop": "Arrêt forcé",
  "save": "Sauvegarder",
  "status_stopped": "Arrêté",
  "status_running": "En cours",
  "tab_system": "Système & Matériel",
  "tab_storage": "Stockage & Médias",
  "tab_display": "Affichage & Graphismes",
  "tab_network": "Réseau & Avancé",
  "tab_console": "Console en direct"
}
```

- [ ] **Step 4: Verify Cargo check compiles dependencies**

Run: `cargo check`
Expected: Compiles and finishes with exit code 0.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml .gitignore assets/ src/main.rs
git commit -m "chore: scaffold project structure and dependencies"
```

---

### Task 2: Configuration Models & Persistence (`config.rs`)

**Files:**
- Create: `src/config.rs`
- Test: `tests/test_config.rs`

**Interfaces:**
- Produces:
  - `pub enum Arch { X86_64, I386, Aarch64, Riscv64, Arm }`
  - `pub enum Accelerator { Auto, WHPX, HAXM, KVM, TCG }`
  - `pub enum DriveFormat { Qcow2, Raw, Vmdk }`
  - `pub enum DriveInterface { VirtIO, Ide, Sata, Nvme, Scsi }`
  - `pub struct DriveConfig { pub path: PathBuf, pub format: DriveFormat, pub interface: DriveInterface, pub boot_index: Option<u32> }`
  - `pub struct PortForwardRule { pub host_port: u16, pub guest_port: u16, pub proto: String }`
  - `pub struct NetworkConfig { pub backend: String, pub nic_model: String, pub port_forwards: Vec<PortForwardRule> }`
  - `pub struct DisplayConfig { pub vga_type: String, pub display_backend: String, pub fullscreen: bool }`
  - `pub struct VmConfig { pub id: Uuid, pub name: String, pub arch: Arch, pub cpu_cores: u32, pub memory_mb: u32, pub accel: Accelerator, pub drives: Vec<DriveConfig>, pub cdroms: Vec<PathBuf>, pub display: DisplayConfig, pub network: NetworkConfig, pub extra_args: String, ... }`
  - `pub struct AppSettings { pub qemu_dir: Option<PathBuf>, pub language: String, pub theme: String }`
  - `pub fn load_vms(path: &Path) -> Vec<VmConfig>`
  - `pub fn save_vms(path: &Path, vms: &[VmConfig]) -> Result<(), String>`
  - `pub fn load_settings(path: &Path) -> AppSettings`
  - `pub fn save_settings(path: &Path, settings: &AppSettings) -> Result<(), String>`

- [ ] **Step 1: Write unit tests for configuration serialization and deserialization**

Create `tests/test_config.rs`:
```rust
use std::path::PathBuf;
use faro_qem::config::{VmConfig, Arch, Accelerator, DriveFormat, DriveInterface, DriveConfig};

#[test]
fn test_vm_config_default_and_roundtrip() {
    let vm = VmConfig::new_default("Test VM", Arch::X86_64);
    assert_eq!(vm.name, "Test VM");
    assert_eq!(vm.cpu_cores, 2);
    assert_eq!(vm.memory_mb, 2048);

    let json = serde_json::to_string_pretty(&vm).expect("serialize");
    let deserialized: VmConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.id, vm.id);
    assert_eq!(deserialized.name, vm.name);
}

#[test]
fn test_drive_config_serialization() {
    let drive = DriveConfig {
        path: PathBuf::from("disk.qcow2"),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: Some(1),
    };
    let json = serde_json::to_string(&drive).unwrap();
    let parsed: DriveConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.format, DriveFormat::Qcow2);
    assert_eq!(parsed.interface, DriveInterface::VirtIO);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_config`
Expected: Compilation failure because `faro_qem::config` does not exist yet.

- [ ] **Step 3: Implement `src/config.rs` and expose `src/lib.rs`**

Create `src/lib.rs`:
```rust
pub mod config;
pub mod i18n;
pub mod qemu;
pub mod ui;
pub mod app;
```

Create `src/config.rs` with comprehensive structs, serde derives, default initializers, and JSON loading/saving functions with atomic write.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_config`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/config.rs tests/test_config.rs
git commit -m "feat: implement VM and app configuration models and persistence"
```

---

### Task 3: Localization Engine (`i18n.rs`)

**Files:**
- Create: `src/i18n.rs`
- Expand: `assets/locales/en.json`, `assets/locales/de.json`, `assets/locales/fr.json`
- Test: `tests/test_i18n.rs`

**Interfaces:**
- Produces:
  - `pub struct I18n { current_locale: String, catalogs: HashMap<String, HashMap<String, String>> }`
  - `pub fn I18n::new(locale: &str) -> Self`
  - `pub fn t(&self, key: &str) -> &str`
  - `pub fn set_locale(&mut self, locale: &str)`
  - `pub fn current_locale(&self) -> &str`
  - `pub fn available_locales(&self) -> Vec<(&str, &str)>` (returns `[("en", "English"), ("de", "Deutsch"), ("fr", "Français")]`)

- [ ] **Step 1: Write test for I18n translation lookups and fallbacks**

Create `tests/test_i18n.rs`:
```rust
use faro_qem::i18n::I18n;

#[test]
fn test_i18n_locales_and_translations() {
    let mut i18n = I18n::new("en");
    assert_eq!(i18n.t("new_vm"), "New VM");
    assert_eq!(i18n.t("launch"), "Launch VM");

    i18n.set_locale("de");
    assert_eq!(i18n.t("new_vm"), "Neue VM");
    assert_eq!(i18n.t("launch"), "VM Starten");

    i18n.set_locale("fr");
    assert_eq!(i18n.t("new_vm"), "Nouvelle MV");
    assert_eq!(i18n.t("launch"), "Lancer MV");
}

#[test]
fn test_i18n_fallback_to_key_or_en() {
    let i18n = I18n::new("de");
    // If key does not exist, return key
    assert_eq!(i18n.t("non_existent_key"), "non_existent_key");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_i18n`
Expected: Compilation failure because `faro_qem::i18n` does not exist.

- [ ] **Step 3: Implement `src/i18n.rs` and complete translations across EN, DE, FR**

Use `include_str!` to bundle the default `assets/locales/en.json`, `de.json`, and `fr.json` into the binary so it has zero external file dependencies at runtime while supporting external overrides.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_i18n`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/i18n.rs assets/locales/ tests/test_i18n.rs
git commit -m "feat: implement JSON-based localization engine for EN, DE, FR"
```

---

### Task 4: QEMU Binary Resolver & CLI Argument Builder (`qemu.rs`)

**Files:**
- Create: `src/qemu.rs`
- Test: `tests/test_qemu_args.rs`

**Interfaces:**
- Produces:
  - `pub fn find_qemu_binary(custom_dir: Option<&Path>, arch: Arch) -> Option<PathBuf>`
  - `pub fn find_qemu_img(custom_dir: Option<&Path>) -> Option<PathBuf>`
  - `pub fn build_qemu_args(vm: &VmConfig) -> Vec<String>`
  - `pub fn create_disk_image(qemu_img: &Path, path: &Path, format: DriveFormat, size_gb: u32) -> Result<(), String>`

- [ ] **Step 1: Write unit tests for command line argument building**

Create `tests/test_qemu_args.rs`:
```rust
use std::path::PathBuf;
use faro_qem::config::{VmConfig, Arch, Accelerator, DriveConfig, DriveFormat, DriveInterface, PortForwardRule};
use faro_qem::qemu::build_qemu_args;

#[test]
fn test_build_qemu_args_basic() {
    let mut vm = VmConfig::new_default("Test Linux", Arch::X86_64);
    vm.cpu_cores = 4;
    vm.memory_mb = 4096;
    vm.accel = Accelerator::TCG;
    vm.extra_args = "-usb -device usb-tablet".to_string();

    let args = build_qemu_args(&vm);
    assert!(args.contains(&"-m".to_string()));
    assert!(args.contains(&"4096M".to_string()));
    assert!(args.contains(&"-smp".to_string()));
    assert!(args.contains(&"4".to_string()));
    assert!(args.contains(&"-usb".to_string()));
    assert!(args.contains(&"usb-tablet".to_string()));
}

#[test]
fn test_build_qemu_args_drives_and_net() {
    let mut vm = VmConfig::new_default("Win11", Arch::X86_64);
    vm.drives.push(DriveConfig {
        path: PathBuf::from("disk.qcow2"),
        format: DriveFormat::Qcow2,
        interface: DriveInterface::VirtIO,
        boot_index: Some(1),
    });
    vm.network.port_forwards.push(PortForwardRule {
        host_port: 8080,
        guest_port: 80,
        proto: "tcp".to_string(),
    });

    let args = build_qemu_args(&vm);
    let joined = args.join(" ");
    assert!(joined.contains("format=qcow2"));
    assert!(joined.contains("hostfwd=tcp::8080-:80"));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --test test_qemu_args`
Expected: Fail to compile.

- [ ] **Step 3: Implement `src/qemu.rs` argument builder and binary resolver**

Implement:
- Standard paths lookup for Windows (`C:\Program Files\qemu`, etc.) and Linux (`/usr/bin`, etc.)
- Argument vector creation with proper escaping
- Safe parsing of `extra_args` via `shell_words::split`
- Disk creation invocation via `std::process::Command` calling `qemu-img`

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_qemu_args`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/qemu.rs tests/test_qemu_args.rs
git commit -m "feat: implement QEMU binary discovery and CLI argument builder"
```

---

### Task 5: QEMU Process Supervisor & Background Log Streaming (`qemu.rs`)

**Files:**
- Modify: `src/qemu.rs`
- Test: `tests/test_process_supervisor.rs`

**Interfaces:**
- Produces:
  - `pub enum QemuEvent { Stdout(String), Stderr(String), Started(u32), Exited(Option<i32>), Error(String) }`
  - `pub struct VmProcess { pid: u32, kill_tx: std::sync::mpsc::Sender<()>, ... }`
  - `pub fn spawn_qemu(binary: &Path, args: &[String], event_tx: std::sync::mpsc::Sender<QemuEvent>) -> Result<VmProcess, String>`
  - `impl VmProcess { pub fn kill(&mut self) -> Result<(), String>; pub fn is_alive(&self) -> bool; }`

- [ ] **Step 1: Write test for process spawning and line streaming using a mock binary**

Create `tests/test_process_supervisor.rs` testing standard command execution and line output streaming over channels.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test test_process_supervisor`
Expected: FAIL.

- [ ] **Step 3: Implement process runner and channel streamer in `src/qemu.rs`**

Implement:
- Thread spawning with `std::io::BufReader` over `child.stdout.take()` and `child.stderr.take()`.
- Waiter thread calling `child.wait()` to emit `QemuEvent::Exited`.
- Cross-platform process killing handle (`child.kill()`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test test_process_supervisor`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/qemu.rs tests/test_process_supervisor.rs
git commit -m "feat: implement threaded QEMU process supervisor with real-time log streaming"
```

---

### Task 6: Modern Typography, Icons & Theme Engine (`src/ui/theme.rs`)

**Files:**
- Create: `src/ui/mod.rs`
- Create: `src/ui/theme.rs`

**Interfaces:**
- Produces:
  - `pub fn configure_modern_fonts(ctx: &egui::Context)`
  - `pub fn apply_theme(ctx: &egui::Context, theme: &str)`
  - Style constants for Status Badges, Tabs, Headers, Action Buttons.

- [ ] **Step 1: Implement `src/ui/theme.rs`**
  Configure modern crisp typography using system font families (Segoe UI, Inter, Cantarell, Roboto) in `egui::FontDefinitions`. Configure Dark and Light visuals, customized frame paddings, rounded corners, and badge colors.
- [ ] **Step 2: Create `src/ui/mod.rs` re-exporting theme**
- [ ] **Step 3: Run `cargo check` to verify syntax and type correctness**
- [ ] **Step 4: Commit**

```bash
git add src/ui/
git commit -m "feat: add modern font configuration and theme visual styling"
```

---

### Task 7: UI Components - Sidebar & Settings Modal (`src/ui/sidebar.rs`, `src/ui/settings.rs`)

**Files:**
- Create: `src/ui/sidebar.rs`
- Create: `src/ui/settings.rs`
- Modify: `src/ui/mod.rs`

**Interfaces:**
- Produces:
  - `pub fn render_sidebar(ui: &mut egui::Ui, app_state: &mut App)`
  - `pub fn render_settings_dialog(ctx: &egui::Context, app_state: &mut App)`
  - New VM dialog, Duplicate VM, Delete VM confirmation, Search filtering.

- [ ] **Step 1: Implement `src/ui/sidebar.rs`**
  - App header with `FARO QEM` and `(C) R.Trunk 2026`.
  - Search filter input.
  - Scrollable VM list showing Name, Arch, and live status badge (`● Stopped`, `● Running (PID: 1234)`, `⚠ Error`).
  - Bottom toolbar: `[+ New VM]`, `[Duplicate]`, `[Delete]`, `[⚙ Settings]`.
- [ ] **Step 2: Implement `src/ui/settings.rs`**
  - Modal window for App Settings:
    - Language selector dropdown (English, Deutsch, Français) triggering dynamic `i18n.set_locale(...)`.
    - Theme selector (Dark / Light) triggering `apply_theme(...)`.
    - QEMU executable directory input with native folder picker button (`rfd::FileDialog::pick_folder`).
- [ ] **Step 3: Verify compilation with `cargo check`**
- [ ] **Step 4: Commit**

```bash
git add src/ui/sidebar.rs src/ui/settings.rs src/ui/mod.rs
git commit -m "feat: implement sidebar navigation and settings dialog with language/theme selector"
```

---

### Task 8: UI Components - VM Detail Tabs & Hardware Controls (`src/ui/vm_detail.rs`)

**Files:**
- Create: `src/ui/vm_detail.rs`
- Modify: `src/ui/mod.rs`

**Interfaces:**
- Produces:
  - `pub fn render_vm_detail(ui: &mut egui::Ui, app_state: &mut App)`
  - Tab 1: System & CPU/RAM
  - Tab 2: Storage & Media (with `rfd` file pickers & `qemu-img create` dialog)
  - Tab 3: Display & Graphics
  - Tab 4: Network & Advanced

- [ ] **Step 1: Implement `src/ui/vm_detail.rs`**
  - **Top Action Bar**:
    - VM Name text editor.
    - Status pill & quick spec summary.
    - `▶ Launch VM` (green button, disabled if running).
    - `⏹ Force Stop` (red button, active if running).
    - `💾 Save` button.
  - **Tab 1: System / Hardware**:
    - Architecture dropdown (`x86_64`, `aarch64`, `i386`, `riscv64`, `arm`).
    - Accelerator selector (`Auto`, `WHPX`, `HAXM`, `KVM`, `TCG`).
    - CPU model string, Cores slider (1 to 32).
    - Memory slider & number input with MB/GB conversion.
    - Optional UEFI/OVMF firmware path with file picker.
  - **Tab 2: Storage & Media**:
    - Disks list with format, interface, and remove button.
    - `[+ Add Existing Disk]` button opening `rfd::FileDialog::pick_file`.
    - `[✨ Create New Disk Image]` modal allowing user to set path, format (qcow2/raw), size in GB, and execute `create_disk_image`.
    - CD-ROM / ISO selector with browse and eject buttons.
  - **Tab 3: Display & Graphics**:
    - VGA adapter selector (`virtio-vga`, `std`, `qxl`, `cirrus`, `none`).
    - Display backend (`default`, `gtk`, `sdl`, `none`).
    - Fullscreen checkbox.
  - **Tab 4: Network & Advanced**:
    - Backend (`User`, `Tap`, `Bridge`).
    - NIC model (`virtio-net-pci`, `e1000`, `rtl8139`).
    - Port forwarding rules table: Add/Remove rows (Host port -> Guest port, TCP/UDP).
    - Extra arguments multi-line text input with syntax tip.
- [ ] **Step 2: Verify compilation with `cargo check`**
- [ ] **Step 3: Commit**

```bash
git add src/ui/vm_detail.rs src/ui/mod.rs
git commit -m "feat: implement VM detail tabs for hardware, storage, display, and network"
```

---

### Task 9: UI Components - Live Console & Logs Viewer (`src/ui/console.rs`)

**Files:**
- Create: `src/ui/console.rs`
- Modify: `src/ui/mod.rs`

**Interfaces:**
- Produces:
  - `pub fn render_console(ui: &mut egui::Ui, app_state: &mut App)`
  - Real-time streaming log lines viewer.
  - ANSI/color-coded stdout vs stderr rendering.
  - CLI command preview, Clear, and Copy buttons.

- [ ] **Step 1: Implement `src/ui/console.rs`**
  - Header with current running state and PID.
  - Expandable CLI command preview showing the exact `qemu-system-...` command constructed.
  - Auto-scrolling text view with colored log items:
    - Standard stdout in neutral light-gray / white text.
    - Stderr in warning orange / error red text.
    - System events (`[Started PID 1234]`, `[Process exited with code 0]`) in cyan/blue text.
  - Toolbar buttons: `[Copy Logs]`, `[Clear Console]`, `[Auto-scroll Checkbox]`.
- [ ] **Step 2: Verify compilation with `cargo check`**
- [ ] **Step 3: Commit**

```bash
git add src/ui/console.rs src/ui/mod.rs
git commit -m "feat: implement real-time live console log streaming viewer"
```

---

### Task 10: App State Integration & End-to-End Verification (`src/app.rs`, `src/main.rs`)

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs`
- Test: Full cargo build and test suite execution

**Interfaces:**
- Produces:
  - `pub struct FaroQemApp: eframe::App`
  - Complete application entry point launching native window.

- [ ] **Step 1: Implement `src/app.rs`**
  - Struct `FaroQemApp` holding `AppSettings`, `Vec<VmConfig>`, `active_vm_id`, `I18n`, `ConsoleLogBuffer`, and `Option<VmProcess>`.
  - Process event receiver loop inside `update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame)`: drains `mpsc::Receiver<QemuEvent>` non-blockingly each frame, appends to log buffer, and updates VM status.
  - Central panel master-detail layout combining Sidebar and VM Detail / Console.
  - Automatic saving of VMs and Settings on change / exit.
- [ ] **Step 2: Implement `src/main.rs`**
  - Sets up `eframe::NativeOptions` (window size 1100x720, centered, native icon/title `FARO QEM (C) R.Trunk 2026`).
  - Calls `eframe::run_native`.
- [ ] **Step 3: Run full verification suite**

Run: `cargo test`
Expected: All unit and integration tests pass.

Run: `cargo check`
Expected: Clean build with zero errors.

- [ ] **Step 4: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat: complete FARO QEM application integration and main loop"
```
