use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// Supported CPU architectures for QEMU emulation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Arch {
    #[serde(rename = "x86_64", alias = "X86_64")]
    #[default]
    X86_64,
    #[serde(rename = "i386", alias = "I386")]
    I386,
    #[serde(rename = "aarch64", alias = "Aarch64")]
    Aarch64,
    #[serde(rename = "riscv64", alias = "Riscv64")]
    Riscv64,
    #[serde(rename = "arm", alias = "Arm")]
    Arm,
}

impl Arch {
    pub fn as_str(&self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::I386 => "i386",
            Arch::Aarch64 => "aarch64",
            Arch::Riscv64 => "riscv64",
            Arch::Arm => "arm",
        }
    }

    pub fn qemu_binary_suffix(&self) -> &'static str {
        self.as_str()
    }

    pub fn default_machine(&self) -> &'static str {
        match self {
            Arch::X86_64 | Arch::I386 => "q35",
            Arch::Aarch64 | Arch::Arm | Arch::Riscv64 => "virt",
        }
    }

    pub fn default_cpu(&self) -> &'static str {
        match self {
            Arch::X86_64 | Arch::I386 => "max",
            Arch::Aarch64 => "max",
            Arch::Riscv64 => "rv64",
            Arch::Arm => "cortex-a15",
        }
    }

    pub fn all() -> &'static [Arch] {
        &[
            Arch::X86_64,
            Arch::I386,
            Arch::Aarch64,
            Arch::Riscv64,
            Arch::Arm,
        ]
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Hardware accelerators supported across operating systems
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Accelerator {
    #[serde(rename = "auto", alias = "Auto")]
    #[default]
    Auto,
    #[serde(rename = "whpx", alias = "WHPX")]
    WHPX,
    #[serde(rename = "haxm", alias = "HAXM")]
    HAXM,
    #[serde(rename = "kvm", alias = "KVM")]
    KVM,
    #[serde(rename = "tcg", alias = "TCG")]
    TCG,
}

impl Accelerator {
    pub fn as_str(&self) -> &'static str {
        match self {
            Accelerator::Auto => "auto",
            Accelerator::WHPX => "whpx",
            Accelerator::HAXM => "haxm",
            Accelerator::KVM => "kvm",
            Accelerator::TCG => "tcg",
        }
    }

    pub fn all() -> &'static [Accelerator] {
        &[
            Accelerator::Auto,
            Accelerator::WHPX,
            Accelerator::HAXM,
            Accelerator::KVM,
            Accelerator::TCG,
        ]
    }
}

impl fmt::Display for Accelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Disk image formats supported by QEMU
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DriveFormat {
    #[serde(rename = "qcow2", alias = "Qcow2")]
    #[default]
    Qcow2,
    #[serde(rename = "raw", alias = "Raw")]
    Raw,
    #[serde(rename = "vmdk", alias = "Vmdk")]
    Vmdk,
}

impl DriveFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            DriveFormat::Qcow2 => "qcow2",
            DriveFormat::Raw => "raw",
            DriveFormat::Vmdk => "vmdk",
        }
    }

    pub fn all() -> &'static [DriveFormat] {
        &[DriveFormat::Qcow2, DriveFormat::Raw, DriveFormat::Vmdk]
    }
}

impl fmt::Display for DriveFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Interface bus types for virtual drives
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DriveInterface {
    #[serde(rename = "virtio", alias = "VirtIO")]
    #[default]
    VirtIO,
    #[serde(rename = "ide", alias = "Ide")]
    Ide,
    #[serde(rename = "sata", alias = "Sata")]
    Sata,
    #[serde(rename = "nvme", alias = "Nvme")]
    Nvme,
    #[serde(rename = "scsi", alias = "Scsi")]
    Scsi,
}

