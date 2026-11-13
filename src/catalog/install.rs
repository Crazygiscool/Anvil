//! Turning a catalog entry into a mutation plan.
//!
//! Installs reuse the existing mutation stack: the entry becomes a
//! [`ContainerSpec`], the spec becomes a [`MutationTarget`], and the backend
//! produces the same [`MutationPlan`] every other mutation uses. The
//! privilege gate, the receipt, the verification step and the rollback plan
//! therefore apply to installs exactly as they do to a container restart.

use crate::catalog::model::CatalogEntry;
use crate::error::{AnvilError, Result};
use crate::runtime::mutation::{
    ContainerSpec, DockerBackend, MutationBackend, MutationPlan, MutationTarget, MutationVerb,
    PodmanBackend, PortMapping, RuntimeKind, VolumeMount,
};

/// The backend that handles a runtime name.
pub fn backend_for(runtime: &str) -> Option<&'static dyn MutationBackend> {
    match runtime {
        "docker" => Some(&DockerBackend),
        "podman" => Some(&PodmanBackend),
        _ => None,
    }
}

/// The runtime kind for a runtime name.
pub fn runtime_kind(runtime: &str) -> Option<RuntimeKind> {
    match runtime {
        "docker" => Some(RuntimeKind::Docker),
        "podman" => Some(RuntimeKind::Podman),
        _ => None,
    }
}

/// Build the container specification for a catalog entry.
pub fn container_spec(entry: &CatalogEntry) -> Result<ContainerSpec> {
    let Some(runtime) = runtime_kind(&entry.runtime) else {
        return Err(AnvilError::Config(format!(
            "catalog entry '{}': unsupported runtime '{}'",
            entry.id, entry.runtime
        )));
    };
    let _ = runtime;

    let ports = entry
        .ports
        .iter()
        .map(|port| PortMapping {
            host: port.host,
            container: port.container,
            protocol: port.protocol.clone(),
        })
        .collect();

    let volumes = entry
        .volumes
        .iter()
        .map(|volume| VolumeMount {
            source: volume.source.clone(),
            target: volume.target.clone(),
            kind: volume.kind.clone(),
        })
        .collect();

    let env = entry
        .env
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();

    Ok(ContainerSpec {
        name: entry.id.clone(),
        image: entry.image.clone(),
        ports,
        volumes,
        env,
        restart_policy: entry.restart_policy.clone(),
    })
}

/// Build the mutation target that installs a catalog entry.
pub fn install_target(entry: &CatalogEntry) -> Result<MutationTarget> {
    let Some(runtime) = runtime_kind(&entry.runtime) else {
        return Err(AnvilError::Config(format!(
            "catalog entry '{}': unsupported runtime '{}'",
            entry.id, entry.runtime
        )));
    };

    Ok(MutationTarget::ContainerInstall {
        runtime,
        spec: container_spec(entry)?,
    })
}

/// Plan the installation of a catalog entry.
pub fn install_plan(entry: &CatalogEntry) -> Result<MutationPlan> {
    let backend = backend_for(&entry.runtime).ok_or_else(|| {
        AnvilError::Config(format!(
            "catalog entry '{}': unsupported runtime '{}'",
            entry.id, entry.runtime
        ))
    })?;
    let target = install_target(entry)?;
    backend.plan(&target, &MutationVerb::Install)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::model::{CatalogPort, CatalogVolume};
    use std::collections::BTreeMap;

    fn entry() -> CatalogEntry {
        CatalogEntry {
            id: "web".to_string(),
            name: "Web".to_string(),
            description: String::new(),
            category: "testing".to_string(),
            image: "nginx:alpine".to_string(),
            runtime: "docker".to_string(),
            restart_policy: "unless-stopped".to_string(),
            ports: vec![CatalogPort {
                host: 8080,
                container: 80,
                protocol: "tcp".to_string(),
            }],
            volumes: vec![CatalogVolume {
                source: "web_data".to_string(),
                target: "/data".to_string(),
                kind: "volume".to_string(),
            }],
            env: BTreeMap::from([("TZ".to_string(), "UTC".to_string())]),
        }
    }

    #[test]
    fn install_plan_uses_the_entry_runtime() {
        let plan = install_plan(&entry()).unwrap();

        assert_eq!(plan.program, "docker");
        assert_eq!(plan.args[0], "run");
        assert!(plan.args.contains(&"nginx:alpine".to_string()));
        assert!(plan.args.contains(&"8080:80/tcp".to_string()));
        assert!(plan.args.contains(&"web_data:/data".to_string()));
        assert!(plan.args.contains(&"TZ=UTC".to_string()));
    }

    #[test]
    fn install_plan_honours_podman_entries() {
        let mut entry = entry();
        entry.runtime = "podman".to_string();

        let plan = install_plan(&entry).unwrap();

        assert_eq!(plan.program, "podman");
    }

    #[test]
    fn install_plan_rejects_an_unknown_runtime() {
        let mut entry = entry();
        entry.runtime = "containerd".to_string();

        assert!(install_plan(&entry).is_err());
    }

    #[test]
    fn container_name_follows_the_entry_id() {
        let spec = container_spec(&entry()).unwrap();
        assert_eq!(spec.name, "web");
    }
}
