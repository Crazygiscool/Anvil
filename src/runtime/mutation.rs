#![allow(clippy::items_after_test_module)]
#![allow(dead_code)]
#![allow(clippy::clone_on_copy)]

use crate::error::AnvilError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeKind {
    Docker,
    Podman,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitKind {
    Systemd,
    Openrc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MutationVerb {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationTarget {
    Container { runtime: RuntimeKind, name: String },
    Service { init: InitKind, name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
        let MutationTarget::Container {
            runtime: RuntimeKind::Docker,
            name,
        } = target
        else {
            return Err(AnvilError::Config(
                "Docker backend received a non-Docker target".to_string(),
            ));
        };
        mutation_from_container_verb(name, verb, "docker")
    }
}

pub struct PodmanBackend;

impl MutationBackend for PodmanBackend {
    fn plan(
        &self,
        target: &MutationTarget,
        verb: &MutationVerb,
    ) -> Result<MutationPlan, AnvilError> {
        let MutationTarget::Container {
            runtime: RuntimeKind::Podman,
            name,
        } = target
        else {
            return Err(AnvilError::Config(
                "Podman backend received a non-Podman target".to_string(),
            ));
        };
        mutation_from_container_verb(name, verb, "podman")
    }
}

fn mutation_from_container_verb(
    name: &str,
    verb: &MutationVerb,
    program: &'static str,
) -> Result<MutationPlan, AnvilError> {
    let command_verb = match verb {
        MutationVerb::Start => "start",
        MutationVerb::Stop => "stop",
        MutationVerb::Restart => "restart",
        MutationVerb::Enable | MutationVerb::Disable => {
            return Err(AnvilError::Config(format!(
                "{program} container lifecycle does not support service enablement"
            )));
        }
    };
    Ok(MutationPlan {
        target: MutationTarget::Container {
            runtime: if program == "docker" {
                RuntimeKind::Docker
            } else {
                RuntimeKind::Podman
            },
            name: name.to_string(),
        },
        verb: *verb,
        program,
        args: vec![command_verb.to_string(), name.to_string()],
        requires_root: true,
        dry_run_only: true,
        summary: format!("plan container {} for {}", command_verb, name),
    })
}

pub struct MutationGateway {
    access: crate::access::AccessContext,
    mode: MutationMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationMode {
    DryRun,
}

impl MutationGateway {
    pub fn new(access: crate::access::AccessContext, mode: MutationMode) -> Self {
        Self { access, mode }
    }

    #[allow(dead_code)]
    pub fn can_mutate(&self) -> bool {
        self.access.allows_mutation()
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

impl PlannedAction {
    pub fn from_plan(plan: &MutationPlan) -> Self {
        Self {
            summary: plan.summary.clone(),
            command: plan.command_line(),
            executed: false,
        }
    }
}

fn verb_name(verb: MutationVerb) -> &'static str {
    match verb {
        MutationVerb::Start => "start service",
        MutationVerb::Stop => "stop service",
        MutationVerb::Restart => "restart service",
        MutationVerb::Enable => "enable service",
        MutationVerb::Disable => "disable service",
    }
}

#[allow(dead_code)]
pub fn mutation_kind(target: &MutationTarget) -> &'static str {
    match target {
        MutationTarget::Container { .. } => "container",
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
}

pub fn rollback_verb(verb: MutationVerb) -> Option<MutationVerb> {
    match verb {
        MutationVerb::Start => Some(MutationVerb::Stop),
        MutationVerb::Stop => Some(MutationVerb::Start),
        MutationVerb::Restart => None,
        MutationVerb::Enable => Some(MutationVerb::Disable),
        MutationVerb::Disable => Some(MutationVerb::Enable),
    }
}

pub fn mutation_target_name(target: &MutationTarget) -> &str {
    match target {
        MutationTarget::Container { name, .. } => name,
        MutationTarget::Service { name, .. } => name,
    }
}
