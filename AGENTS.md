# AGENTS.md

Anvil is a Rust homelab management/provisioning TUI + headless daemon.

## Toolchain

- Rust is managed by `rust-toolchain.toml`; the toolchain has `rustfmt` and `clippy`.
- Use the stable toolchain binaries if shell tools are unavailable.
- Target Linux only: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`.

## Commands

- Format: `cargo fmt --check`
- Lint: `cargo clippy --all-targets -- -D warnings`
- Tests: `cargo test`
- Single test: `cargo test --test cli -- apply_dry_run_validates_versioned_manifest`
- Build: `cargo build`
- Release build: `cargo build --release`
- x86_64 release: `cargo build --release --target x86_64-unknown-linux-musl`
- aarch64 release: `cargo build --release --target aarch64-unknown-linux-musl`
- Help: `cargo run -- --help`
- TUI mode: `cargo run`
- Provisioning dry run: `cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run`
- Provisioning dry run JSON: `cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run --json`
- Read-only discovery: `cargo run -- discover`
- JSON discovery: `cargo run -- discover --json`
- Runtime inspection: `cargo run -- runtime`
- Plugin inventory: `cargo run -- plugins`
- JSON plugin inventory: `cargo run -- plugins --json`

## Current Implementation State

- Phase 1--13: CLI, config, privilege detection, logging, event loop, app state scaffolding, read-only host discovery, telemetry sampling/state updates, read-only storage and runtime inspection, declarative provisioning plan validation, interactive TUI rendering, Wasm plugin scanning, lifecycle mutation planning, gated mutation execution, provider command execution, centralized mutation controller/state orchestration, release documentation, build verification, and integration tests are implemented.
- `apply` requires `--dry-run`; it never mutates system state.
- `discover`, `discover --json`, `storage`, and `runtime` are read-only.
- Plugin report diagnostics distinguish invalid plugins without failing inventory.
- Container integrations and init-system execution remain capability-gated and dry-run by default; Wasm plugin loading uses restricted capabilities.
- Remove any pre-implementation assumptions before continuing.

## Architecture

- Single binary crate; entry point is `src/main.rs`.
- Discovery code lives under `src/discovery` and probes through traits in `src/platform/traits.rs`.
- Storage inspection lives under `src/storage` and uses trait backends.
- Runtime inspection lives under `src/runtime` and uses trait backends.
- Plugin scanning lives under `src/plugins` and uses Wasmtime to load sandboxed `.wasm` manifests.
- All later integration behavior must be hidden behind traits and tested with mocks.
- Interactive TUI mode is the default when `anvil` is invoked with no subcommand.
- Privilege model: root can mutate, non-root runs read-only unless `--read-only` is forced.

## Paths And Standards

- Config: root `/etc/anvil/config.toml`; user mode follows `XDG_CONFIG_HOME/anvil/config.toml` and the FHS/XDG defaults.
- Manifests: `/etc/anvil/manifest.toml` or a directory of `*.toml`.
- Logs: `/var/log/anvil/anvil.log` when root; otherwise XDG state home, then `~/.local/state/anvil/anvil.log`.
- Provisioning manifests require an explicit top-level `version = 1`.
- Plugin manifests use `api_version = 1` and `.wasm` libraries under configured plugin directories.

## Verification Expectations

- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` after code changes.
- Run both target release builds before publishing artifacts.
- Log changes must never write to terminal stdout in TUI mode.
- State mutation must stay centralized and permission-gated.
- Host API capabilities exposed to plugins must remain restricted and versioned.
