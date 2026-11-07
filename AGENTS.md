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
- Help: `cargo run -- --help`
- TUI mode: `cargo run`
- Provisioning dry run: `cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run`
- Read-only discovery: `cargo run -- discover`
- JSON discovery: `cargo run -- discover --json`
- Runtime inspection: `cargo run -- runtime`

## Current Implementation State

- Phase 1--7: CLI, config, privilege detection, logging, event loop, app state scaffolding, read-only host discovery, telemetry sampling/state updates, read-only storage and runtime inspection, declarative provisioning plan validation, and interactive TUI rendering are implemented.
- `apply` requires `--dry-run`; it never mutates system state.
- `discover`, `discover --json`, `storage`, and `runtime` are read-only.
- Container integrations and init-system mutation are not wired.
- Remove any pre-implementation assumptions before continuing.

## Architecture

- Single binary crate; entry point is `src/main.rs`.
- Discovery code lives under `src/discovery` and probes through traits in `src/platform/traits.rs`.
- All later integration behavior must be hidden behind traits and tested with mocks.
- Interactive TUI mode is the default when `anvil` is invoked with no subcommand.
- Privilege model: root can mutate, non-root runs read-only unless `--read-only` is forced.

## Paths And Standards

- Config: root `/etc/anvil/config.toml`; user mode follows `XDG_CONFIG_HOME/anvil/config.toml` and the FHS/XDG defaults.
- Manifests: `/etc/anvil/manifest.toml` or a directory of `*.toml`.
- Logs: `/var/log/anvil/anvil.log` when root; otherwise XDG state home, then `~/.local/state/anvil/anvil.log`.
- Provisioning manifests require an explicit top-level `version = 1`.

## Verification Expectations

- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` after code changes.
- Log changes must never write to terminal stdout in TUI mode.
- State mutation must stay centralized and permission-gated.
