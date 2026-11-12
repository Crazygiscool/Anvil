#![allow(dead_code)]
#![allow(unused_imports)]
use crate::access::AccessContext;
use crate::error::AnvilError;
use crate::platform::CommandRunner;
use crate::runtime::container::{ContainerClient, ContainerClients, ContainerCreateRequest};
use crate::runtime::mutation::{
    ContainerApiAction, ExecutionMechanism, MutationBackend, MutationExecutionReceipt,
    MutationMode, MutationPlan, MutationTarget, MutationVerb, build_rollback_plan, rollback_verb,
    runtime_name, verify_mutation,
};
use serde::Serialize;
use std::ops::Deref;
use std::sync::Arc;
#[derive(Clone)]
pub struct MutationController {
    gateway: crate::runtime::mutation::MutationGateway,
    runner: Arc<dyn CommandRunner + Send + Sync>,
    clients: ContainerClients,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControllerReceipt {
    pub target: MutationTarget,
    pub verb: MutationVerb,
    pub plan: MutationPlan,
    pub execution: MutationExecutionReceipt,
    pub verification: Option<crate::runtime::mutation::VerificationResult>,
    pub rollback: Option<MutationPlan>,
}

impl MutationController {
    pub fn new(
        access: AccessContext,
        mode: crate::runtime::mutation::MutationMode,
        runner: Arc<dyn CommandRunner + Send + Sync>,
    ) -> Self {
        Self {
            gateway: crate::runtime::mutation::MutationGateway::new(access, mode),
            runner,
            clients: ContainerClients::empty(),
        }
    }

    /// Build a controller that can reach the container daemons through their
    /// API.
    ///
    /// A container mutation is then carried out through the API when the
    /// runtime has a client; everything else still runs its command line.
    pub fn with_clients(
        access: AccessContext,
        mode: crate::runtime::mutation::MutationMode,
        runner: Arc<dyn CommandRunner + Send + Sync>,
        clients: ContainerClients,
    ) -> Self {
        Self {
            gateway: crate::runtime::mutation::MutationGateway::new(access, mode),
            runner,
            clients,
        }
    }

    /// The container clients the controller can use.
    pub fn clients(&self) -> &ContainerClients {
        &self.clients
    }

    pub fn can_mutate(&self) -> bool {
        self.gateway.can_mutate()
    }

    /// The mode the controller runs in.
    pub fn mode(&self) -> MutationMode {
        self.gateway.mode()
    }

    pub fn plan(
        &self,
        backend: &dyn MutationBackend,
        target: &MutationTarget,
        verb: &MutationVerb,
    ) -> Result<MutationPlan, AnvilError> {
        self.gateway.plan(backend, target, verb)
    }

    pub fn execute(
        &self,
        backend: &dyn MutationBackend,
        target: &MutationTarget,
        verb: &MutationVerb,
    ) -> Result<ControllerReceipt, AnvilError> {
        if !self.can_mutate() {
            return Err(AnvilError::Permission(
                "read-only access cannot execute provider mutations".to_string(),
            ));
        }
        let plan = self.gateway.plan(backend, target, verb)?;
        let execution = self.execute_plan(&plan)?;
        let verification = if execution.executed {
            Some(self.verify_plan(&plan)?)
        } else {
            None
        };
        let rollback = build_rollback_plan(backend, target, &plan.verb).ok();

        Ok(ControllerReceipt {
            target: target.clone(),
            verb: *verb,
            plan,
            execution,
            verification,
            rollback,
        })
    }

    /// Carry out a plan.
    ///
    /// A container plan goes through the API when the runtime has a client;
    /// otherwise the command line runs, which is also the path for services.
    fn execute_plan(&self, plan: &MutationPlan) -> Result<MutationExecutionReceipt, AnvilError> {
        if let Some(action) = &plan.api_action
            && let Some(client) = self.client_for(plan)
        {
            return self.execute_via_api(client.as_ref(), action);
        }

        self.gateway.execute(plan, self.runner.as_ref())
    }

    /// The client for the runtime the plan targets.
    fn client_for(&self, plan: &MutationPlan) -> Option<&Arc<dyn ContainerClient>> {
        let runtime = match &plan.target {
            MutationTarget::Container { runtime, .. } => runtime,
            MutationTarget::ContainerInstall { runtime, .. } => runtime,
            MutationTarget::Service { .. } => return None,
        };
        self.clients.get(runtime_name(runtime))
    }

