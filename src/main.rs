mod access;
mod app;
mod cli;
mod config;
mod discovery;
mod error;
mod logging;
mod platform;
mod plugins;
mod provision;
mod runtime;
mod storage;
mod telemetry;
mod tui;

use crate::plugins::PluginBackend;
use anyhow::Context;
use app::AppRuntime;
use clap::Parser;
use cli::{Cli, Commands};
use config::load as load_config;
use error::AnvilError;
use platform::OsTextReader;
use provision::{load_manifest_jobs, plan_manifest_jobs};
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

#[allow(clippy::collapsible_if)]
async fn run(cli: Cli, access: access::AccessContext) -> anyhow::Result<()> {
    let config = load_config(cli.config.clone())?;
    let reader = Arc::new(OsTextReader);
    let command_runner = Arc::new(platform::OsCommandRunner);
    let runtime = AppRuntime::bootstrap(
        config.clone(),
        access,
        Vec::new(),
        reader.clone(),
        command_runner.clone(),
    );

    runtime.state.write().await.discovery = Some(discovery::all(reader.as_ref(), &*command_runner));
    runtime.state.write().await.storage =
        Some(storage::inspect_storage(reader.as_ref(), &*command_runner));
    runtime.state.write().await.runtime =
        Some(runtime::inspect_runtime(reader.as_ref(), &*command_runner));
    runtime.state.write().await.mutation_controller = Some(runtime::MutationController::new(
        access,
        if std::env::var("ANVIL_ALLOW_MUTATION").ok().as_deref() == Some("1") {
            runtime::mutation::MutationMode::Execute
        } else {
            runtime::mutation::MutationMode::DryRun
        },
        command_runner.clone(),
    ));
    runtime.state.write().await.plugins = Some(crate::plugins::WasmPluginEngine::new().scan(
        reader.as_ref(),
        &crate::plugins::default_plugin_directories(access::effective_uid()),
    ));

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
        Some(Commands::Runtime { json }) => {
            let report = runtime
                .state
                .read()
                .await
                .runtime
                .clone()
                .context("runtime report is unavailable")?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("runtime report could not be encoded")?
                );
            } else {
                print_runtime_report(&report);
            }
            Ok(())
        }
        Some(Commands::Plugins { json }) => {
            let report = runtime
                .state
                .read()
                .await
                .plugins
                .clone()
                .context("plugin catalog is unavailable")?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .context("plugin catalog could not be encoded")?
                );
            } else {
                print_plugin_report(&report);
            }
            Ok(())
        }
        Some(Commands::Apply {
            manifest,
            json,
            dry_run,
            verbose,
        }) => {
            if !dry_run {
                return Err(AnvilError::Permission(
                    "apply requires --dry-run; it never mutates system state".to_string(),
                )
                .into());
            }

            let jobs = load_manifest_jobs(manifest)?;
            let plan = plan_manifest_jobs(&jobs)?;

            for job in &jobs {
                println!(
                    "[DRY-RUN] Reading manifest: {} (API v{})",
                    job.path.display(),
                    job.manifest.version
                );
            }

            for action in &plan.actions {
                println!(
                    "[DRY-RUN] [{}] {}",
                    action.domain.to_ascii_uppercase(),
                    action.summary
                );
                if verbose > 0 {
                    if let Some(command) = &action.command {
                        println!(
                            "[DRY-RUN] Command: {} {}",
                            command.program,
                            command.args.join(" ")
                        );
                    }
                }
            }
            if json {
                let actions: Vec<serde_json::Value> = plan
                    .actions
                    .iter()
                    .map(|action| {
                        serde_json::json!({
                            "domain": action.domain,
                            "resource": action.resource,
                            "summary": action.summary,
                            "command": action.command.as_ref().map(|command| serde_json::json!({
                                "program": command.program,
                                "args": command.args,
                            }))
                            .unwrap_or(serde_json::Value::Null),
                        })
                    })
                    .collect();
                let payload = serde_json::to_string_pretty(&serde_json::json!({
                    "manifests": jobs.len(),
                    "actions": actions,
                    "error_count": plan.error_count,
                    "changes_applied": 0
                }))?;
                println!("{}", payload);
                return Ok(());
            }
            if plan.error_count > 0 {
                println!(
                    "Summary: {} action(s) planned, {} manifest error(s), 0 changes applied (Dry-run mode).",
                    plan.actions.len(),
                    plan.error_count
                );
                return Err(AnvilError::Config(
                    "provisioning plan has manifest errors".to_string(),
                )
                .into());
            }

            println!(
                "Summary: {} action(s) planned, 0 errors, 0 changes applied (Dry-run mode).",
                plan.actions.len()
            );
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

fn print_runtime_report(report: &crate::runtime::RuntimeInspection) {
    println!(
        "container_runtimes={} init={} init_detected={}",
        report.container_runtimes.len(),
        report.init.kind,
        report.init.detected
    );
    for runtime in &report.container_runtimes {
        println!(
            "runtime={} daemon_reachable={} socket={} daemon_status={} containers={}",
            runtime.name,
            runtime.daemon_reachable,
            runtime.socket,
            runtime.daemon_status.as_deref().unwrap_or("<unknown>"),
            runtime.containers.len()
        );
        for container in &runtime.containers {
            println!(
                "  container {} {} {} {}",
                container.id, container.name, container.image, container.status
            );
        }
    }
}

fn print_plugin_report(report: &crate::plugins::PluginCatalog) {
    println!(
        "api_version={} plugins={} diagnostics={}",
        report.api_version,
        report.plugins.len(),
        report.diagnostics.len()
    );
    for plugin in &report.plugins {
        println!(
            "plugin {} v{} api={} exports={} invoked={} result={:?}",
            plugin.name,
            plugin.version,
            plugin.api_version,
            plugin.exports.join(","),
            plugin.invoked,
            plugin.entry_result
        );
    }
    for diagnostic in &report.diagnostics {
        println!("diagnostic {}: {}", diagnostic.source, diagnostic.error);
    }
}
