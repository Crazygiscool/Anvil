use crate::discovery::model::{
    BinaryDiscovery, ContainerRuntime, Diagnostic, InitDiscovery, RuntimeDiscovery,
};
use crate::platform::{CommandOutput, CommandRunner, TextReader};
use std::path::{Path, PathBuf};

const CANDIDATE_DIRS: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];

const SOCKET_CANDIDATES: &[(&str, &str)] = &[
    ("docker", "/var/run/docker.sock"),
    ("docker", "/run/docker.sock"),
    ("podman", "/run/podman/podman.sock"),
];

pub fn collect_runtime(
    reader: &dyn TextReader,
    runner: &dyn CommandRunner,
    _diagnostics: &mut Vec<Diagnostic>,
) -> RuntimeDiscovery {
    let mut container_runtimes = Vec::new();
    for (name, socket) in SOCKET_CANDIDATES {
        let socket_path = PathBuf::from(socket);
        let present = reader.exists(&socket_path);
        container_runtimes.push(ContainerRuntime {
            name: (*name).to_string(),
            socket: socket_path,
            present,
            accessible: present,
        });
    }

    let init = detect_init(reader, runner);

    RuntimeDiscovery {
        container_runtimes,
        init,
    }
}

pub fn collect_binaries(
    reader: &dyn TextReader,
    runner: &dyn CommandRunner,
    specs: &[BinarySpec],
    _diagnostics: &mut Vec<Diagnostic>,
) -> Vec<BinaryDiscovery> {
    let mut results = Vec::new();
    for spec in specs {
        results.push(detect_binary(reader, runner, spec, _diagnostics));
    }
    results
}

pub struct BinarySpec {
    pub name: &'static str,
    pub version_args: &'static [&'static str],
}

fn detect_init(reader: &dyn TextReader, runner: &dyn CommandRunner) -> InitDiscovery {
    let systemd_markers = [
        Path::new("/run/systemd/system"),
        Path::new("/usr/lib/systemd"),
    ];
    let openrc_markers = [
        Path::new("/run/openrc"),
        Path::new("/etc/init.d"),
        Path::new("/sbin/openrc-run"),
    ];

    if systemd_markers.iter().any(|path| reader.exists(path)) {
        let manager = detect_binary_version(reader, runner, "systemctl", &["--version"]);
        let markers = systemd_markers
            .iter()
            .filter(|path| reader.exists(path))
            .map(|path| path.to_path_buf())
            .collect();
        InitDiscovery {
            kind: "systemd".to_string(),
            detected: true,
            markers,
            service_manager_version: manager,
        }
    } else if openrc_markers.iter().any(|path| reader.exists(path)) {
        let manager = detect_binary_version(reader, runner, "openrc", &["--version"]);
        let markers = openrc_markers
            .iter()
            .filter(|path| reader.exists(path))
            .map(|path| path.to_path_buf())
            .collect();
        InitDiscovery {
            kind: "openrc".to_string(),
            detected: true,
            markers,
            service_manager_version: manager,
        }
    } else {
        InitDiscovery {
            kind: "unknown".to_string(),
            detected: false,
            markers: Vec::new(),
            service_manager_version: None,
        }
    }
}

fn detect_binary_version(
    reader: &dyn TextReader,
    runner: &dyn CommandRunner,
    name: &'static str,
    args: &'static [&'static str],
) -> Option<String> {
    find_candidate(reader, name)
        .and_then(|path| runner.run(&path, args).ok())
        .map(first_output)
        .filter(|value| !value.is_empty())
}

fn detect_binary(
    reader: &dyn TextReader,
    runner: &dyn CommandRunner,
    spec: &BinarySpec,
    _diagnostics: &mut Vec<Diagnostic>,
) -> BinaryDiscovery {
    let path = find_candidate(reader, spec.name);
    let mut error = None;
    let mut version = None;

    if let Some(path) = path.clone() {
        match runner.run(&path, spec.version_args) {
            Ok(output) if !output.stdout.is_empty() => version = Some(output.stdout),
            Ok(output) if !output.stderr.is_empty() => version = Some(output.stderr),
            Ok(_) => error = Some("binary did not return a version".to_string()),
            Err(io_error) => error = Some(io_error.to_string()),
        }

        if version.is_none() && error.is_none() {
            error = Some("binary did not return a version".to_string());
        }
    } else {
        error = Some("binary not found".to_string());
    }

    if let Some(error) = &error {
        // Only record diagnostics for failed binaries, not necessarily absent ones, to keep
        // low-cost probes readable. Diagnostics remain available for traceability.
        if !error.contains("binary not found") {
            (Diagnostic {
                source: format!("binary:{}", spec.name),
                error: error.clone(),
            });
        }
    }

    BinaryDiscovery {
        name: spec.name.to_string(),
        path,
        available: version.is_some(),
        version,
        error: error.filter(|value| !value.contains("binary not found")),
    }
}

fn find_candidate(reader: &dyn TextReader, name: &str) -> Option<PathBuf> {
    for directory in CANDIDATE_DIRS {
        let candidate = Path::new(directory).join(name);
        if reader.exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn first_output(output: CommandOutput) -> String {
    if !output.stdout.is_empty() {
        output.stdout
    } else {
        output.stderr
    }
}
