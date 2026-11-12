//! Loading, merging and validating the catalog index.
//!
//! An index is looked up in the standard locations and merged on top of the
//! built-in defaults, so a user file can add entries or replace individual
//! built-in ones without having to restate the whole catalog.

use crate::catalog::model::{
    CatalogEntry, CatalogIndex, CatalogPort, CatalogVolume, SUPPORTED_INDEX_VERSION,
};
use crate::error::{AnvilError, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The curated applications that ship with Anvil.
///
/// This is data, not behaviour: the same format is used for user-provided
/// index files.
const BUILTIN_INDEX_TOML: &str = r#"
version = 1

[[apps]]
id = "portainer"
name = "Portainer"
description = "Container management UI for Docker"
category = "management"
image = "portainer/portainer-ce:latest"
restart_policy = "unless-stopped"

[[apps.ports]]
host = 9000
container = 9000
protocol = "tcp"

[[apps.volumes]]
source = "portainer_data"
target = "/data"
kind = "volume"

[[apps]]
id = "nginx"
name = "nginx"
description = "HTTP server and reverse proxy"
category = "networking"
image = "nginx:alpine"
restart_policy = "unless-stopped"

[[apps.ports]]
host = 8080
container = 80
protocol = "tcp"

[[apps]]
id = "postgres"
name = "PostgreSQL"
description = "Relational database"
category = "database"
image = "postgres:17-alpine"
restart_policy = "unless-stopped"
env = { POSTGRES_PASSWORD = "change-me" }

[[apps.ports]]
host = 5432
container = 5432
protocol = "tcp"

[[apps.volumes]]
source = "postgres_data"
target = "/var/lib/postgresql/data"
kind = "volume"

[[apps]]
id = "redis"
name = "Redis"
description = "In-memory key/value store"
category = "database"
image = "redis:7-alpine"
restart_policy = "unless-stopped"

[[apps.ports]]
host = 6379
container = 6379
protocol = "tcp"

[[apps.volumes]]
source = "redis_data"
target = "/data"
kind = "volume"

[[apps]]
id = "prometheus"
name = "Prometheus"
description = "Metrics collection and storage"
category = "monitoring"
image = "prom/prometheus:latest"
restart_policy = "unless-stopped"

[[apps.ports]]
host = 9090
container = 9090
protocol = "tcp"

[[apps.volumes]]
source = "prometheus_data"
target = "/prometheus"
kind = "volume"

[[apps]]
id = "gitea"
name = "Gitea"
description = "Lightweight self-hosted git server"
category = "devops"
image = "gitea/gitea:latest"
restart_policy = "unless-stopped"

[[apps.ports]]
host = 3000
container = 3000
protocol = "tcp"

[[apps.volumes]]
source = "gitea_data"
target = "/data"
kind = "volume"
"#;

/// The built-in catalog index.
pub fn builtin_index() -> Result<CatalogIndex> {
    parse_index(BUILTIN_INDEX_TOML, Path::new("<builtin catalog>"))
}

/// Parse and validate a catalog index from TOML source.
pub fn parse_index(source: &str, path: &Path) -> Result<CatalogIndex> {
    let index: CatalogIndex = toml::from_str(source)
        .map_err(|error| AnvilError::Config(format!("{}: {error}", path.display())))?;

    if index.version != SUPPORTED_INDEX_VERSION {
        return Err(AnvilError::ManifestVersion {
            path: path.to_path_buf(),
            found: index.version,
            expected: SUPPORTED_INDEX_VERSION,
        });
    }

    validate_index(&index, path)?;
    Ok(index)
}

/// Read and validate a catalog index from a file.
pub fn read_index(path: &Path) -> Result<CatalogIndex> {
    let source = std::fs::read_to_string(path).map_err(|error| AnvilError::Io {
        path: path.to_path_buf(),
        source: error,
    })?;
    parse_index(&source, path)
}

/// Merge an overlay index on top of a base index.
///
/// Entries with the same identifier are replaced by the overlay; entries that
/// only exist in the base are kept.
pub fn merge_indexes(base: CatalogIndex, overlay: CatalogIndex) -> Result<CatalogIndex> {
    let mut apps: Vec<CatalogEntry> = Vec::with_capacity(base.apps.len() + overlay.apps.len());
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();

    for entry in base.apps.into_iter().chain(overlay.apps) {
        match seen.get(&entry.id) {
            Some(&index) => apps[index] = entry,
            None => {
                seen.insert(entry.id.clone(), apps.len());
                apps.push(entry);
            }
        }
    }

    let merged = CatalogIndex::new(apps);
    validate_index(&merged, Path::new("<merged catalog>"))?;
    Ok(merged)
}

/// The standard locations an index is loaded from, most specific first.
pub fn standard_index_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if crate::access::effective_uid() == 0 {
        paths.push(PathBuf::from("/etc/anvil/catalog.toml"));
    } else {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            paths.push(PathBuf::from(xdg).join("anvil/catalog.toml"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(home).join(".config/anvil/catalog.toml"));
        }
        paths.push(PathBuf::from("/etc/anvil/catalog.toml"));
    }
    paths
}