impl DriveInterface {
    pub fn as_str(&self) -> &'static str {
        match self {
            DriveInterface::VirtIO => "virtio",
            DriveInterface::Ide => "ide",
            DriveInterface::Sata => "sata",
            DriveInterface::Nvme => "nvme",
            DriveInterface::Scsi => "scsi",
        }
    }

    pub fn all() -> &'static [DriveInterface] {
        &[
            DriveInterface::VirtIO,
            DriveInterface::Ide,
            DriveInterface::Sata,
            DriveInterface::Nvme,
            DriveInterface::Scsi,
        ]
    }
}

impl fmt::Display for DriveInterface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Drive configuration attached to a virtual machine
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveConfig {
    pub path: PathBuf,
    pub format: DriveFormat,
    pub interface: DriveInterface,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_index: Option<u32>,
}

/// User-mode networking port forwarding rule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForwardRule {
    pub host_port: u16,
    pub guest_port: u16,
    #[serde(default = "default_proto")]
    pub proto: String,
}

fn default_proto() -> String {
    "tcp".to_string()
}

/// Network interface configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_net_backend")]
    pub backend: String,
    #[serde(default = "default_nic_model")]
    pub nic_model: String,
    #[serde(default)]
    pub port_forwards: Vec<PortForwardRule>,
}

fn default_net_backend() -> String {
    "user".to_string()
}

fn default_nic_model() -> String {
    "virtio-net-pci".to_string()
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            backend: default_net_backend(),
            nic_model: default_nic_model(),
            port_forwards: Vec::new(),
        }
    }
}

/// Display and graphics adapter configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayConfig {
    #[serde(default = "default_vga_type")]
    pub vga_type: String,
    #[serde(default = "default_display_backend")]
    pub display_backend: String,
    #[serde(default)]
    pub fullscreen: bool,
}

fn default_vga_type() -> String {
    "virtio-vga".to_string()
}

fn default_display_backend() -> String {
    "default".to_string()
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            vga_type: default_vga_type(),
            display_backend: default_display_backend(),
            fullscreen: false,
        }
    }
}

/// Complete configuration profile for a Virtual Machine
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmConfig {
    pub id: Uuid,
    pub name: String,
    pub arch: Arch,
    #[serde(default)]
    pub machine_type: String,
    #[serde(default)]
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub memory_mb: u32,
    #[serde(default)]
    pub accel: Accelerator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bios_uefi: Option<PathBuf>,
    #[serde(default)]
    pub drives: Vec<DriveConfig>,
    #[serde(default)]
    pub cdroms: Vec<PathBuf>,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub extra_args: String,
}

impl VmConfig {
    /// Create a new VM configuration with sensible defaults for the selected architecture
    pub fn new_default(name: impl Into<String>, arch: Arch) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            arch,
            machine_type: arch.default_machine().to_string(),
            cpu_model: arch.default_cpu().to_string(),
            cpu_cores: 2,
            memory_mb: 2048,
            accel: Accelerator::Auto,
            bios_uefi: None,
            drives: Vec::new(),
            cdroms: Vec::new(),
            display: DisplayConfig::default(),
            network: NetworkConfig::default(),
            extra_args: String::new(),
        }
    }
}

impl Default for VmConfig {
    fn default() -> Self {
        Self::new_default("Debian 12", Arch::X86_64)
    }
}

/// Global application settings
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qemu_dir: Option<PathBuf>,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub default_arch: Arch,
}

fn default_language() -> String {
    "en".to_string()
}

fn default_theme() -> String {
    "dark".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            qemu_dir: None,
            language: default_language(),
            theme: default_theme(),
            default_arch: Arch::X86_64,
        }
    }
}

/// Resolve the default directory for application configurations
pub fn default_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("faro-qem");
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("faro-qem");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".config").join("faro-qem");
        }
    }
    PathBuf::from(".")
}

/// Resolve the default path for `vms.json`
pub fn default_vms_path() -> PathBuf {
    default_config_dir().join("vms.json")
}

/// Resolve the default path for `settings.json`
pub fn default_settings_path() -> PathBuf {
    default_config_dir().join("settings.json")
}

