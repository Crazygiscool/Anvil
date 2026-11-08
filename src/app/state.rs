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
use std::path::PathBuf;
use std::sync::Arc;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Config,
    pub access: AccessContext,
    pub discovery: Option<DiscoveryReport>,
    pub storage: Option<StorageInspection>,
    pub runtime: Option<RuntimeInspection>,
    pub plugins: Option<PluginCatalog>,
    pub telemetry: Option<TelemetrySample>,
    pub manifest_paths: Vec<PathBuf>,
    pub running: bool,
    pub mutation_controller: Option<MutationController>,
    pub mutation_ledger: Vec<ControllerReceipt>,
}

#[allow(dead_code)]
impl AppState {
    pub fn bootstrap(config: Config, access: AccessContext, manifest_paths: Vec<PathBuf>) -> Self {
        Self {
            config,
            access,
            discovery: None,
            storage: None,
            runtime: None,
            plugins: None,
            telemetry: None,
            manifest_paths,
            running: true,
            mutation_controller: None,
            mutation_ledger: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn controller(&self) -> Option<&MutationController> {
        self.mutation_controller.as_ref()
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

        let receipt = controller.execute(backend, target, verb)?;
        self.mutation_controller = Some(controller);
        self.mutation_ledger.push(receipt.clone());
        Ok(receipt)
    }

    #[allow(dead_code)]
    pub fn mutation_audit(&self) -> &[ControllerReceipt] {
        &self.mutation_ledger
    }

    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn default_runner() -> Arc<dyn crate::platform::CommandRunner + Send + Sync> {
        Arc::new(OsCommandRunner)
    }
}
