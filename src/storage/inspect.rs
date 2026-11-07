use crate::platform::{CommandRunner, TextReader};
use crate::storage::model::{BtrfsSubvolume, LvmVolume, StorageInspection, ZfsDataset, ZfsPool};
use std::path::{Path, PathBuf};

pub trait StorageBackend: Send + Sync {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> StorageInspection;
}

pub struct ZfsBackend;

impl StorageBackend for ZfsBackend {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> StorageInspection {
        let mut report = StorageInspection::default();

        match find_binary(reader, "zpool") {
            Some(program) => match runner.run(
                &program,
                &["list", "-H", "-o", "name,health,size,alloc,free"],
            ) {
                Ok(output) => {
                    output
                        .stdout
                        .lines()
                        .filter_map(parse_zfs_pool)
                        .for_each(|pool| report.zfs_pools.push(pool));
                }
                Err(error) => push_diagnostic(&mut report.diagnostics, "zpool", error.to_string()),
            },
            None => push_diagnostic(
                &mut report.diagnostics,
                "zpool",
                "binary not found".to_string(),
            ),
        }

        match find_binary(reader, "zfs") {
            Some(program) => match runner.run(
                &program,
                &["list", "-H", "-o", "name,used,avail,refer,mountpoint"],
            ) {
                Ok(output) => {
                    output
                        .stdout
                        .lines()
                        .filter_map(parse_zfs_dataset)
                        .for_each(|dataset| report.zfs_datasets.push(dataset));
                }
                Err(error) => push_diagnostic(&mut report.diagnostics, "zfs", error.to_string()),
            },
            None => push_diagnostic(
                &mut report.diagnostics,
                "zfs",
                "binary not found".to_string(),
            ),
        }

        report
    }
}

pub struct BtrfsBackend;

impl StorageBackend for BtrfsBackend {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> StorageInspection {
        let mut report = StorageInspection::default();

        match find_binary(reader, "btrfs") {
            Some(program) => match runner.run(&program, &["subvolume", "list", "-o", "/"]) {
                Ok(output) => {
                    output
                        .stdout
                        .lines()
                        .filter_map(parse_btrfs_subvolume)
                        .for_each(|subvolume| report.btrfs_subvolumes.push(subvolume));
                }
                Err(error) => push_diagnostic(&mut report.diagnostics, "btrfs", error.to_string()),
            },
            None => push_diagnostic(
                &mut report.diagnostics,
                "btrfs",
                "binary not found".to_string(),
            ),
        }

        report
    }
}

pub struct LvmBackend;

impl StorageBackend for LvmBackend {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> StorageInspection {
        let mut report = StorageInspection::default();

        match find_binary(reader, "lvs") {
            Some(program) => match runner.run(
                &program,
                &[
                    "--noheadings",
                    "--units",
                    "b",
                    "-o",
                    "vg_name,lv_name,lv_size,attr,kernel_major",
                ],
            ) {
                Ok(output) => {
                    output
                        .stdout
                        .lines()
                        .filter_map(parse_lvm_volume)
                        .for_each(|volume| report.lvm_volumes.push(volume));
                }
                Err(error) => push_diagnostic(&mut report.diagnostics, "lvs", error.to_string()),
            },
            None => push_diagnostic(
                &mut report.diagnostics,
                "lvs",
                "binary not found".to_string(),
            ),
        }

        report
    }
}

pub struct StorageCoordinator {
    pub backends: Vec<Box<dyn StorageBackend>>,
}

impl StorageCoordinator {
    pub fn new() -> Self {
        Self {
            backends: vec![
                Box::new(ZfsBackend),
                Box::new(BtrfsBackend),
                Box::new(LvmBackend),
            ],
        }
    }
}

