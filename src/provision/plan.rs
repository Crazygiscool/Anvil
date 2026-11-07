use crate::error::AnvilError;
use crate::provision::model::*;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[allow(dead_code)]
pub trait Provisioner: Send + Sync {
    fn plan(&self, jobs: &[ManifestJob]) -> Result<ProvisionPlan, AnvilError>;
}

#[allow(dead_code)]
pub struct StandardProvisioner;

impl Provisioner for StandardProvisioner {
    fn plan(&self, jobs: &[ManifestJob]) -> Result<ProvisionPlan, AnvilError> {
        plan_manifest_jobs(jobs)
    }
}

pub fn parse_provisioning_manifest(path: &Path) -> Result<ProvisioningManifest, AnvilError> {
    let source = std::fs::read_to_string(path).map_err(|error| AnvilError::Io {
        path: path.to_path_buf(),
        source: error,
    })?;
    let manifest: ProvisioningManifest = toml::from_str(&source)
        .map_err(|error| AnvilError::Config(format!("{}: {error}", path.display())))?;
    validate_manifest(&manifest, path)?;
    Ok(manifest)
}

pub fn validate_manifest(manifest: &ProvisioningManifest, path: &Path) -> Result<(), AnvilError> {
    let expected_version = crate::config::SUPPORTED_VERSION;
    if manifest.version != expected_version {
        return Err(AnvilError::ManifestVersion {
            path: path.to_path_buf(),
            found: manifest.version,
            expected: expected_version,
        });
    }

    for storage in &manifest.storage {
        if storage.kind.trim().is_empty() || storage.kind == "unknown" {
            return Err(AnvilError::Config(format!(
                "{}: storage resource must declare a supported kind",
                path.display()
            )));
        }
    }
    for service in &manifest.services {
        if service.name.trim().is_empty() {
            return Err(AnvilError::Config(format!(
                "{}: service resource must have a non-empty name",
                path.display()
            )));
        }
    }
    for container in &manifest.containers {
        if container.name.trim().is_empty() || container.image.trim().is_empty() {
            return Err(AnvilError::Config(format!(
                "{}: container resource must have a non-empty name and image",
                path.display()
            )));
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub fn validate_top_level_version(
    manifest: &ProvisioningManifest,
    path: &Path,
) -> Result<(), AnvilError> {
    validate_manifest(manifest, path)
}

pub fn load_manifest_jobs(input: PathBuf) -> Result<Vec<ManifestJob>, AnvilError> {
    let mut paths = Vec::new();
    if input.is_dir() {
        for entry in std::fs::read_dir(&input).map_err(|error| AnvilError::Io {
            path: input.clone(),
            source: error,
        })? {
            let entry = entry.map_err(|error| AnvilError::Io {
                path: input.clone(),
                source: error,
            })?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some("toml") {
                paths.push(path);
            }
        }
        paths.sort();
        if paths.is_empty() {
            return Err(AnvilError::Config(format!(
                "no TOML manifests found in {}",
                input.display()
            )));
        }
    } else {
        paths.push(input);
    }

    let mut jobs = Vec::new();
    let mut seen = HashMap::new();
    for path in paths {
        let manifest = parse_provisioning_manifest(&path)?;
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("manifest")
            .to_string();
        if let Some(existing) = seen.insert(id.clone(), path.clone()) {
            return Err(AnvilError::Config(format!(
                "duplicate manifest id {id} from {} and {}",
                existing.display(),
                path.display()
            )));
        }
        jobs.push(ManifestJob {
            id,
            path,
            depends_on: manifest.depends_on.clone(),
            manifest,
        });
    }

    toposort_jobs(jobs)
}

pub fn toposort_jobs(jobs: Vec<ManifestJob>) -> Result<Vec<ManifestJob>, AnvilError> {
    let index: BTreeMap<&str, usize> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| (job.id.as_str(), index))
        .collect();
    let mut incoming = HashMap::new();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for job in &jobs {
        let mut dependencies = Vec::new();
        for dependency in &job.depends_on {
            let dependency = dependency.as_str();
            if !index.contains_key(dependency) {
                return Err(AnvilError::Config(format!(
                    "manifest {} depends on unknown {}",
                    job.path.display(),
                    dependency
                )));
            }
            dependencies.push(dependency);
        }
        incoming.insert(job.id.as_str(), dependencies.len());
        outgoing.entry(job.id.as_str()).or_default();
        for dependency in dependencies {
            outgoing
                .entry(dependency)
                .or_default()
                .push(job.id.as_str());
        }
    }

    let mut queue: Vec<&str> = incoming
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    queue.sort();
    let mut sorted = Vec::new();
    while let Some(id) = queue.first().copied() {
        queue.remove(0);
        sorted.push(id);
        let dependents = outgoing.get(id).cloned().unwrap_or_default();
        for dependent in dependents {
            if let Some(degree) = incoming.get_mut(dependent) {
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    queue.push(dependent);
                    queue.sort();
                }
            }
        }
    }
    if sorted.len() != jobs.len() {
        return Err(AnvilError::Config(
            "circular dependency in provisioning manifests".to_string(),
        ));
    }

    let by_id: HashMap<&str, usize> = jobs
        .iter()
        .enumerate()
        .map(|(index, job)| (job.id.as_str(), index))
        .collect();
    Ok(sorted
        .into_iter()
        .map(|id| {
            let index = by_id[id];
            jobs[index].clone()
        })
        .collect())
}

pub fn plan_manifest_jobs(jobs: &[ManifestJob]) -> Result<ProvisionPlan, AnvilError> {
    let mut plan = ProvisionPlan::default();
    let mut errors = 0;

    for job in jobs {
        let manifest = &job.manifest;
        if let Err(error) = validate_manifest(manifest, &job.path) {
            plan.actions.push(ProvisionAction {
                domain: "manifest",
                resource: job.id.clone(),
                summary: format!("invalid manifest: {error}"),
                command: None,
            });
            errors += 1;
            continue;
        }
        if true {
            plan.actions.push(ProvisionAction {
                domain: "manifest",
                resource: job.id.clone(),
                summary: format!(
                    "validated manifest {} (API v{}, {} storage, {} services, {} containers)",
                    job.path.display(),
                    manifest.version,
                    manifest.storage.len(),
                    manifest.services.len(),
                    manifest.containers.len()
                ),
                command: None,
            });

            for storage in &manifest.storage {
                let target = if storage.targets.is_empty() {
                    storage.pool.clone()
                } else {
                    format!("{} -> {}", storage.pool, storage.targets.join(", "))
                };
                plan.actions.push(ProvisionAction {
                    domain: "storage",
                    resource: storage
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("{}:{}", storage.kind, storage.pool)),
                    summary: format!(
                        "plan storage resource with kind {} for {}",
                        storage.kind, target
                    ),
                    command: Some(CommandPlan {
                        program: storage.kind.clone(),
                        args: vec!["plan".to_string(), target.clone()],
                    }),
                });
            }

            for service in &manifest.services {
                plan.actions.push(ProvisionAction {
                    domain: "service",
                    resource: service.name.clone(),
                    summary: format!(
                        "plan service registration: {} ({})",
                        service.name,
                        if service.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ),
                    command: Some(CommandPlan {
                        program: "systemctl".to_string(),
                        args: if service.enabled {
                            vec!["enable".to_string(), service.name.clone()]
                        } else {
                            vec!["disable".to_string(), service.name.clone()]
                        },
                    }),
                });
            }

            for container in &manifest.containers {
                plan.actions.push(ProvisionAction {
                    domain: "container",
                    resource: container.name.clone(),
                    summary: format!(
                        "plan container image pull and setup: {} ({}, {})",
                        container.name, container.image, container.runtime
                    ),
                    command: Some(CommandPlan {
                        program: container.runtime.clone(),
                        args: vec!["pull".to_string(), container.image.clone()],
                    }),
                });
            }
        }
    }

    plan.error_count = errors;
    Ok(plan)
}
