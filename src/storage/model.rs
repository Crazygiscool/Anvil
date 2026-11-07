use crate::discovery::model::Diagnostic;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ZfsPool {
    pub name: String,
    pub health: String,
    pub size: String,
    pub allocated: String,
    pub free: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ZfsDataset {
    pub name: String,
    pub used: String,
    pub available: String,
    pub referenced: String,
    pub mountpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BtrfsSubvolume {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LvmVolume {
    pub volume_group: String,
    pub name: String,
    pub size: String,
    pub attributes: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StorageInspection {
    pub zfs_pools: Vec<ZfsPool>,
    pub zfs_datasets: Vec<ZfsDataset>,
    pub btrfs_subvolumes: Vec<BtrfsSubvolume>,
    pub lvm_volumes: Vec<LvmVolume>,
    pub diagnostics: Vec<Diagnostic>,
}