impl StorageBackend for StorageCoordinator {
    fn inspect(&self, reader: &dyn TextReader, runner: &dyn CommandRunner) -> StorageInspection {
        let mut report = StorageInspection::default();

        for backend in &self.backends {
            let partial = backend.inspect(reader, runner);
            report.zfs_pools.extend(partial.zfs_pools);
            report.zfs_datasets.extend(partial.zfs_datasets);
            report.btrfs_subvolumes.extend(partial.btrfs_subvolumes);
            report.lvm_volumes.extend(partial.lvm_volumes);
            report.diagnostics.extend(partial.diagnostics);
        }

        report
    }
}

pub fn inspect_storage(reader: &dyn TextReader, runner: &dyn CommandRunner) -> StorageInspection {
    StorageCoordinator::new().inspect(reader, runner)
}

pub fn find_binary(reader: &dyn TextReader, name: &str) -> Option<PathBuf> {
    const DIRECTORIES: &[&str] = &[
        "/usr/local/sbin",
        "/usr/local/bin",
        "/usr/sbin",
        "/usr/bin",
        "/sbin",
        "/bin",
    ];
    for directory in DIRECTORIES {
        let path = Path::new(directory).join(name);
        if reader.exists(&path) {
            return Some(path);
        }
    }
    None
}

pub fn parse_zfs_pool(line: &str) -> Option<ZfsPool> {
    let fields = fields(line);
    if fields.len() < 5 {
        return None;
    }
    Some(ZfsPool {
        name: fields[0].to_string(),
        health: fields[1].to_string(),
        size: fields[2].to_string(),
        allocated: fields[3].to_string(),
        free: fields[4].to_string(),
    })
}

pub fn parse_zfs_dataset(line: &str) -> Option<ZfsDataset> {
    let fields = fields(line);
    if fields.len() < 5 {
        return None;
    }
    Some(ZfsDataset {
        name: fields[0].to_string(),
        used: fields[1].to_string(),
        available: fields[2].to_string(),
        referenced: fields[3].to_string(),
        mountpoint: fields[4].to_string(),
    })
}

pub fn parse_btrfs_subvolume(line: &str) -> Option<BtrfsSubvolume> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let path_index = fields.iter().rposition(|field| *field == "path")?;
    let path = fields.get(path_index + 1)?;
    let id = fields
        .windows(2)
        .find(|window| window[0] == "ID")
        .map(|window| window[1].to_string())?;
    Some(BtrfsSubvolume {
        id,
        path: (*path).to_string(),
    })
}

pub fn parse_lvm_volume(line: &str) -> Option<LvmVolume> {
    let fields = fields(line);
    if fields.len() < 4 {
        return None;
    }
    Some(LvmVolume {
        volume_group: fields[0].to_string(),
        name: fields[1].to_string(),
        size: fields[2].to_string(),
        attributes: fields[3].to_string(),
    })
}

fn fields(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

fn push_diagnostic(
    diagnostics: &mut Vec<crate::discovery::model::Diagnostic>,
    source: &str,
    error: String,
) {
    diagnostics.push(crate::discovery::model::Diagnostic {
        source: source.to_string(),
        error,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_zfs_pool() {
        let pool = parse_zfs_pool("tank ONLINE 1T 100G 900G").unwrap();
        assert_eq!(pool.name, "tank");
        assert_eq!(pool.health, "ONLINE");
    }

    #[test]
    fn parses_zfs_dataset() {
        let dataset = parse_zfs_dataset("tank/home 1K 2K 1K /mnt/home").unwrap();
        assert_eq!(dataset.name, "tank/home");
        assert_eq!(dataset.mountpoint, "/mnt/home");
    }

    #[test]
    fn parses_btrfs_subvolume() {
        let subvolume = parse_btrfs_subvolume("ID 256 gen 10 top level 5 path home").unwrap();
        assert_eq!(subvolume.id, "256");
        assert_eq!(subvolume.path, "home");
    }

    #[test]
    fn parses_lvm_volume() {
        let volume = parse_lvm_volume("vg0 lv0 4.00g -wi-a-----").unwrap();
        assert_eq!(volume.volume_group, "vg0");
        assert_eq!(volume.name, "lv0");
    }
}
