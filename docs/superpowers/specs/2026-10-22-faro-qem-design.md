# FARO QEM (C) R.Trunk 2026 - Specification & Architecture Design

## 1. Overview
**FARO QEM (C) R.Trunk 2026** is a lightweight, modern native desktop GUI for QEMU written in Rust. Inspired by intuitive tools like QEMUWin, it provides a clean, responsive interface for configuring virtual machines, managing disks, networking, CPU/RAM allocations, and launching QEMU with real-time console streaming and process supervision.

## 2. Technology Stack & Key Crates
* **Language:** Rust (2021 Edition, Rust 1.96+)
* **GUI Framework:** `eframe` / `egui` (0.29+) with customized modern typography and theme styling.
* **Serialization:** `serde`, `serde_json` for configuration persistence and localization files.
* **Process Management:** `std::process::Command`, piped `stdout`/`stderr` read across dedicated background reader threads with `std::sync::mpsc` channels and `egui::Context::request_repaint()`.
* **File Dialogs:** `rfd` (Rust File Dialog) for native OS file selection (ISOs, disk images, QEMU binaries).
* **Localization:** JSON-based multi-language engine (`en.json`, `de.json`, `fr.json`) with embedded defaults and dynamic switching.
* **UUID:** `uuid` (v4) for VM identification.

## 3. Architecture & Project Layout

```text
quemugui/
├── Cargo.toml
├── assets/
│   ├── fonts/               # Modern font assets if needed (or bundled system fallbacks)
│   └── locales/
│       ├── en.json          # English translations
│       ├── de.json          # German translations (Deutsch)
│       └── fr.json          # French translations (Français)
├── src/
│   ├── main.rs              # App entry point, eframe initialization & font configuration
│   ├── app.rs               # Main application state and top-level eframe::App loop
│   ├── config.rs            # Data models (VmConfig, AppSettings, DriveConfig, etc.) and persistence
│   ├── i18n.rs              # Localization manager loading en/de/fr JSON catalogs
│   ├── qemu.rs              # QEMU executable discovery, CLI builder, and process supervisor
│   └── ui/
│       ├── mod.rs           # UI module re-exports
│       ├── sidebar.rs       # Left VM list sidebar and toolbar
│       ├── vm_detail.rs     # Right panel containing settings tabs and top launch bar
│       ├── console.rs       # Real-time console log viewer with auto-scroll and ANSI/styling
│       └── settings.rs      # Global settings modal (language, theme, QEMU paths)
└── tests/                   # CLI builder and config round-trip tests
```

## 4. Detailed Component Design

### 4.1 Data Models (`config.rs`)
* **`AppSettings`**:
  * `qemu_dir: Option<PathBuf>`: Path to directory containing `qemu-system-*` and `qemu-img`.
  * `language: String`: `"en"`, `"de"`, or `"fr"`.
  * `theme: String`: `"dark"`, `"light"`.
  * `default_arch: Arch`: Default architecture for new VMs (default: `x86_64`).
* **`VmConfig`**:
  * `id: Uuid`
  * `name: String`
  * `arch: Arch` (`x86_64`, `i386`, `aarch64`, `riscv64`, `arm`)
  * `machine_type: String` (e.g. `q35`, `pc-i440fx`, `virt`)
  * `cpu_model: String` (e.g. `host`, `max`, `qemu64`)
  * `cpu_cores: u32` (1..=64)
  * `memory_mb: u32` (128..=131072)
  * `accel: Accelerator` (`Auto`, `WHPX`, `HAXM`, `KVM`, `TCG`)
  * `bios_uefi: Option<PathBuf>` (OVMF/UEFI firmware file)
  * `drives: Vec<DriveConfig>`:
    * `path: PathBuf`
    * `format: DriveFormat` (`Qcow2`, `Raw`, `Vmdk`)
    * `interface: DriveInterface` (`VirtIO`, `Ide`, `Sata`, `Nvme`, `Scsi`)
    * `boot_index: Option<u32>`
  * `cdroms: Vec<PathBuf>` (ISO files)
  * `display: DisplayConfig`:
    * `vga_type: VgaType` (`VirtioVga`, `Std`, `Qxl`, `Cirrus`, `None`)
    * `display_backend: DisplayBackend` (`Default`, `Gtk`, `Sdl`, `None`)
    * `fullscreen: bool`
  * `network: NetworkConfig`:
    * `backend: NetBackend` (`User`, `Tap`, `Bridge`)
    * `nic_model: String` (`virtio-net-pci`, `e1000`, `rtl8139`)
    * `port_forwards: Vec<PortForwardRule>`: `host_port: u16`, `guest_port: u16`, `proto: String`
  * `extra_args: String` (raw custom CLI arguments)

