use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_anvil"))
}

#[test]
fn help_exits_success() {
    let output = Command::new(bin()).arg("--help").output().unwrap();
    assert!(output.status.success());
}

#[test]
fn apply_without_dry_run_exits_failure() {
    let directory = tempfile_dir("apply-without-dry-run");
    let manifest = directory.join("manifest.toml");
    fs::write(&manifest, "version = 1\n").unwrap();
    let output = Command::new(bin())
        .arg("apply")
        .arg("--manifest")
        .arg(&manifest)
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn apply_dry_run_validates_versioned_manifest() {
    let directory = tempfile_dir("apply-dry-run-ok");
    let manifest = directory.join("manifest.toml");
    fs::write(&manifest, "version = 1\n").unwrap();
    let output = Command::new(bin())
        .arg("apply")
        .arg("--manifest")
        .arg(&manifest)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn apply_dry_run_rejects_wrong_version() {
    let directory = tempfile_dir("apply-wrong-version");
    let manifest = directory.join("manifest.toml");
    fs::write(&manifest, "version = 2\n").unwrap();
    let output = Command::new(bin())
        .arg("apply")
        .arg("--manifest")
        .arg(&manifest)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

fn tempfile_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push("anvil-integration-tests");
    path.push(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn discover_exits_success() {
    let output = Command::new(bin()).arg("discover").output().unwrap();
    assert!(output.status.success());
}

#[test]
fn discover_json_exits_success() {
    let output = Command::new(bin())
        .arg("discover")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("schema_version"));
    assert!(stdout.contains("\"binaries\""));
}

#[test]
fn plugin_inventory_exits_success() {
    let output = Command::new(bin()).arg("plugins").output().unwrap();
    assert!(output.status.success());
}

#[test]
fn plugin_inventory_json_exits_success() {
    let output = Command::new(bin())
        .arg("plugins")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("api_version"));
}

#[test]
fn runtime_json_exits_success() {
    let output = Command::new(bin())
        .arg("runtime")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("provider_integration"));
    assert!(stdout.contains("container_runtimes"));
    assert!(stdout.contains("provider_integration"));
}

#[test]
fn catalog_list_exits_success() {
    let output = Command::new(bin())
        .arg("catalog")
        .arg("list")
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("catalog entries="));
}

#[test]
fn catalog_list_json_exits_success() {
    let output = Command::new(bin())
        .arg("catalog")
        .arg("list")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"apps\""));
    assert!(stdout.contains("\"categories\""));
}

#[test]
fn catalog_show_prints_the_entry() {
    let output = Command::new(bin())
        .arg("catalog")
        .arg("show")
        .arg("nginx")
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("id=nginx"));
    assert!(stdout.contains("image=nginx:alpine"));
}

#[test]
fn catalog_show_rejects_an_unknown_entry() {
    let output = Command::new(bin())
        .arg("catalog")
        .arg("show")
        .arg("does-not-exist")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn catalog_install_dry_run_prints_the_command_without_touching_the_host() {
    let output = Command::new(bin())
        .arg("catalog")
        .arg("install")
        .arg("nginx")
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("docker run"));
    assert!(stdout.contains("nginx:alpine"));
    assert!(stdout.contains("nothing was changed"));
}

#[test]
fn catalog_install_without_root_is_refused() {
    // The integration tests never run as root in CI, so an install without
    // --dry-run has to be refused rather than executed.
    if effective_uid() == 0 {
        return;
    }

    let output = Command::new(bin())
        .arg("catalog")
        .arg("install")
        .arg("nginx")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn catalog_update_writes_a_validated_index() {
    let directory = tempfile_dir("catalog-update");
    let source = directory.join("catalog.toml");
    fs::write(
        &source,
        r#"
version = 1

[[apps]]
id = "demo"
name = "Demo"
category = "testing"
image = "nginx:alpine"
"#,
    )
    .unwrap();

    let config_home = directory.join("xdg");
    let output = Command::new(bin())
        .arg("catalog")
        .arg("update")
        .arg("--from-file")
        .arg(&source)
        .env("XDG_CONFIG_HOME", &config_home)
        .output()
        .unwrap();
    assert!(output.status.success());

    let written = config_home.join("anvil/catalog.toml");
    assert!(written.is_file());

    // The written index is picked up by a subsequent listing.
    let listed = Command::new(bin())
        .arg("catalog")
        .arg("list")
        .env("XDG_CONFIG_HOME", &config_home)
        .output()
        .unwrap();
    assert!(listed.status.success());
    let stdout = String::from_utf8(listed.stdout).unwrap();
    assert!(stdout.contains("demo"));
}

#[test]
fn catalog_update_rejects_an_invalid_index() {
    let directory = tempfile_dir("catalog-update-invalid");
    let source = directory.join("catalog.toml");
    fs::write(
        &source,
        r#"
version = 1

[[apps]]
id = "demo"
name = "Demo"
category = "testing"
image = "nginx; rm -rf /"
"#,
    )
    .unwrap();

    let config_home = directory.join("xdg");
    let output = Command::new(bin())
        .arg("catalog")
        .arg("update")
        .arg("--from-file")
        .arg(&source)
        .env("XDG_CONFIG_HOME", &config_home)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!config_home.join("anvil/catalog.toml").exists());
}

/// The effective uid of the test process, read without pulling in a crate.
fn effective_uid() -> u32 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(1000)
}

/// A configuration that cannot be found is a configuration error, which the
/// CLI reports with exit code 2.
#[test]
fn a_missing_config_file_exits_with_the_configuration_code() {
    let output = Command::new(bin())
        .arg("--config")
        .arg("/nonexistent/anvil/config.toml")
        .arg("discover")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
}

/// A manifest with the wrong version is a configuration error too.
#[test]
fn a_manifest_with_the_wrong_version_exits_with_the_configuration_code() {
    let directory = tempfile_dir("manifest-wrong-version-code");
    let manifest = directory.join("manifest.toml");
    fs::write(&manifest, "version = 2\n").unwrap();

    let output = Command::new(bin())
        .arg("apply")
        .arg("--manifest")
        .arg(&manifest)
        .arg("--dry-run")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
}

/// A mutation that the session is not allowed to make is a permission
/// error, which the CLI reports with exit code 3.
#[test]
fn a_refused_mutation_exits_with_the_permission_code() {
    if effective_uid() == 0 {
        return;
    }

    let output = Command::new(bin())
        .arg("catalog")
        .arg("install")
        .arg("nginx")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
}
