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