### 4.2 Localization Engine (`i18n.rs`)
* JSON catalog per locale:
  * Key-value strings for all labels, tooltips, dialogs, status badges, and tab headers.
  * Supported: English (`en`), German (`de`), French (`fr`).
  * Embedded into the binary using `include_str!` as default fallbacks, while also supporting custom override JSONs in `locales/`.
  * Quick lookup method `i18n.t("key") -> &str` with fallback to English key if translation is missing.

### 4.3 Typography & Theming
* Modern font configuration: Load standard clean sans-serif typography (e.g. Segoe UI on Windows / system modern sans, with crisp proportional sizing) into `egui::FontDefinitions`.
* Theme manager: Full Dark / Light mode switching with customized contrast, border radiuses, and badge accents (Running = Emerald Green, Stopped = Gray, Error = Crimson).

### 4.4 QEMU Process Manager & Argument Builder (`qemu.rs`)
* **Executable Discovery**:
  * Scans user-specified `AppSettings.qemu_dir`, system `PATH`, and standard platform locations:
    * Windows: `C:\Program Files\qemu`, `C:\Program Files (x86)\qemu`
    * Linux: `/usr/bin`, `/usr/local/bin`
  * Validates existence of both `qemu-system-<arch>` and `qemu-img`.
* **Argument Builder**:
  * Accel & Machine: `-machine {type},accel={accel}`
  * SMP & Memory: `-smp {cores} -m {mb}M`
  * CPU: `-cpu {cpu_model}`
  * Disks: `-drive file={path},format={fmt},if={iface}` (and `-cdrom` for ISOs)
  * Network: `-netdev user,id=net0,hostfwd=tcp::{hport}-:{gport} -device {nic},netdev=net0`
  * Display: `-vga {vga} -display {backend}`
  * Custom Arguments: Parsed via shell-words rules and appended.
* **Process Lifecycle**:
  * Spawns child process with piped stdout/stderr.
  * Reader threads continuously stream log lines into an `mpsc::channel` (`QemuEvent::Stdout`, `QemuEvent::Stderr`, `QemuEvent::Exited(ExitStatus)`).
  * `ctx.request_repaint()` called on each chunk to keep the UI smooth and responsive.
  * Clean kill support: calls `child.kill()` if force-stop requested.
* **Disk Helper (`qemu-img`)**:
  * Supports executing `qemu-img create -f qcow2 <path> <size_gb>G` directly from the Storage tab.

### 4.5 User Interface Layout (`ui/`)
* **Master-Detail Layout**:
  * **Sidebar (Left)**:
    * App Branding: `FARO QEM (C) R.Trunk 2026`
    * Search filter bar
    * VM list items with status badges and quick select
    * Actions: New VM, Duplicate, Delete, Settings
  * **Detail View (Right)**:
    * Top Bar: VM Name, Specs pill, `Launch VM` (Green), `Force Stop` (Red), `Save`
    * Tab 1: **System / CPU / RAM** (Arch, Machine, Accel, Cores, Memory slider, UEFI)
    * Tab 2: **Storage & Media** (Disks table, Add Disk, Create Disk modal, ISO selector)
    * Tab 3: **Display & Graphics** (VGA model, backend, fullscreen)
    * Tab 4: **Network & Advanced** (Backend, NIC model, Port forwards table, Extra args)
    * Tab 5: **Console & Logs** (Live stdout/stderr stream, CLI preview, Clear, Copy)
  * **Settings Dialog**:
    * Language selector (English, Deutsch, Français)
    * Theme selector (Dark / Light)
    * QEMU executable directory browser

## 5. Verification & Testing Strategy
* **Unit Tests**:
  * Argument builder output tests verifying exact flag generation for various VM profiles.
  * Serialization and deserialization tests for JSON configs.
  * Localization fallback tests ensuring all keys exist across `en`, `de`, and `fr`.
* **Integration Tests / Manual Verification**:
  * Compile and run with `cargo check` / `cargo test` / `cargo run`.
  * Test VM creation, editing, disk creation helper, and argument generation.
