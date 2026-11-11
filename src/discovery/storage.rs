use crate::discovery::model::{
    BtrfsDiscovery, Diagnostic, MountDiscovery, StorageDiscovery, ZfsDiscovery,
};
use crate::platform::TextReader;
use std::io;
use std::path::{Path, PathBuf};

pub fn collect_storage(
    reader: &dyn TextReader,
    diagnostics: &mut Vec<Diagnostic>,
) -> StorageDiscovery {
    let mut report = StorageDiscovery {
        supported_filesystems: Vec::new(),
        mounts: Vec::new(),
        zfs: ZfsDiscovery {
            present: reader.exists(Path::new("/dev/zfs")),
            zpool_binary: None,
            zfs_binary: None,
        },
        btrfs: BtrfsDiscovery {
            present: reader.exists(Path::new("/sys/fs/btrfs")),
            filesystem_candidates: Vec::new(),
        },
        lvm_present: reader.exists(Path::new("/dev/mapper/control")),
    };

    match reader.read_to_string(Path::new("/proc/filesystems")) {
        Ok(value) => {
            value
                .lines()
                .filter_map(parse_filesystem)
                .for_each(|filesystem| report.supported_filesystems.push(filesystem));
            report.supported_filesystems.sort();
            report.supported_filesystems.dedup();
        }
        Err(error) => push_diagnostic(diagnostics, "/proc/filesystems", error),
    }

    match reader.read_to_string(Path::new("/proc/mounts")) {
        Ok(value) => {
            value
                .lines()
                .filter_map(parse_mount)
                .for_each(|mount| report.mounts.push(mount));
            report
                .mounts
                .sort_by(|left, right| left.target.cmp(&right.target));
        }
        Err(error) => push_diagnostic(diagnostics, "/proc/mounts", error),
    }

    if report.btrfs.present
        && let Ok(names) = reader.read_dir_names(Path::new("/sys/fs/btrfs"))
    {
        report.btrfs.filesystem_candidates = names
            .into_iter()
            .map(|name| Path::new("/sys/fs/btrfs").join(name))
            .collect();
    }

    report
}

fn parse_filesystem(line: &str) -> Option<String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    match fields.as_slice() {
        ["nodev", filesystem] => Some((*filesystem).to_string()),
        [filesystem] if !filesystem.is_empty() => Some((*filesystem).to_string()),
        _ => None,
    }
}

fn parse_mount(line: &str) -> Option<MountDiscovery> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 4 {
        return None;
    }
    let target = PathBuf::from(fields[1]);
    let filesystem = fields[2].to_string();
    Some(MountDiscovery {
        device: fields[0].to_string(),
        mount_point: target.clone(),
        target,
        fs_type: filesystem.clone(),
        filesystem,
        options: fields[3].to_string(),
    })
}

fn push_diagnostic(diagnostics: &mut Vec<Diagnostic>, source: &str, error: io::Error) {
    diagnostics.push(Diagnostic {
        source: source.to_string(),
        error: error.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mount_populates_alias_fields() {
        let mount = parse_mount("/dev/sda1 / ext4 rw,relatime 0 0").unwrap();

        assert_eq!(mount.target, PathBuf::from("/"));
        assert_eq!(mount.mount_point, PathBuf::from("/"));
        assert_eq!(mount.filesystem, "ext4");
        assert_eq!(mount.fs_type, "ext4");
        assert_eq!(mount.device, "/dev/sda1");
        assert_eq!(mount.options, "rw,relatime");
    }

    #[test]
    fn parse_mount_rejects_short_lines() {
        assert!(parse_mount("/dev/sda1 / ext4").is_none());
    }
}
