use crate::access::AccessContext;
use crate::config::Config;
use crate::discovery::DiscoveryReport;
use crate::platform::OsCommandRunner;
use crate::plugins::PluginCatalog;
use crate::runtime::RuntimeInspection;
use crate::runtime::controller::ControllerReceipt;
use crate::runtime::controller::MutationController;
use crate::storage::StorageInspection;
use crate::telemetry::TelemetrySample;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Config,
    pub access: AccessContext,
    pub discovery: Option<DiscoveryReport>,
    pub storage: Option<StorageInspection>,
    pub runtime: Option<RuntimeInspection>,
    pub plugins: Option<PluginCatalog>,
    pub telemetry: Option<TelemetrySample>,
    pub running: bool,
    pub mutation_controller: Option<MutationController>,
    pub mutation_ledger: Vec<ControllerReceipt>,
}

impl AppState {
    pub fn bootstrap(config: Config, access: AccessContext) -> Self {
        Self {
            config,
            access,
            discovery: None,
            storage: None,
            runtime: None,
            plugins: None,
            telemetry: None,
            running: true,
            mutation_controller: None,
            mutation_ledger: Vec::new(),
        }
    }

    pub fn plan_runtime_mutation(
        &mut self,
        backend: &dyn crate::runtime::mutation::MutationBackend,
        target: &crate::runtime::mutation::MutationTarget,
        verb: &crate::runtime::mutation::MutationVerb,
    ) -> crate::error::Result<crate::runtime::mutation::MutationPlan> {
        let controller = self.mutation_controller();
        controller.plan(backend, target, verb)
    }

    pub fn execute_runtime_mutation(
        &mut self,
        backend: &dyn crate::runtime::mutation::MutationBackend,
        target: &crate::runtime::mutation::MutationTarget,
        verb: &crate::runtime::mutation::MutationVerb,
    ) -> crate::error::Result<ControllerReceipt> {
        let controller = self.mutation_controller.take().unwrap_or_else(|| {
            MutationController::new(
                self.access,
                crate::runtime::mutation::MutationMode::DryRun,
                MutationController::default_runner(),
            )
        });

        // Keep the controller even when execution fails, so a failed call
        // cannot silently downgrade the mutation mode for later calls.
        let receipt = controller.execute(backend, target, verb);
        self.mutation_controller = Some(controller);

        let receipt = receipt?;
        self.mutation_ledger.push(receipt.clone());
        Ok(receipt)
    }

    /// The mutation mode of this session.
    ///
    /// `Some(Execute)` means a confirmed mutation really touches the host;
    /// `Some(DryRun)` means the session only plans. `None` means no
    /// controller has been set up yet.
    pub fn mutation_mode(&self) -> Option<crate::runtime::mutation::MutationMode> {
        self.mutation_controller
            .as_ref()
            .map(|controller| controller.mode())
    }

    pub fn mutation_audit(&self) -> &[ControllerReceipt] {
        &self.mutation_ledger
    }
    fn mutation_controller(&mut self) -> &mut MutationController {
        if self.mutation_controller.is_none() {
            let mode = if std::env::var("ANVIL_ALLOW_MUTATION").ok().as_deref() == Some("1") {
                crate::runtime::mutation::MutationMode::Execute
            } else {
                crate::runtime::mutation::MutationMode::DryRun
            };
            self.mutation_controller = Some(MutationController::new(
                self.access,
                mode,
                MutationController::default_runner(),
            ));
        }

        self.mutation_controller
            .as_mut()
            .expect("mutation controller was just initialized")
    }
}

impl MutationController {
    pub fn default_runner() -> Arc<dyn crate::platform::CommandRunner + Send + Sync> {
        Arc::new(OsCommandRunner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_controller_does_not_plan_lifecycle_mutation() {
        let mut state = AppState::bootstrap(Config::default(), AccessContext::detect(false));
        let result = state.plan_runtime_mutation(
            &crate::runtime::mutation::SystemdBackend,
            &crate::runtime::mutation::MutationTarget::Service {
                init: crate::runtime::mutation::InitKind::Systemd,
                name: "demo".to_string(),
            },
            &crate::runtime::mutation::MutationVerb::Restart,
        );
        assert!(result.is_err());
        assert!(state.mutation_audit().is_empty());
    }

    #[test]
    fn controller_records_executed_mutation_in_central_state() {
        let access = AccessContext {
            tier: crate::access::AccessTier::Root,
        };
        let mut state = AppState::bootstrap(Config::default(), access);
        state.mutation_controller = Some(MutationController::new(
            access,
            crate::runtime::mutation::MutationMode::DryRun,
            MutationController::default_runner(),
        ));
        let target = crate::runtime::mutation::MutationTarget::Service {
            init: crate::runtime::mutation::InitKind::Systemd,
            name: "demo".to_string(),
        };
        let receipt = state
            .execute_runtime_mutation(
                &crate::runtime::mutation::SystemdBackend,
                &target,
                &crate::runtime::mutation::MutationVerb::Restart,
            )
            .unwrap();
        assert!(!receipt.execution.executed);
        assert_eq!(state.mutation_audit().len(), 1);
    }
}
