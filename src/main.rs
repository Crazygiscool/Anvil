mod access;
mod app;
mod cli;
mod config;
mod discovery;
mod error;
mod logging;
mod platform;
mod storage;
mod telemetry;

use anyhow::Context;
use app::AppRuntime;
use clap::Parser;
use cli::{Cli, Commands};
use config::{SUPPORTED_VERSION, load as load_config};
use error::AnvilError;
use platform::{OsCommandRunner, OsTextReader};
use std::process::ExitCode;
use std::sync::Arc;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let access = access::AccessContext::detect(cli.read_only);

    let log_file = cli
        .log_file
        .clone()
        .unwrap_or_else(access::default_log_file);
    if let Err(error) = logging::init(Some(&log_file)) {
        eprintln!("logging error: {error}");
    }

    let result = run(cli, access).await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli, access: access::AccessContext) -> anyhow::Result<()> {
    let config = load_config(cli.config.clone())?;
    let reader = Arc::new(OsTextReader);
    let runner = OsCommandRunner;
    let runtime = AppRuntime::bootstrap(config.clone(), access, Vec::new(), reader.clone());

    runtime.state.write().await.discovery = Some(discovery::all(reader.as_ref(), &runner));
    runtime.state.write().await.storage = Some(storage::inspect_storage(reader.as_ref(), &runner));

    match cli.command {
        None | Some(Commands::Tui { .. }) => {
            let refresh_rate = if let Some(Commands::Tui { refresh_rate }) = cli.command {
                refresh_rate
            } else {
                config.refresh_rate_ms
            };
            runtime
                .run_tui(refresh_rate)
                .await
                .context("TUI event loop failed")
        }
        Some(Commands::Discover { json }) => {
            let report = runtime
                .state
                .read()
                .await
                .discovery
                .clone()
                .context("discovery report is unavailable")?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("discovery report could not be encoded")?
                );
            } else {
                print_discovery_report(&report);
            }
            Ok(())
        }
        Some(Commands::Storage { json }) => {
            let report = runtime
                .state
                .read()
                .await
                .storage
                .clone()
                .context("storage report is unavailable")?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("storage report could not be encoded")?
                );
            } else {
                print_storage_report(&report);
            }
            Ok(())
        }
        Some(Commands::Apply {
            manifest,
            dry_run,
            verbose,
        }) => {
            if !dry_run {
                return Err(AnvilError::Permission(
                    "apply is staged only in Phase 1; rerun with --dry-run".to_string(),
                )
                .into());
            }
            let paths = collect_manifest_paths(manifest)?;
            for path in &paths {
                validate_manifest_version(path, SUPPORTED_VERSION)?;
                if verbose > 0 {
                    tracing::info!("validated manifest {}", path.display());
                }
            }
            println!(
                "[DRY-RUN] Validated {} manifest(s); 0 changes applied.",
                paths.len()
            );
            for path in paths {
                println!("[DRY-RUN] {} -> schema OK", path.display());
            }
            Ok(())
        }
    }
}

fn print_discovery_report(report: &crate::discovery::DiscoveryReport) {
    println!("schema_version={}", report.schema_version);
    println!("storage:");
    println!("  zfs_present={}", report.storage.zfs.present);
    println!("  btrfs_present={}", report.storage.btrfs.present);
    println!("  mounts={}", report.storage.mounts.len());
    println!("network:");
    println!("  interfaces={}", report.network.interfaces.len());
    for interface in &report.network.interfaces {
        println!(
            "  - {}: {:?} (physical={}, wireless={})",
            interface.name, interface.kind, interface.device_present, interface.wireless
        );
    }
    println!("runtime:");
    println!("  init={}", report.runtime.init.kind);
    println!("  containers={}", report.runtime.container_runtimes.len());
    println!("binaries:");
    for binary in &report.binaries {
        println!(
            "  - {}: available={}, path={}, version={}",
            binary.name,
            binary.available,
            binary
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<not found>".to_string()),
            binary.version.as_deref().unwrap_or("<unknown>")
        );
    }
}

fn print_storage_report(report: &crate::storage::StorageInspection) {
    println!(
        "zfs_pools={} datasets={} btrfs_subvolumes={} lvm_volumes={} diagnostics={}",
        report.zfs_pools.len(),
        report.zfs_datasets.len(),
        report.btrfs_subvolumes.len(),
        report.lvm_volumes.len(),
        report.diagnostics.len()
    );
    for pool in &report.zfs_pools {
        println!(
            "zpool {} health={} size={}",
            pool.name, pool.health, pool.size
        );
    }
    for dataset in &report.zfs_datasets {
        println!(
            "zfs {} mount={} used={} avail={}",
            dataset.name, dataset.mountpoint, dataset.used, dataset.available
        );
    }
    for subvolume in &report.btrfs_subvolumes {
        println!(
            "btrfs subvolume id={} path={}",
            subvolume.id, subvolume.path
        );
    }
    for volume in &report.lvm_volumes {
        println!(
            "lv {}={} vg? size={} attr={}",
            volume.name, volume.volume_group, volume.size, volume.attributes
        );
    }
}

fn collect_manifest_paths(input: std::path::PathBuf) -> anyhow::Result<Vec<std::path::PathBuf>> {
    if input.is_dir() {
        let mut paths: Vec<_> = std::fs::read_dir(&input)
            .map_err(|error| AnvilError::Io {
                path: input.clone(),
                source: error,
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("toml")
            })
            .collect();
        paths.sort();
        if paths.is_empty() {
            return Err(AnvilError::Config(format!(
                "no TOML manifests found in {}",
                input.display()
            ))
            .into());
        }
        Ok(paths)
    } else {
        Ok(vec![input])
    }
}

fn validate_manifest_version(path: &std::path::Path, expected: u32) -> anyhow::Result<()> {
    let source = std::fs::read_to_string(path).map_err(|error| AnvilError::Io {
        path: path.to_path_buf(),
        source: error,
    })?;
    let value: toml::Value = toml::from_str(&source)
        .map_err(|error| AnvilError::Config(format!("{}: {error}", path.display())))?;
    let found = value
        .get("version")
        .and_then(toml::Value::as_integer)
        .map(|value| value as u32)
        .ok_or_else(|| {
            AnvilError::Config(format!("{}: missing integer version", path.display()))
        })?;
    if found != expected {
        return Err(AnvilError::ManifestVersion {
            path: path.to_path_buf(),
            found,
            expected,
        }
        .into());
    }
    Ok(())
}