    /// Carry out a container plan through the daemon API.
    fn execute_via_api(
        &self,
        client: &dyn ContainerClient,
        action: &ContainerApiAction,
    ) -> Result<MutationExecutionReceipt, AnvilError> {
        if self.gateway.mode() == MutationMode::DryRun {
            return Ok(MutationExecutionReceipt {
                executed: false,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                mechanism: ExecutionMechanism::ContainerApi,
            });
        }

        match action {
            ContainerApiAction::Start { name } => client.start(name)?,
            ContainerApiAction::Stop { name } => client.stop(name)?,
            ContainerApiAction::Restart { name } => client.restart(name)?,
            ContainerApiAction::Remove { name } => client.remove(name)?,
            ContainerApiAction::Create { spec } => {
                let request = ContainerCreateRequest {
                    name: spec.name.clone(),
                    image: spec.image.clone(),
                    ports: spec.ports.clone(),
                    volumes: spec.volumes.clone(),
                    env: spec.env.clone(),
                    restart_policy: spec.restart_policy.clone(),
                };
                client.create(&request)?;
            }
        }

        Ok(MutationExecutionReceipt {
            executed: true,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            mechanism: ExecutionMechanism::ContainerApi,
        })
    }

    /// Check that a mutation had the effect it promised.
    fn verify_plan(
        &self,
        plan: &MutationPlan,
    ) -> Result<crate::runtime::mutation::VerificationResult, AnvilError> {
        if let Some(action) = &plan.api_action
            && let Some(client) = self.client_for(plan)
        {
            return verify_via_api(client.as_ref(), action);
        }

        verify_mutation(&plan.target, self.runner.as_ref())
    }
}

/// Check a container mutation through the daemon API.
fn verify_via_api(
    client: &dyn ContainerClient,
    action: &ContainerApiAction,
) -> Result<crate::runtime::mutation::VerificationResult, AnvilError> {
    let daemon = client.runtime_name();
    let name = action.container_name();

    if matches!(action, ContainerApiAction::Remove { .. }) {
        // A removed container has to be gone from the daemon's list.
        let command = format!("{daemon} API: list containers");
        return match client.list_containers() {
            Ok(containers) => {
                let still_there = containers.iter().any(|container| container.name == name);
                Ok(crate::runtime::mutation::VerificationResult {
                    success: !still_there,
                    command,
                    exit_code: Some(0),
                    stdout: String::new(),
                    stderr: if still_there {
                        format!("the container '{name}' is still listed")
                    } else {
                        String::new()
                    },
                })
            }
            Err(error) => Ok(crate::runtime::mutation::VerificationResult {
                success: false,
                command,
                exit_code: None,
                stdout: String::new(),
                stderr: error.to_string(),
            }),
        };
    }

    let command = format!("{daemon} API: inspect {name}");
    match client.inspect(name) {
        Ok(details) => {
            let expected_running = !matches!(action, ContainerApiAction::Stop { .. });
            let success = details.running == expected_running;
            Ok(crate::runtime::mutation::VerificationResult {
                success,
                command,
                exit_code: Some(0),
                stdout: details.status,
                stderr: if success {
                    String::new()
                } else {
                    format!(
                        "the container '{name}' is {} but should be {}",
                        if details.running {
                            "running"
                        } else {
                            "stopped"
                        },
                        if expected_running {
                            "running"
                        } else {
                            "stopped"
                        }
                    )
                },
            })
        }
        Err(error) => Ok(crate::runtime::mutation::VerificationResult {
            success: false,
            command,
            exit_code: None,
            stdout: String::new(),
            stderr: error.to_string(),
        }),
    }
}

impl std::fmt::Debug for MutationController {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MutationController")
            .finish_non_exhaustive()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::mutation::MutationMode;
    use crate::runtime::mutation::SystemdBackend;

    struct StaticRunner {
        lines: Vec<(&'static str, crate::platform::CommandOutput)>,
    }

    impl crate::platform::CommandRunner for StaticRunner {
        fn run(
            &self,
            program: &std::path::Path,
            args: &[&str],
        ) -> Result<crate::platform::CommandOutput, std::io::Error> {
            let command = format!("{} {}", program.display(), args.join(" "));
            for (expected, output) in &self.lines {
                if command == *expected {
                    return Ok(output.clone());
                }
            }
            Err(std::io::Error::other(format!(
                "unexpected command {:?} {:?}",
                program, args
            )))
        }
    }