/// The path `anvil catalog update` writes to.
pub fn default_index_path() -> PathBuf {
    standard_index_paths()
        .into_iter()
        .next()
        .unwrap_or_else(|| PathBuf::from("/etc/anvil/catalog.toml"))
}

/// Load the catalog index: built-in defaults merged with the first index file
/// found in the standard locations.
pub fn load_index() -> Result<CatalogIndex> {
    let base = builtin_index()?;
    for path in standard_index_paths() {
        if path.is_file() {
            let overlay = read_index(&path)?;
            return merge_indexes(base, overlay);
        }
    }
    Ok(base)
}

/// Validate a catalog index.
pub fn validate_index(index: &CatalogIndex, path: &Path) -> Result<()> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (position, entry) in index.apps.iter().enumerate() {
        if let Some(previous) = seen.insert(&entry.id, position) {
            return Err(AnvilError::Config(format!(
                "{}: duplicate catalog id '{}' (entries {} and {})",
                path.display(),
                entry.id,
                previous + 1,
                position + 1
            )));
        }
        validate_entry(entry, path)?;
    }
    Ok(())
}

/// Validate a single catalog entry.
pub fn validate_entry(entry: &CatalogEntry, path: &Path) -> Result<()> {
    let fail = |message: String| {
        Err(AnvilError::Config(format!(
            "{}: catalog entry '{}': {message}",
            path.display(),
            entry.id
        )))
    };

    if !is_valid_identifier(&entry.id) {
        return fail("id must be lowercase alphanumeric with dashes".to_string());
    }
    if entry.name.trim().is_empty() {
        return fail("name must not be empty".to_string());
    }
    if !is_valid_identifier(&entry.category) {
        return fail("category must be lowercase alphanumeric with dashes".to_string());
    }
    if !is_valid_image(&entry.image) {
        return fail(format!(
            "image '{}' is not a safe image reference",
            entry.image
        ));
    }
    if entry.runtime != "docker" && entry.runtime != "podman" {
        return fail(format!(
            "runtime '{}' must be 'docker' or 'podman'",
            entry.runtime
        ));
    }
    if !is_valid_restart_policy(&entry.restart_policy) {
        return fail(format!(
            "restart policy '{}' is not supported",
            entry.restart_policy
        ));
    }

    for port in &entry.ports {
        validate_port(port, path, &entry.id)?;
    }
    for volume in &entry.volumes {
        validate_volume(volume, path, &entry.id)?;
    }
    for (key, value) in &entry.env {
        if !is_valid_env_name(key) {
            return fail(format!("environment variable '{key}' has an invalid name"));
        }
        if value.contains('\0') {
            return fail(format!("environment variable '{key}' contains a null byte"));
        }
    }

    Ok(())
}

fn validate_port(port: &CatalogPort, path: &Path, id: &str) -> Result<()> {
    if port.host == 0 || port.container == 0 {
        return Err(AnvilError::Config(format!(
            "{}: catalog entry '{id}': port numbers must be greater than zero",
            path.display()
        )));
    }
    if !matches!(port.protocol.as_str(), "tcp" | "udp" | "sctp") {
        return Err(AnvilError::Config(format!(
            "{}: catalog entry '{id}': protocol '{}' must be tcp, udp or sctp",
            path.display(),
            port.protocol
        )));
    }
    Ok(())
}

