use crate::discovery::model::DiscoveryReport;
use crate::discovery::network::collect_network;
use crate::discovery::runtime::{BinarySpec, collect_binaries, collect_runtime};
use crate::discovery::storage::collect_storage;
use crate::platform::{CommandRunner, TextReader};

pub static BINARY_SPECS: &[BinarySpec] = &[
    BinarySpec {
        name: "systemctl",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "openrc",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "openrc-run",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "docker",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "podman",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "zfs",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "zpool",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "btrfs",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "lvs",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "vgs",
        version_args: &["--version"],
    },
    BinarySpec {
        name: "pvs",
        version_args: &["--version"],
    },
];

pub fn all(text: &dyn TextReader, runner: &dyn CommandRunner) -> DiscoveryReport {
    let mut report = DiscoveryReport::new();
    let mut diagnostics = Vec::new();

    report.storage = collect_storage(text, &mut diagnostics);
    report.network = collect_network(text);
    report.binaries = collect_binaries(text, runner, BINARY_SPECS, &mut diagnostics);
    report.runtime = collect_runtime(text, runner, &mut diagnostics);
    report.diagnostics = diagnostics;

    report
}