    #[test]
    fn controller_plans_without_running_an_executor() {
        let runner = Arc::new(StaticRunner { lines: Vec::new() });
        let controller = MutationController::new(
            crate::access::AccessContext {
                tier: crate::access::AccessTier::ReadOnly,
            },
            MutationMode::DryRun,
            runner,
        );
        let backend = SystemdBackend;
        let target = crate::runtime::mutation::MutationTarget::Service {
            init: crate::runtime::mutation::InitKind::Systemd,
            name: "demo".to_string(),
        };
        assert!(
            controller
                .plan(
                    &backend,
                    &target,
                    &crate::runtime::mutation::MutationVerb::Restart
                )
                .is_err()
        );
    }

    #[test]
    fn controller_builds_rollback_after_successful_execution() {
        let runner = Arc::new(StaticRunner {
            lines: vec![
                (
                    "systemctl start demo",
                    crate::platform::CommandOutput {
                        status: Some(0),
                        stdout: String::new(),
                        stderr: String::new(),
                    },
                ),
                (
                    "systemctl is-active demo",
                    crate::platform::CommandOutput {
                        status: Some(0),
                        stdout: "active".to_string(),
                        stderr: String::new(),
                    },
                ),
            ],
        });
        let controller = MutationController::new(
            crate::access::AccessContext {
                tier: crate::access::AccessTier::Root,
            },
            MutationMode::Execute,
            Arc::clone(&runner) as Arc<dyn crate::platform::CommandRunner + Send + Sync>,
        );
        let backend = SystemdBackend;
        let target = crate::runtime::mutation::MutationTarget::Service {
            init: crate::runtime::mutation::InitKind::Systemd,
            name: "demo".to_string(),
        };
        let result = controller
            .execute(
                &backend,
                &target,
                &crate::runtime::mutation::MutationVerb::Start,
            )
            .unwrap();
        let rollback = result.rollback.unwrap();
        assert!(result.execution.executed);
        assert_eq!(rollback.program, "systemctl");
        assert_eq!(rollback.args, vec!["stop".to_string(), "demo".to_string()]);
    }

    /// A container start goes through the API when the runtime has a client.
    #[test]
    fn a_container_mutation_uses_the_api_client() {
        let runner = Arc::new(StaticRunner { lines: Vec::new() });
        let mock = crate::runtime::container::MockContainerClient::new("docker");
        mock.set_details(
            "web",
            crate::runtime::container::ContainerDetails {
                id: "abc".to_string(),
                name: "web".to_string(),
                image: "nginx:latest".to_string(),
                status: "running".to_string(),
                running: true,
                ports: Vec::new(),
            },
        );
        let clients = ContainerClients::new(vec![Arc::new(mock)]);

        let controller = MutationController::with_clients(
            crate::access::AccessContext {
                tier: crate::access::AccessTier::Root,
            },
            MutationMode::Execute,
            Arc::clone(&runner) as Arc<dyn crate::platform::CommandRunner + Send + Sync>,
            clients,
        );

        let target = MutationTarget::Container {
            runtime: crate::runtime::mutation::RuntimeKind::Docker,
            name: "web".to_string(),
        };
        let receipt = controller
            .execute(
                &crate::runtime::mutation::DockerBackend,
                &target,
                &MutationVerb::Start,
            )
            .unwrap();

        assert!(receipt.execution.executed);
        assert_eq!(
            receipt.execution.mechanism,
            ExecutionMechanism::ContainerApi
        );
        let verification = receipt.verification.expect("the mutation is verified");
        assert!(verification.success);
    }

    /// Without a client the command line runs, as it always did.
    #[test]
    fn a_container_mutation_falls_back_to_the_command_line() {
        let runner = Arc::new(StaticRunner {
            lines: vec![
                (
                    "docker start web",
                    crate::platform::CommandOutput {
                        status: Some(0),
                        stdout: String::new(),
                        stderr: String::new(),
                    },
                ),
                (
                    "docker inspect --format={{.State.Running}} web",
                    crate::platform::CommandOutput {
                        status: Some(0),
                        stdout: "true".to_string(),
                        stderr: String::new(),
                    },
                ),
            ],
        });
        let controller = MutationController::new(
            crate::access::AccessContext {
                tier: crate::access::AccessTier::Root,
            },
            MutationMode::Execute,
            Arc::clone(&runner) as Arc<dyn crate::platform::CommandRunner + Send + Sync>,
        );

        let target = MutationTarget::Container {
            runtime: crate::runtime::mutation::RuntimeKind::Docker,
            name: "web".to_string(),
        };
        let receipt = controller
            .execute(
                &crate::runtime::mutation::DockerBackend,
                &target,
                &MutationVerb::Start,
            )
            .unwrap();

        assert!(receipt.execution.executed);
        assert_eq!(receipt.execution.mechanism, ExecutionMechanism::Command);
    }
}
