#![allow(clippy::items_after_test_module)]
#![allow(dead_code)]
#![allow(clippy::clone_on_copy)]

use crate::error::AnvilError;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Docker,
    Podman,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitKind {
    Systemd,
    Openrc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationVerb {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
    Install,
    Remove,
}

/// A port mapping for a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
    pub protocol: String,
}

/// A volume mount for a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VolumeMount {
    pub source: String,
    pub target: String,
    /// `volume` for a named volume, `bind` for a host path.
    pub kind: String,
}

/// Everything needed to create a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMount>,
    pub env: Vec<(String, String)>,
    pub restart_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MutationTarget {
    Container {
        runtime: RuntimeKind,
        name: String,
    },
    Service {
        init: InitKind,
        name: String,
    },
    /// A container that does not exist yet and has to be created.
    ContainerInstall {
        runtime: RuntimeKind,
        spec: ContainerSpec,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MutationPlan {
    pub target: MutationTarget,
    pub verb: MutationVerb,
    pub program: &'static str,
    pub args: Vec<String>,
    pub requires_root: bool,
    pub dry_run_only: bool,
    pub summary: String,
}

impl MutationPlan {
    pub fn command_line(&self) -> String {
        let mut line = self.program.to_string();
        for arg in &self.args {
            line.push(' ');
            line.push_str(arg);
        }
        line
    }
}

pub trait MutationBackend: Send + Sync {
    fn plan(
        &self,
        target: &MutationTarget,
        verb: &MutationVerb,
    ) -> Result<MutationPlan, AnvilError>;
}

pub struct SystemdBackend;

impl MutationBackend for SystemdBackend {
    fn plan(
        &self,
        target: &MutationTarget,
        verb: &MutationVerb,
    ) -> Result<MutationPlan, AnvilError> {
        let MutationTarget::Service {
            init: InitKind::Systemd,
            name,
        } = target
        else {
            return Err(AnvilError::Config(
                "systemd backend received a non-systemd target".to_string(),
            ));
        };
        let command = match verb {
            MutationVerb::Start => ("start", vec![name.clone()]),
            MutationVerb::Stop => ("stop", vec![name.clone()]),
            MutationVerb::Restart => ("restart", vec![name.clone()]),
            MutationVerb::Enable => ("enable", vec![name.clone()]),
            MutationVerb::Disable => ("disable", vec![name.clone()]),
            MutationVerb::Install | MutationVerb::Remove => {
                return Err(AnvilError::Config(
                    "systemd services do not support install or remove".to_string(),
                ));
            }
        };
        Ok(MutationPlan {
            target: target.clone(),
            verb: *verb,
            program: "systemctl",
            args: std::iter::once(command.0.to_string())
                .chain(command.1)
                .collect(),
            requires_root: true,
            dry_run_only: true,
            summary: format!("plan {} for systemd service {}", verb_name(*verb), name),
        })
    }
}

pub struct OpenrcBackend;

impl MutationBackend for OpenrcBackend {
    fn plan(
        &self,
        target: &MutationTarget,
        verb: &MutationVerb,
    ) -> Result<MutationPlan, AnvilError> {
        let MutationTarget::Service {
            init: InitKind::Openrc,
            name,
        } = target
        else {
            return Err(AnvilError::Config(
                "OpenRC backend received a non-OpenRC target".to_string(),
            ));
        };
        let service_verb = match verb {
            MutationVerb::Start => "start",
            MutationVerb::Stop => "stop",
            MutationVerb::Restart => "restart",
            MutationVerb::Enable => "enable",
            MutationVerb::Disable => "disable",
            MutationVerb::Install | MutationVerb::Remove => {
                return Err(AnvilError::Config(
                    "OpenRC services do not support install or remove".to_string(),
                ));
            }
        };
        Ok(MutationPlan {
            target: target.clone(),
            verb: *verb,
            program: "rc-service",
            args: vec![name.clone(), service_verb.to_string()],
            requires_root: true,
            dry_run_only: true,
            summary: format!("plan {} for OpenRC service {}", verb_name(*verb), name),
        })
    }
}

pub struct DockerBackend;

impl MutationBackend for DockerBackend {
    fn plan(
        &self,
        target: &MutationTarget,
        verb: &MutationVerb,
    ) -> Result<MutationPlan, AnvilError> {
        plan_container_mutation(target, verb, "docker", RuntimeKind::Docker)
    }
}

pub struct PodmanBackend;

impl MutationBackend for PodmanBackend {
    fn plan(
        &self,
        target: &MutationTarget,
        verb: &MutationVerb,
    ) -> Result<MutationPlan, AnvilError> {
        plan_container_mutation(target, verb, "podman", RuntimeKind::Podman)
    }
}

/// Build the command for a container lifecycle mutation.
///
/// Container installs and removals go through the same plan/execute path as
/// every other mutation, so the privilege gate, the receipt, the
/// verification step and the rollback plan all apply unchanged.
fn plan_container_mutation(
    target: &MutationTarget,
    verb: &MutationVerb,
    program: &'static str,
    expected: RuntimeKind,
) -> Result<MutationPlan, AnvilError> {
    match target {
        MutationTarget::Container { runtime, name } => {
            if *runtime != expected {
                return Err(AnvilError::Config(format!(
                    "{program} backend received a target for a different runtime"
                )));
            }
            let command_verb = match verb {
                MutationVerb::Start => "start",
                MutationVerb::Stop => "stop",
                MutationVerb::Restart => "restart",
                MutationVerb::Remove => "rm",
                MutationVerb::Enable | MutationVerb::Disable => {
                    return Err(AnvilError::Config(format!(
                        "{program} container lifecycle does not support service enablement"
                    )));
                }
                MutationVerb::Install => {
                    return Err(AnvilError::Config(format!(
                        "{program} install requires a container specification"
                    )));
                }
            };
            let mut args = vec![command_verb.to_string()];
            if command_verb == "rm" {
                args.push("-f".to_string());
            }
            args.push(name.clone());
            Ok(MutationPlan {
                target: target.clone(),
                verb: *verb,
                program,
                args,
                requires_root: true,
                dry_run_only: true,
                summary: format!("plan container {} for {}", command_verb, name),
            })
        }
        MutationTarget::ContainerInstall { runtime, spec } => {
            if *runtime != expected {
                return Err(AnvilError::Config(format!(
                    "{program} backend received a target for a different runtime"
                )));
            }
            if !matches!(verb, MutationVerb::Install) {
                return Err(AnvilError::Config(format!(
                    "{program} container specification only supports install"
                )));
            }
            Ok(MutationPlan {
                target: target.clone(),
                verb: *verb,
                program,
                args: container_run_args(spec),
                requires_root: true,
                dry_run_only: true,
                summary: format!(
                    "plan install of container {} from {}",
                    spec.name, spec.image
                ),
            })
        }
        MutationTarget::Service { .. } => Err(AnvilError::Config(format!(
            "{program} backend received a non-container target"
        ))),
    }
}

/// Arguments for `docker run` / `podman run` that create and start a
/// container in one step.
fn container_run_args(spec: &ContainerSpec) -> Vec<String> {
    let mut args = vec!["run".to_string(), "-d".to_string()];
    args.push("--name".to_string());
    args.push(spec.name.clone());

    let policy = spec.restart_policy.trim();
    if !policy.is_empty() && policy != "no" {
        args.push("--restart".to_string());
        args.push(policy.to_string());
    }

    for port in &spec.ports {
        args.push("-p".to_string());
        args.push(format!(
            "{}:{}/{}",
            port.host, port.container, port.protocol
        ));
    }

    for volume in &spec.volumes {
        args.push("-v".to_string());
        args.push(format!("{}:{}", volume.source, volume.target));
    }

    for (key, value) in &spec.env {
        args.push("-e".to_string());
        args.push(format!("{key}={value}"));
    }

    args.push(spec.image.clone());
    args
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MutationExecutionReceipt {
    pub executed: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}
#[derive(Debug, Clone)]
pub struct MutationGateway {
    access: crate::access::AccessContext,
    mode: MutationMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationMode {
    DryRun,
    Execute,
}

impl MutationGateway {
    pub fn new(access: crate::access::AccessContext, mode: MutationMode) -> Self {
        Self { access, mode }
    }

    #[allow(dead_code)]
    pub fn can_mutate(&self) -> bool {
        self.access.allows_mutation()
    }

    /// The mode the gateway runs in.
    ///
    /// The UI needs this to tell the user whether a confirmed action will
    /// really touch the host or only be planned.
    pub fn mode(&self) -> MutationMode {
        self.mode
    }

    pub fn plan(
        &self,
        backend: &dyn MutationBackend,
        target: &MutationTarget,
        verb: &MutationVerb,
    ) -> Result<MutationPlan, AnvilError> {
        if !self.can_mutate() {
            return Err(AnvilError::Permission(
                "read-only access cannot plan lifecycle mutations".to_string(),
            ));
        }
        let plan = backend.plan(target, verb)?;
        if self.mode == MutationMode::DryRun {
            Ok(MutationPlan {
                dry_run_only: true,
                ..plan
            })
        } else {
            Ok(plan)
        }
    }
}

pub struct PlannedAction {
    pub summary: String,
    pub command: String,
    pub executed: bool,
}

fn verb_name(verb: MutationVerb) -> &'static str {
    match verb {
        MutationVerb::Start => "start service",
        MutationVerb::Stop => "stop service",
        MutationVerb::Restart => "restart service",
        MutationVerb::Enable => "enable service",
        MutationVerb::Disable => "disable service",
        MutationVerb::Install => "install container",
        MutationVerb::Remove => "remove container",
    }
}

#[allow(dead_code)]
pub fn mutation_kind(target: &MutationTarget) -> &'static str {
    match target {
        MutationTarget::Container { .. } | MutationTarget::ContainerInstall { .. } => "container",
        MutationTarget::Service { .. } => "service",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemd_backend_builds_command_array() {
        let backend = SystemdBackend;
        let plan = backend
            .plan(
                &MutationTarget::Service {
                    init: InitKind::Systemd,
                    name: "demo.service".to_string(),
                },
                &MutationVerb::Restart,
            )
            .unwrap();

        assert_eq!(plan.program, "systemctl");
        assert_eq!(
            plan.args,
            vec!["restart".to_string(), "demo.service".to_string()]
        );
        assert!(plan.requires_root);
        assert!(plan.dry_run_only);
    }

    #[test]
    fn container_backends_reject_unsupported_verbs() {
        let backend = DockerBackend;
        let result = backend.plan(
            &MutationTarget::Container {
                runtime: RuntimeKind::Docker,
                name: "app".to_string(),
            },
            &MutationVerb::Enable,
        );
        assert!(result.is_err());
    }

    #[test]
    fn gateway_rejects_read_only_access() {
        let access = crate::access::AccessContext {
            tier: crate::access::AccessTier::ReadOnly,
        };
        let backend = OpenrcBackend;
        let target = MutationTarget::Service {
            init: InitKind::Openrc,
            name: "demo".to_string(),
        };
        let gateway = MutationGateway::new(access, MutationMode::DryRun);
        assert!(
            gateway
                .plan(&backend, &target, &MutationVerb::Restart)
                .is_err()
        );
    }

    fn install_spec() -> ContainerSpec {
        ContainerSpec {
            name: "web".to_string(),
            image: "nginx:latest".to_string(),
            ports: vec![PortMapping {
                host: 8080,
                container: 80,
                protocol: "tcp".to_string(),
            }],
            volumes: vec![VolumeMount {
                source: "web_data".to_string(),
                target: "/data".to_string(),
                kind: "volume".to_string(),
            }],
            env: vec![("TZ".to_string(), "UTC".to_string())],
            restart_policy: "unless-stopped".to_string(),
        }
    }

    #[test]
    fn container_install_plans_a_run_command() {
        let backend = DockerBackend;
        let target = MutationTarget::ContainerInstall {
            runtime: RuntimeKind::Docker,
            spec: install_spec(),
        };

        let plan = backend.plan(&target, &MutationVerb::Install).unwrap();

        assert_eq!(plan.program, "docker");
        assert_eq!(
            plan.args,
            vec![
                "run".to_string(),
                "-d".to_string(),
                "--name".to_string(),
                "web".to_string(),
                "--restart".to_string(),
                "unless-stopped".to_string(),
                "-p".to_string(),
                "8080:80/tcp".to_string(),
                "-v".to_string(),
                "web_data:/data".to_string(),
                "-e".to_string(),
                "TZ=UTC".to_string(),
                "nginx:latest".to_string(),
            ]
        );
        assert!(plan.requires_root);
    }

    #[test]
    fn container_install_rolls_back_by_removing_the_container() {
        let backend = DockerBackend;
        let target = MutationTarget::ContainerInstall {
            runtime: RuntimeKind::Docker,
            spec: install_spec(),
        };

        let rollback = build_rollback_plan(&backend, &target, &MutationVerb::Install).unwrap();

        assert_eq!(rollback.program, "docker");
        assert_eq!(
            rollback.args,
            vec!["rm".to_string(), "-f".to_string(), "web".to_string()]
        );
    }

    #[test]
    fn container_remove_uses_the_force_flag() {
        let backend = PodmanBackend;
        let target = MutationTarget::Container {
            runtime: RuntimeKind::Podman,
            name: "web".to_string(),
        };

        let plan = backend.plan(&target, &MutationVerb::Remove).unwrap();

        assert_eq!(plan.program, "podman");
        assert_eq!(
            plan.args,
            vec!["rm".to_string(), "-f".to_string(), "web".to_string()]
        );
    }

    #[test]
    fn container_install_target_is_verified_with_inspect() {
        let target = MutationTarget::ContainerInstall {
            runtime: RuntimeKind::Docker,
            spec: install_spec(),
        };

        let command = verification_command(&target).unwrap();

        assert_eq!(command.program, "docker");
        assert_eq!(
            command.args,
            vec![
                "inspect".to_string(),
                "--format={{.State.Running}}".to_string(),
                "web".to_string(),
            ]
        );
    }

    #[test]
    fn container_backends_reject_targets_from_another_runtime() {
        let backend = DockerBackend;
        let result = backend.plan(
            &MutationTarget::Container {
                runtime: RuntimeKind::Podman,
                name: "app".to_string(),
            },
            &MutationVerb::Start,
        );
        assert!(result.is_err());
    }

    #[test]
    fn install_requires_a_container_specification() {
        let backend = DockerBackend;
        let result = backend.plan(
            &MutationTarget::Container {
                runtime: RuntimeKind::Docker,
                name: "app".to_string(),
            },
            &MutationVerb::Install,
        );
        assert!(result.is_err());
    }

    #[test]
    fn service_backends_reject_install_and_remove() {
        let backend = SystemdBackend;
        let target = MutationTarget::Service {
            init: InitKind::Systemd,
            name: "demo.service".to_string(),
        };
        assert!(backend.plan(&target, &MutationVerb::Install).is_err());
        assert!(backend.plan(&target, &MutationVerb::Remove).is_err());
    }
}

pub fn rollback_verb(verb: MutationVerb) -> Option<MutationVerb> {
    match verb {
        MutationVerb::Start => Some(MutationVerb::Stop),
        MutationVerb::Stop => Some(MutationVerb::Start),
        MutationVerb::Restart => None,
        MutationVerb::Enable => Some(MutationVerb::Disable),
        MutationVerb::Disable => Some(MutationVerb::Enable),
        MutationVerb::Install => Some(MutationVerb::Remove),
        MutationVerb::Remove => None,
    }
}

pub fn mutation_target_name(target: &MutationTarget) -> &str {
    match target {
        MutationTarget::Container { name, .. } => name,
        MutationTarget::Service { name, .. } => name,
        MutationTarget::ContainerInstall { spec, .. } => &spec.name,
    }
}

impl MutationGateway {
    pub fn execute(
        &self,
        plan: &MutationPlan,
        runner: &dyn crate::platform::CommandRunner,
    ) -> Result<MutationExecutionReceipt, AnvilError> {
        if !self.can_mutate() {
            return Err(AnvilError::Permission(
                "read-only access cannot execute lifecycle mutations".to_string(),
            ));
        }

        if self.mode == MutationMode::DryRun {
            return Ok(MutationExecutionReceipt {
                executed: false,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            });
        }

        let output = runner
            .run(
                std::path::Path::new(plan.program),
                &plan.args.iter().map(|arg| arg.as_str()).collect::<Vec<_>>(),
            )
            .map_err(|error| AnvilError::Io {
                path: std::path::PathBuf::from(plan.program),
                source: error,
            })?;

        let status = output.status.unwrap_or(-1);
        if output.status != Some(0) {
            return Err(AnvilError::CommandExecution {
                program: plan.program.to_string(),
                status,
                stderr: if output.stderr.is_empty() {
                    output.stdout
                } else {
                    output.stderr
                },
            });
        }

        Ok(MutationExecutionReceipt {
            executed: true,
            exit_code: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    pub fn plan_and_execute(
        &self,
        backend: &dyn MutationBackend,
        target: &MutationTarget,
        verb: &MutationVerb,
        runner: &dyn crate::platform::CommandRunner,
    ) -> Result<MutationExecutionReceipt, AnvilError> {
        let plan = self.plan(backend, target, verb)?;
        self.execute(&plan, runner)
    }
}

#[cfg(test)]
mod execution_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockRunner {
        successes: Vec<bool>,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl crate::platform::CommandRunner for MockRunner {
        fn run(
            &self,
            program: &std::path::Path,
            args: &[&str],
        ) -> Result<crate::platform::CommandOutput, std::io::Error> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{} {}", program.display(), args.join(" ")));
            let success = *self
                .successes
                .get(self.calls.lock().unwrap().len() - 1)
                .unwrap_or(&true);
            Ok(crate::platform::CommandOutput {
                status: Some(if success { 0 } else { 1 }),
                stdout: String::new(),
                stderr: if success {
                    String::new()
                } else {
                    "failed".to_string()
                },
            })
        }
    }

    #[test]
    fn gateway_dry_run_does_not_call_runner() {
        let runner = MockRunner {
            successes: vec![true],
            calls: Arc::new(Mutex::new(Vec::new())),
        };
        let calls = Arc::clone(&runner.calls);
        let gateway = MutationGateway::new(
            crate::access::AccessContext {
                tier: crate::access::AccessTier::Root,
            },
            MutationMode::DryRun,
        );
        let target = MutationTarget::Service {
            init: InitKind::Systemd,
            name: "demo".to_string(),
        };
        let plan = gateway
            .plan(&SystemdBackend, &target, &MutationVerb::Start)
            .unwrap();

        let receipt = gateway.execute(&plan, &runner).unwrap();
        assert!(!receipt.executed);
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn gateway_execute_uses_command_array() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = MockRunner {
            successes: vec![true],
            calls: Arc::clone(&calls),
        };
        let gateway = MutationGateway::new(
            crate::access::AccessContext {
                tier: crate::access::AccessTier::Root,
            },
            MutationMode::Execute,
        );
        let target = MutationTarget::Service {
            init: InitKind::Systemd,
            name: "demo".to_string(),
        };
        let plan = gateway
            .plan(&SystemdBackend, &target, &MutationVerb::Start)
            .unwrap();
        let receipt = gateway.execute(&plan, &runner).unwrap();

        assert!(receipt.executed);
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["systemctl start demo".to_string()]
        );
    }

    #[test]
    fn gateway_execute_reports_nonzero_exit() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let runner = MockRunner {
            successes: vec![false],
            calls: Arc::clone(&calls),
        };
        let gateway = MutationGateway::new(
            crate::access::AccessContext {
                tier: crate::access::AccessTier::Root,
            },
            MutationMode::Execute,
        );
        let target = MutationTarget::Service {
            init: InitKind::Systemd,
            name: "demo".to_string(),
        };
        let plan = gateway
            .plan(&SystemdBackend, &target, &MutationVerb::Start)
            .unwrap();
        let result = gateway.execute(&plan, &runner);

        assert!(result.is_err());
        assert_eq!(calls.lock().unwrap().len(), 1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificationResult {
    pub success: bool,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub fn verification_command(
    target: &MutationTarget,
) -> Option<crate::provision::model::CommandPlan> {
    match target {
        MutationTarget::Container {
            runtime: RuntimeKind::Docker,
            name,
        } => Some(container_inspect_plan("docker", name)),
        MutationTarget::Container {
            runtime: RuntimeKind::Podman,
            name,
        } => Some(container_inspect_plan("podman", name)),
        MutationTarget::ContainerInstall { runtime, spec } => Some(container_inspect_plan(
            match runtime {
                RuntimeKind::Docker => "docker",
                RuntimeKind::Podman => "podman",
            },
            &spec.name,
        )),
        MutationTarget::Service {
            init: InitKind::Systemd,
            name,
        } => Some(crate::provision::model::CommandPlan {
            program: "systemctl".to_string(),
            args: vec!["is-active".to_string(), name.clone()],
        }),
        MutationTarget::Service {
            init: InitKind::Openrc,
            name,
        } => Some(crate::provision::model::CommandPlan {
            program: "rc-service".to_string(),
            args: vec![name.clone(), "status".to_string()],
        }),
    }
}

fn container_inspect_plan(program: &str, name: &str) -> crate::provision::model::CommandPlan {
    crate::provision::model::CommandPlan {
        program: program.to_string(),
        args: vec![
            "inspect".to_string(),
            "--format={{.State.Running}}".to_string(),
            name.to_string(),
        ],
    }
}

pub fn build_rollback_plan(
    backend: &dyn MutationBackend,
    target: &MutationTarget,
    verb: &MutationVerb,
) -> Result<MutationPlan, AnvilError> {
    let Some(reverse) = rollback_verb(*verb) else {
        return Err(AnvilError::Config(
            "unreversible lifecycle mutation".to_string(),
        ));
    };
    match target {
        // Rolling back an install removes the container it creates, so the
        // rollback targets that container rather than the specification.
        MutationTarget::ContainerInstall { runtime, spec } => {
            let created = MutationTarget::Container {
                runtime: runtime.clone(),
                name: spec.name.clone(),
            };
            backend.plan(&created, &reverse)
        }
        _ => backend.plan(target, &reverse),
    }
}

pub fn verify_mutation(
    target: &MutationTarget,
    runner: &dyn crate::platform::CommandRunner,
) -> Result<VerificationResult, AnvilError> {
    let Some(command) = verification_command(target) else {
        return Err(AnvilError::Config(
            "target has no verification command".to_string(),
        ));
    };
    let output = runner
        .run(
            std::path::Path::new(&command.program),
            &command
                .args
                .iter()
                .map(|arg| arg.as_str())
                .collect::<Vec<_>>(),
        )
        .map_err(|error| AnvilError::Io {
            path: std::path::PathBuf::from(&command.program),
            source: error,
        })?;
    let success = output.stdout.trim() == "true"
        || output.stdout.trim() == "active"
        || (output.status == Some(0)
            && output.stdout.trim().is_empty()
            && output.stderr.trim().is_empty());
    Ok(VerificationResult {
        success,
        command: format!("{} {}", command.program, command.args.join(" ")),
        exit_code: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

#[cfg(test)]
mod phase_twelve_tests {
    use super::*;

    #[test]
    fn verification_commands_cover_every_target() {
        let targets = [
            MutationTarget::Container {
                runtime: RuntimeKind::Docker,
                name: "app".to_string(),
            },
            MutationTarget::Container {
                runtime: RuntimeKind::Podman,
                name: "app".to_string(),
            },
            MutationTarget::Service {
                init: InitKind::Systemd,
                name: "demo.service".to_string(),
            },
            MutationTarget::Service {
                init: InitKind::Openrc,
                name: "demo".to_string(),
            },
        ];

        for target in targets {
            assert!(verification_command(&target).is_some());
        }
    }

    #[test]
    fn verification_succeeds_for_active_status() {
        struct Runner;
        impl crate::platform::CommandRunner for Runner {
            fn run(
                &self,
                program: &std::path::Path,
                args: &[&str],
            ) -> Result<crate::platform::CommandOutput, std::io::Error> {
                assert_eq!(program, std::path::Path::new("systemctl"));
                assert_eq!(args, ["is-active", "demo.service"]);
                Ok(crate::platform::CommandOutput {
                    status: Some(0),
                    stdout: "active".to_string(),
                    stderr: String::new(),
                })
            }
        }

        let result = verify_mutation(
            &MutationTarget::Service {
                init: InitKind::Systemd,
                name: "demo.service".to_string(),
            },
            &Runner,
        )
        .unwrap();
        assert!(result.success);
    }

    #[test]
    fn rollback_plan_preserves_root_gate() {
        let target = MutationTarget::Service {
            init: InitKind::Openrc,
            name: "demo".to_string(),
        };
        let plan = build_rollback_plan(&OpenrcBackend, &target, &MutationVerb::Start).unwrap();
        assert_eq!(plan.program, "rc-service");
        assert_eq!(plan.args, vec!["demo".to_string(), "stop".to_string()]);
    }
}
