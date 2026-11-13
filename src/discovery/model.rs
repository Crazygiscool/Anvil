use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceKind {
    Physical,
    Virtual,
    Loopback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MountDiscovery {
    pub device: String,
    pub target: PathBuf,
    pub filesystem: String,
    pub options: String,
    /// UI-facing alias for `target` (mount point).
    #[serde(skip_serializing)]
    pub mount_point: PathBuf,
    /// UI-facing alias for `filesystem` (fs type).
    #[serde(skip_serializing)]
    pub fs_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinaryDiscovery {
    pub name: String,
    pub path: Option<PathBuf>,
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub source: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ZfsDiscovery {
    pub present: bool,
    pub zpool_binary: Option<String>,
    pub zfs_binary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BtrfsDiscovery {
    pub present: bool,
    pub filesystem_candidates: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkInterface {
    pub name: String,
    pub kind: InterfaceKind,
    pub wireless: bool,
    pub driver_present: bool,
    pub device_present: bool,
    pub address_present: bool,
    pub stats_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StorageDiscovery {
    pub supported_filesystems: Vec<String>,
    pub mounts: Vec<MountDiscovery>,
    pub zfs: ZfsDiscovery,
    pub btrfs: BtrfsDiscovery,
    pub lvm_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerRuntime {
    pub name: String,
    pub socket: PathBuf,
    pub present: bool,
    pub accessible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitDiscovery {
    pub kind: String,
    pub detected: bool,
    pub markers: Vec<PathBuf>,
    pub service_manager_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeDiscovery {
    pub container_runtimes: Vec<ContainerRuntime>,
    pub init: InitDiscovery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NetworkDiscovery {
    pub interfaces: Vec<NetworkInterface>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveryReport {
    pub schema_version: u32,
    pub storage: StorageDiscovery,
    pub network: NetworkDiscovery,
    pub runtime: RuntimeDiscovery,
    pub binaries: Vec<BinaryDiscovery>,
    pub diagnostics: Vec<Diagnostic>,
}

impl DiscoveryReport {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            storage: StorageDiscovery {
                supported_filesystems: Vec::new(),
                mounts: Vec::new(),
                zfs: ZfsDiscovery {
                    present: false,
                    zpool_binary: None,
                    zfs_binary: None,
                },
                btrfs: BtrfsDiscovery {
                    present: false,
                    filesystem_candidates: Vec::new(),
                },
                lvm_present: false,
            },
            network: NetworkDiscovery {
                interfaces: Vec::new(),
            },
            runtime: RuntimeDiscovery {
                container_runtimes: Vec::new(),
                init: InitDiscovery {
                    kind: "unknown".to_string(),
                    detected: false,
                    markers: Vec::new(),
                    service_manager_version: None,
                },
            },
            binaries: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}
