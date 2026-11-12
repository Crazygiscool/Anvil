//! A container client that records what it was asked to do.
//!
//! The mock is what the tests use instead of a daemon: it keeps the calls in
//! memory, so a test can assert that a mutation really went through the API
//! instead of the command line.

use crate::error::Result;
use crate::runtime::container::{ContainerClient, ContainerCreateRequest, ContainerDetails};
use crate::runtime::model::ContainerSummary;
use std::collections::HashMap;
use std::sync::Mutex;

/// A client that answers from memory.
pub struct MockContainerClient {
    runtime: String,
    reachable: Mutex<bool>,
    status: Mutex<Option<String>>,
    containers: Mutex<Vec<ContainerSummary>>,
    details: Mutex<HashMap<String, ContainerDetails>>,
    calls: Mutex<Vec<String>>,
}

impl MockContainerClient {
    /// Build a mock for a runtime.
    pub fn new(runtime: &str) -> Self {
        Self {
            runtime: runtime.to_string(),
            reachable: Mutex::new(true),
            status: Mutex::new(Some("mock".to_string())),
            containers: Mutex::new(Vec::new()),
            details: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// The calls the client received, in order.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    /// The containers the mock reports.
    pub fn set_containers(&self, containers: Vec<ContainerSummary>) {
        *self.containers.lock().unwrap() = containers;
    }

    /// The details the mock reports for a container.
    pub fn set_details(&self, name: &str, details: ContainerDetails) {
        self.details
            .lock()
            .unwrap()
            .insert(name.to_string(), details);
    }

    /// Make the daemon unreachable.
    pub fn set_unreachable(&self) {
        *self.reachable.lock().unwrap() = false;
    }

    fn record(&self, call: &str) -> Result<()> {
        self.calls.lock().unwrap().push(call.to_string());
        if !*self.reachable.lock().unwrap() {
            return Err(crate::error::AnvilError::Runtime(format!(
                "{}: the daemon is not reachable",
                self.runtime
            )));
        }
        Ok(())
    }
}

impl ContainerClient for MockContainerClient {
    fn runtime_name(&self) -> &str {
        &self.runtime
    }

    fn is_reachable(&self) -> bool {
        *self.reachable.lock().unwrap()
    }

    fn daemon_status(&self) -> Option<String> {
        self.status.lock().unwrap().clone()
    }

    fn list_containers(&self) -> Result<Vec<ContainerSummary>> {
        self.record("list")?;
        Ok(self.containers.lock().unwrap().clone())
    }

    fn inspect(&self, name: &str) -> Result<ContainerDetails> {
        self.record(&format!("inspect {name}"))?;
        self.details
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or_else(|| {
                crate::error::AnvilError::Runtime(format!(
                    "{}: no such container: {name}",
                    self.runtime
                ))
            })
    }

    fn start(&self, name: &str) -> Result<()> {
        self.record(&format!("start {name}"))
    }

    fn stop(&self, name: &str) -> Result<()> {
        self.record(&format!("stop {name}"))
    }

    fn restart(&self, name: &str) -> Result<()> {
        self.record(&format!("restart {name}"))
    }

    fn remove(&self, name: &str) -> Result<()> {
        self.record(&format!("remove {name}"))
    }

    fn pull(&self, image: &str) -> Result<()> {
        self.record(&format!("pull {image}"))
    }

    fn create(&self, request: &ContainerCreateRequest) -> Result<String> {
        self.record(&format!("create {}", request.name))?;
        Ok(format!("id-{}", request.name))
    }
}