fn validate_volume(volume: &CatalogVolume, path: &Path, id: &str) -> Result<()> {
    let fail = |message: String| {
        Err(AnvilError::Config(format!(
            "{}: catalog entry '{id}': volume '{}': {message}",
            path.display(),
            volume.source
        )))
    };

    if !volume.target.starts_with('/') {
        return fail("target must be an absolute path".to_string());
    }

    match volume.kind.as_str() {
        "volume" => {
            if volume.source.contains('/') || volume.source.contains(':') {
                return fail("named volume source must not contain '/' or ':'".to_string());
            }
            if !is_valid_volume_name(&volume.source) {
                return fail(
                    "named volume source must be lowercase alphanumeric with dashes or underscores"
                        .to_string(),
                );
            }
        }
        "bind" => {
            if !volume.source.starts_with('/') {
                return fail("bind source must be an absolute path".to_string());
            }
            if is_protected_host_path(&volume.source) {
                return fail(format!(
                    "bind source '{}' is a protected system path",
                    volume.source
                ));
            }
        }
        other => return fail(format!("kind '{other}' must be 'volume' or 'bind'")),
    }
    Ok(())
}

/// Host paths a catalog entry must never bind-mount over.
fn is_protected_host_path(path: &str) -> bool {
    const PROTECTED: &[&str] = &[
        "/",
        "/bin",
        "/boot",
        "/dev",
        "/etc",
        "/lib",
        "/lib64",
        "/proc",
        "/root",
        "/sbin",
        "/sys",
        "/usr",
        "/var/lib/docker",
        "/var/lib/containers",
    ];
    let normalized = path.trim_end_matches('/');
    let normalized = if normalized.is_empty() {
        "/"
    } else {
        normalized
    };
    PROTECTED.contains(&normalized)
}

/// Identifiers are lowercase alphanumerics separated by single dashes.
fn is_valid_identifier(value: &str) -> bool {
    if value.is_empty() || value.starts_with('-') || value.ends_with('-') {
        return false;
    }
    value.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    })
}

/// Named volumes may also contain underscores, like Docker volume names.
fn is_valid_volume_name(value: &str) -> bool {
    if value.is_empty() || value.starts_with('-') || value.ends_with('-') {
        return false;
    }
    value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || character == '_'
    })
}

/// Image references must not contain whitespace or shell metacharacters.
fn is_valid_image(value: &str) -> bool {
    if value.is_empty() || value.starts_with('-') {
        return false;
    }
    if value
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return false;
    }
    !value
        .chars()
        .any(|character| ";&|$`<>(){}[]!*?\"'\\".contains(character))
}