/// Why a configuration file could not be loaded.
///
/// The distinction matters: `Missing` is a normal first start and may be
/// answered with defaults, while `Unreadable` and `Corrupt` mean a file exists
/// that the user cares about. Replacing one of those destroys their work.
#[derive(Debug)]
pub enum ConfigLoadError {
    /// Nothing to load: no such file, or a file with no content at all. A
    /// zero-byte file is an aborted write, not a configuration.
    Missing,
    /// The file exists but could not be read.
    Unreadable(std::io::Error),
    /// The file was read but does not parse as this configuration.
    Corrupt { detail: String },
}

impl fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigLoadError::Missing => write!(f, "no configuration file present"),
            ConfigLoadError::Unreadable(e) => write!(f, "{}", e),
            ConfigLoadError::Corrupt { detail } => write!(f, "{}", detail),
        }
    }
}

/// Read a configuration file into a string, separating absence from read errors.
fn read_config_text(path: &Path) -> Result<String, ConfigLoadError> {
    if !path.exists() {
        return Err(ConfigLoadError::Missing);
    }
    let content = std::fs::read_to_string(path).map_err(ConfigLoadError::Unreadable)?;
    if content.trim().is_empty() {
        return Err(ConfigLoadError::Missing);
    }
    Ok(content)
}

/// Load the list of VMs, reporting why it could not be loaded.
pub fn try_load_vms(path: &Path) -> Result<Vec<VmConfig>, ConfigLoadError> {
    let content = read_config_text(path)?;
    serde_json::from_str(&content).map_err(|e| ConfigLoadError::Corrupt {
        detail: e.to_string(),
    })
}

/// Load application settings, reporting why they could not be loaded.
pub fn try_load_settings(path: &Path) -> Result<AppSettings, ConfigLoadError> {
    let content = read_config_text(path)?;
    serde_json::from_str(&content).map_err(|e| ConfigLoadError::Corrupt {
        detail: e.to_string(),
    })
}

/// Load the list of VMs from a JSON file. Returns an empty list if missing or corrupted.
///
/// Lenient: it cannot tell a first start from a damaged file, so a caller that
/// writes on "empty" will overwrite a damaged one. New code should use
/// [`try_load_vms`] and preserve what it cannot read.
pub fn load_vms(path: &Path) -> Vec<VmConfig> {
    try_load_vms(path).unwrap_or_default()
}

/// Save the list of VMs to a JSON file.
pub fn save_vms(path: &Path, vms: &[VmConfig]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(vms)
        .map_err(|e| format!("Failed to serialize VM configurations: {}", e))?;
    write_config_file(path, json.as_bytes())
}

/// Load application settings from a JSON file. Returns defaults if missing or corrupted.
///
/// Lenient in the same way as [`load_vms`]; prefer [`try_load_settings`].
pub fn load_settings(path: &Path) -> AppSettings {
    try_load_settings(path).unwrap_or_default()
}

/// Save application settings to a JSON file.
pub fn save_settings(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize app settings: {}", e))?;
    write_config_file(path, json.as_bytes())
}

/// Build a unique staging path next to `path`, used for an atomic write.
///
/// The staging file must live in the same directory as the target so that the
/// final rename stays on one volume.
fn temp_sibling_path(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);

    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".tmp-{}-{}", std::process::id(), seq));

    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}

/// Write data to a target path, creating parent directories if needed.
///
/// The payload goes to a staging file first and is then renamed over the
/// target, so a concurrent reader always sees either the previous file or the
/// complete new one — never a truncated one. Writing straight to the target
/// would empty it before refilling it, and a crash or a reader in that window
/// would find a torn configuration.
fn write_config_file(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }
    }

    let staging = temp_sibling_path(path);
    std::fs::write(&staging, data)
        .map_err(|e| format!("Failed to write {}: {}", staging.display(), e))?;

    std::fs::rename(&staging, path).map_err(|e| {
        let _ = std::fs::remove_file(&staging);
        format!(
            "Failed to move {} onto {}: {}",
            staging.display(),
            path.display(),
            e
        )
    })
}
