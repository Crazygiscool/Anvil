mod access;
mod app;
mod catalog;
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
use cli::{CatalogCommands, Cli, Commands};
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
        reader.clone(),
        command_runner.clone(),
    );

    runtime.state.write().await.discovery = Some(discovery::all(reader.as_ref(), &*command_runner));
    runtime.state.write().await.storage =
        Some(storage::inspect_storage(reader.as_ref(), &*command_runner));
    runtime.state.write().await.runtime = Some(runtime::inspect_runtime_with_clients(
        reader.as_ref(),
        &*command_runner,
        &runtime.clients,
    ));
    runtime.state.write().await.mutation_controller =
        Some(runtime::MutationController::with_clients(
            access,
            if std::env::var("ANVIL_ALLOW_MUTATION").ok().as_deref() == Some("1") {
                runtime::mutation::MutationMode::Execute
            } else {
                runtime::mutation::MutationMode::DryRun
            },
            command_runner.clone(),
            runtime.clients.clone(),
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
        Some(Commands::Catalog { command }) => {
            let runner: Arc<dyn platform::CommandRunner + Send + Sync> = command_runner.clone();
            run_catalog(command, access, &runner, &runtime.clients).await
        }
    }
}

/// Run a catalog subcommand.
async fn run_catalog(
    command: CatalogCommands,
    access: access::AccessContext,
    runner: &Arc<dyn platform::CommandRunner + Send + Sync>,
    clients: &runtime::ContainerClients,
) -> anyhow::Result<()> {
    match command {
        CatalogCommands::List { json, category } => {
            let index = catalog::load_index()?;
            let entries: Vec<&catalog::CatalogEntry> = match &category {
                Some(category) => index
                    .apps
                    .iter()
                    .filter(|entry| &entry.category == category)
                    .collect(),
                None => index.apps.iter().collect(),
            };

            if json {
                let payload = serde_json::to_string_pretty(&serde_json::json!({
                    "version": index.version,
                    "categories": index.categories(),
                    "apps": entries,
                }))?;
                println!("{}", payload);
                return Ok(());
            }

            println!(
                "catalog entries={} categories={}",
                entries.len(),
                index.categories().len()
            );
            for entry in entries {
                println!(
                    "  - {} | {} | {} | {}",
                    entry.id, entry.category, entry.image, entry.name
                );
            }
            Ok(())
        }
        CatalogCommands::Show { id, json } => {
            let index = catalog::load_index()?;
            let entry = index.find(&id).ok_or_else(|| {
                AnvilError::Config(format!("catalog entry '{id}' does not exist"))
            })?;

            if json {
                println!("{}", serde_json::to_string_pretty(entry)?);
                return Ok(());
            }

            print_catalog_entry(entry);
            Ok(())
        }
        CatalogCommands::Install { id, json, dry_run } => {
            let index = catalog::load_index()?;
            let entry = index.find(&id).ok_or_else(|| {
                AnvilError::Config(format!("catalog entry '{id}' does not exist"))
            })?;

            let backend = catalog::backend_for(&entry.runtime).ok_or_else(|| {
                AnvilError::Config(format!(
                    "catalog entry '{id}': unsupported runtime '{}'",
                    entry.runtime
                ))
            })?;
            let target = catalog::install_target(entry)?;

            if dry_run {
                // A dry run only prints what would run. It never touches the
                // host, so it does not need the mutation gate.
                let plan = catalog::install_plan(entry)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "id": entry.id,
                            "dry_run": true,
                            "executed": false,
                            "plan": {
                                "program": plan.program,
                                "args": plan.args,
                                "command": plan.command_line(),
                                "action": plan.describe(),
                                "summary": plan.summary,
                            },
                        }))?
                    );
                } else {
                    println!("catalog install id={}", entry.id);
                    println!("  action: {}", plan.describe());
                    println!("  command: {}", plan.command_line());
                    println!("  result: planned (dry-run), nothing was changed");
                }
                return Ok(());
            }

            let controller = runtime::MutationController::with_clients(
                access,
                mutation_mode(false),
                Arc::clone(runner),
                clients.clone(),
            );
            let receipt =
                controller.execute(backend, &target, &runtime::mutation::MutationVerb::Install)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&receipt_json(&receipt))?);
                return Ok(());
            }

            println!("catalog install id={}", entry.id);
            println!("  action: {}", receipt.plan.describe());
            println!("  command: {}", receipt.plan.command_line());
            if receipt.execution.executed {
                println!("  result: executed");
            } else {
                println!("  result: planned (dry-run), nothing was changed");
            }
            if let Some(verification) = &receipt.verification {
                println!(
                    "  verification: {} ({})",
                    if verification.success { "ok" } else { "failed" },
                    verification.command
                );
            }
            if let Some(rollback) = &receipt.rollback {
                println!(
                    "  rollback: {} {}",
                    rollback.program,
                    rollback.args.join(" ")
                );
            }
            Ok(())
        }
        CatalogCommands::Update { from_file } => {
            // The file is validated before it replaces anything, so a broken
            // index can never take the catalog down.
            let index = catalog::read_index(&from_file)?;
            let target = catalog::default_index_path();

            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| AnvilError::Io {
                    path: parent.to_path_buf(),
                    source: error,
                })?;
            }
            let rendered = toml::to_string_pretty(&index).map_err(|error| {
                AnvilError::Config(format!("catalog index could not be encoded: {error}"))
            })?;
            std::fs::write(&target, rendered).map_err(|error| AnvilError::Io {
                path: target.clone(),
                source: error,
            })?;

            println!(
                "catalog updated entries={} source={} target={}",
                index.apps.len(),
                from_file.display(),
                target.display()
            );
            Ok(())
        }
    }
}

/// The mutation mode for a catalog install.
///
/// An install only touches the host when it runs as root with
/// `ANVIL_ALLOW_MUTATION=1`; `--dry-run` always plans without executing.
fn mutation_mode(dry_run: bool) -> runtime::mutation::MutationMode {
    if dry_run {
        return runtime::mutation::MutationMode::DryRun;
    }
    if std::env::var("ANVIL_ALLOW_MUTATION").ok().as_deref() == Some("1") {
        runtime::mutation::MutationMode::Execute
    } else {
        runtime::mutation::MutationMode::DryRun
    }
}

fn receipt_json(receipt: &runtime::controller::ControllerReceipt) -> serde_json::Value {
    serde_json::to_value(receipt).unwrap_or_else(|_| serde_json::json!({}))
}
fn print_catalog_entry(entry: &catalog::CatalogEntry) {
    println!("id={}", entry.id);
    println!("name={}", entry.name);
    if !entry.description.is_empty() {
        println!("description={}", entry.description);
    }
    println!("category={}", entry.category);
    println!("image={}", entry.image);
    println!("runtime={}", entry.runtime);
    println!("restart_policy={}", entry.restart_policy);
    for port in &entry.ports {
        println!("port={}:{}/{}", port.host, port.container, port.protocol);
    }
    for volume in &entry.volumes {
        println!(
            "volume={} kind={} target={}",
            volume.source, volume.kind, volume.target
        );
    }
    for (key, value) in &entry.env {
        println!("env={key}={value}");
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

fn print_runtime_report(report: &crate::runtime::RuntimeInspection) {
    println!(
        "container_runtimes={} init={} init_detected={} services={}",
        report.container_runtimes.len(),
        report.init.kind,
        report.init.detected,
        report.init.services.len()
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
    for service in &report.init.services {
        println!("  service {} {}", service.name, service.state);
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