fn is_valid_env_name(value: &str) -> bool {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn is_valid_restart_policy(value: &str) -> bool {
    matches!(
        value,
        "no" | "always" | "unless-stopped" | "on-failure" | "unless-stopped:5" | "on-failure:5"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> &'static Path {
        Path::new("test.toml")
    }

    fn entry() -> CatalogEntry {
        CatalogEntry {
            id: "demo".to_string(),
            name: "Demo".to_string(),
            description: String::new(),
            category: "testing".to_string(),
            image: "nginx:alpine".to_string(),
            runtime: "docker".to_string(),
            restart_policy: "unless-stopped".to_string(),
            ports: Vec::new(),
            volumes: Vec::new(),
            env: BTreeMap::new(),
        }
    }

    #[test]
    fn builtin_index_is_valid() {
        let index = builtin_index().unwrap();
        assert_eq!(index.version, SUPPORTED_INDEX_VERSION);
        assert!(!index.apps.is_empty());
        assert!(index.find("nginx").is_some());
    }

    #[test]
    fn builtin_index_has_unique_ids() {
        let index = builtin_index().unwrap();
        let mut ids: Vec<&str> = index.apps.iter().map(|entry| entry.id.as_str()).collect();
        ids.sort_unstable();
        let total = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), total);
    }

    #[test]
    fn index_parses_from_toml() {
        let source = r#"
version = 1

[[apps]]
id = "demo"
name = "Demo"
category = "testing"
image = "nginx:alpine"

[[apps.ports]]
host = 8080
container = 80
"#;
        let index = parse_index(source, path()).unwrap();
        assert_eq!(index.apps.len(), 1);
        assert_eq!(index.apps[0].ports[0].host, 8080);
        assert_eq!(index.apps[0].runtime, "docker");
        assert_eq!(index.apps[0].restart_policy, "unless-stopped");
    }

    #[test]
    fn index_rejects_a_wrong_version() {
        let source = "version = 99\n";
        assert!(parse_index(source, path()).is_err());
    }

    #[test]
    fn index_rejects_duplicate_ids() {
        let source = r#"
version = 1

[[apps]]
id = "demo"
name = "Demo"
category = "testing"
image = "nginx:alpine"

[[apps]]
id = "demo"
name = "Demo again"
category = "testing"
image = "nginx:alpine"
"#;
        assert!(parse_index(source, path()).is_err());
    }

    #[test]
    fn index_rejects_unknown_fields() {
        let source = r#"
version = 1

[[apps]]
id = "demo"
name = "Demo"
category = "testing"
image = "nginx:alpine"
mystery = true
"#;
        assert!(parse_index(source, path()).is_err());
    }

    #[test]
    fn validation_rejects_unsafe_image_references() {
        let mut entry = entry();
        for image in [
            "nginx; rm -rf /",
            "nginx && reboot",
            "nginx | tee",
            "nginx alpine",
            "-nginx",
            "",
        ] {
            entry.image = image.to_string();
            assert!(
                validate_entry(&entry, path()).is_err(),
                "image {image:?} should be rejected"
            );
        }
    }

    #[test]
    fn validation_rejects_bad_identifiers() {
        let mut entry = entry();
        entry.id = "Not Valid".to_string();
        assert!(validate_entry(&entry, path()).is_err());

        entry.id = "-leading".to_string();
        assert!(validate_entry(&entry, path()).is_err());

        entry.id = "trailing-".to_string();
        assert!(validate_entry(&entry, path()).is_err());

        entry.id = "valid-id".to_string();
        assert!(validate_entry(&entry, path()).is_ok());
    }

    #[test]
    fn validation_rejects_unknown_runtimes() {
        let mut entry = entry();
        entry.runtime = "containerd".to_string();
        assert!(validate_entry(&entry, path()).is_err());
    }

    #[test]
    fn validation_rejects_protected_bind_mounts() {
        let mut entry = entry();
        entry.volumes.push(CatalogVolume {
            source: "/etc".to_string(),
            target: "/host-etc".to_string(),
            kind: "bind".to_string(),
        });
        assert!(validate_entry(&entry, path()).is_err());

        entry.volumes[0].source = "/srv/media".to_string();
        assert!(validate_entry(&entry, path()).is_ok());
    }

    #[test]
    fn validation_requires_absolute_volume_targets() {
        let mut entry = entry();
        entry.volumes.push(CatalogVolume {
            source: "data".to_string(),
            target: "relative".to_string(),
            kind: "volume".to_string(),
        });
        assert!(validate_entry(&entry, path()).is_err());
    }

    #[test]
    fn validation_rejects_zero_ports() {
        let mut entry = entry();
        entry.ports.push(CatalogPort {
            host: 0,
            container: 80,
            protocol: "tcp".to_string(),
        });
        assert!(validate_entry(&entry, path()).is_err());
    }

    #[test]
    fn validation_rejects_invalid_environment_names() {
        let mut entry = entry();
        entry
            .env
            .insert("BAD NAME".to_string(), "value".to_string());
        assert!(validate_entry(&entry, path()).is_err());
    }

    #[test]
    fn merge_replaces_entries_with_the_same_id() {
        let base = CatalogIndex::new(vec![entry()]);
        let mut overlay_entry = entry();
        overlay_entry.name = "Replaced".to_string();
        let overlay = CatalogIndex::new(vec![overlay_entry]);

        let merged = merge_indexes(base, overlay).unwrap();

        assert_eq!(merged.apps.len(), 1);
        assert_eq!(merged.apps[0].name, "Replaced");
    }

    #[test]
    fn merge_keeps_entries_that_only_exist_in_the_base() {
        let base = CatalogIndex::new(vec![entry()]);
        let mut other = entry();
        other.id = "other".to_string();
        let overlay = CatalogIndex::new(vec![other]);

        let merged = merge_indexes(base, overlay).unwrap();

        assert_eq!(merged.apps.len(), 2);
    }

    #[test]
    fn categories_are_sorted_and_unique() {
        let index = builtin_index().unwrap();
        let categories = index.categories();
        let mut sorted = categories.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(categories, sorted);
    }
}
