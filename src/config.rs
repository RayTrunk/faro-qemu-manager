use std::fmt;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
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
            Arch::X86_64 | Arch::I386 => "host",
            Arch::Aarch64 => "host",
            Arch::Riscv64 => "rv64",
            Arch::Arm => "cortex-a15",
        }
    }

    pub fn all() -> &'static [Arch] {
        &[Arch::X86_64, Arch::I386, Arch::Aarch64, Arch::Riscv64, Arch::Arm]
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
    if let Some(proj) = directories::ProjectDirs::from("com", "rtrunk", "faro-qem") {
        proj.config_dir().to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

/// Resolve the default path for `vms.json`
pub fn default_vms_path() -> PathBuf {
    default_config_dir().join("vms.json")
}

/// Resolve the default path for `settings.json`
pub fn default_settings_path() -> PathBuf {
    default_config_dir().join("settings.json")
}

/// Load the list of VMs from a JSON file. Returns empty list if missing or corrupted.
pub fn load_vms(path: &Path) -> Vec<VmConfig> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Atomically save the list of VMs to a JSON file.
pub fn save_vms(path: &Path, vms: &[VmConfig]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(vms)
        .map_err(|e| format!("Failed to serialize VM configurations: {}", e))?;
    atomic_write_file(path, json.as_bytes())
}

/// Load application settings from a JSON file. Returns defaults if missing or corrupted.
pub fn load_settings(path: &Path) -> AppSettings {
    if !path.exists() {
        return AppSettings::default();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

/// Atomically save application settings to a JSON file.
pub fn save_settings(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize app settings: {}", e))?;
    atomic_write_file(path, json.as_bytes())
}

/// Helper function to atomically write data to a target path using a temporary file
fn atomic_write_file(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let unique = Uuid::new_v4();
    let temp_path = if let Some(parent) = path.parent() {
        parent.join(format!(".{}_{}.tmp", file_name, unique))
    } else {
        PathBuf::from(format!(".{}_{}.tmp", file_name, unique))
    };

    if let Err(e) = std::fs::write(&temp_path, data) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("Failed to write temporary file {}: {}", temp_path.display(), e));
    }

    #[cfg(not(windows))]
    {
        if let Err(e) = std::fs::rename(&temp_path, path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(format!("Failed to rename temporary file to {}: {}", path.display(), e));
        }
    }

    #[cfg(windows)]
    {
        if let Err(_e) = std::fs::rename(&temp_path, path) {
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
            if let Err(e2) = std::fs::rename(&temp_path, path) {
                let _ = std::fs::remove_file(&temp_path);
                if let Err(e3) = std::fs::write(path, data) {
                    return Err(format!(
                        "Failed to atomic save to {} (rename err: {}, direct write err: {})",
                        path.display(),
                        e2,
                        e3
                    ));
                }
            }
        }
    }

    Ok(())
}
