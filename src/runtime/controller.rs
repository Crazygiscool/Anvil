#![allow(dead_code)]
#![allow(unused_imports)]
use crate::access::AccessContext;
use crate::error::AnvilError;
use crate::platform::CommandRunner;
use crate::runtime::mutation::{
    MutationBackend, MutationExecutionReceipt, MutationMode, MutationPlan, MutationTarget,
    MutationVerb, build_rollback_plan, rollback_verb, verify_mutation,
};
use std::ops::Deref;
use std::sync::Arc;
#[derive(Clone)]
pub struct MutationController {
    gateway: crate::runtime::mutation::MutationGateway,
    runner: Arc<dyn CommandRunner + Send + Sync>,
}

#[derive(Debug, Clone)]
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
        }
    }

    pub fn can_mutate(&self) -> bool {
        self.gateway.can_mutate()
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
        let execution = self.gateway.execute(&plan, self.runner.as_ref())?;
        let verification = if execution.executed {
            Some(verify_mutation(target, self.runner.as_ref())?)
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
    use crate::runtime::mutation::{MutationMode, SystemdBackend};

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
}
