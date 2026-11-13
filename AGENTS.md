# AGENTS.md

Anvil is a Rust homelab management/provisioning TUI + headless daemon. Single static binary targeting Linux only.

## Toolchain

- Rust managed by `rust-toolchain.toml` (stable, with `clippy` and `rustfmt`)
- Targets: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`

## Commands

```sh
# Development
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --test cli -- apply_dry_run_validates_versioned_manifest  # single test

# Build
cargo build
cargo build --release
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl

# Run modes
cargo run -- --help
cargo run                              # TUI mode (default)
cargo run -- --read-only               # force read-only
cargo run -- discover                  # host discovery
cargo run -- discover --json
cargo run -- storage                   # storage inspection
cargo run -- storage --json
cargo run -- runtime                   # container/init runtime inspection
cargo run -- runtime --json
cargo run -- plugins                   # Wasm plugin inventory
cargo run -- plugins --json
cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run
cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run --json

# Application catalog
cargo run -- catalog list
cargo run -- catalog list --json --category media
cargo run -- catalog show portainer
cargo run -- catalog install portainer --dry-run
cargo run -- catalog update --from-file ./catalog.toml

# Native runtime installer (optional Docker/Podman)
sh scripts/install-runtimes.sh --dry-run
sh scripts/install-runtimes.sh --runtime podman --execute --yes
```

## Key Constraints

- `apply` **requires** `--dry-run`; never mutates system state
- `discover`, `storage`, `runtime`, `plugins` are read-only
- TUI mode: logs go to file only, never stdout
- Root can mutate; non-root runs read-only unless `--read-only` forced
- Mutation gated by `ANVIL_ALLOW_MUTATION=1` env var (not exposed via CLI)

## Architecture

- Single binary crate; entry point `src/main.rs`
- Discovery: `src/discovery` via traits in `src/platform/traits.rs`
- Storage inspection: `src/storage` with trait backends
- Runtime inspection: `src/runtime` with trait backends
- Container runtimes: `src/runtime/container` — the `ContainerClient` trait, the
  `bollard` API client, the command line fallback client, and the mock client
- Catalog: `src/catalog` — index model, index loading/merging/validation, and
  the install planner that reuses the mutation stack
- Plugin scanning: `src/plugins` uses Wasmtime for sandboxed `.wasm` manifests
- All integration behavior hidden behind traits, tested with mocks
- Config loader: `src/config/loader.rs` (root `/etc/anvil/config.toml`, user `XDG_CONFIG_HOME`)

## Current Implementation State

- `apply` requires `--dry-run`; it never mutates system state.
- `discover`, `storage`, `runtime`, `plugins` and `catalog list/show` are read-only.
- `catalog install` without `--dry-run` goes through the mutation controller, so
  it needs root **and** `ANVIL_ALLOW_MUTATION=1`.
- Container state is read through the Docker API when the runtime exposes a
  socket, and through the runtime's command line otherwise.
- `connect()` waits for the daemon's version probe, so the client already
  knows whether the daemon answered when it is handed out.
- A daemon that answers with an error has answered; only a transport failure
  marks the daemon unreachable.
- The UI never shows simulated data: every number comes from a real read, and
  every mutation receipt names the mechanism that carried it out.
- The init system is read for real: `systemctl list-units` and `rc-status
  --all` are parsed into the service list the runtime tab shows, and the
  service mutations go through `SystemdBackend`/`OpenrcBackend`.
- Creating a container through the API pulls the image first; the daemon does
  not fetch it when the container is created.
- The CLI exits with a code that says what has to change: 2 for a
  configuration error, 3 for a permission error, 1 for a runtime failure.
- There is no `#[allow(dead_code)]` in `src/`; dead code is removed or gated
  with `#[cfg(test)]` instead.
- Plugin report diagnostics distinguish invalid plugins without failing inventory.
- The Wasm plugin host does not expose host functions yet; the sandbox is
  capability-gated and versioned.

## Phase Ledger

Phases 1–14 are the only record of the original roadmap, so they are kept here.

| Phase | Scope | Commit |
|-------|-------|--------|
| 1–4 | Core foundation: CLI, config loader, privilege detection, logging, event loop, app state scaffolding | `3d2c5ac` |
| 5–7 | Read-only host discovery, telemetry sampling, storage and runtime inspection, provider integration, declarative provisioning plan validation, interactive TUI rendering | `7fba061` |
| 8 | Wasm plugin scanning through Wasmtime with restricted capabilities | `91bf7e0` |
| 9 | Release packaging, CI workflow, target verification | `41899d2` |
| 10 | Trait-backed lifecycle mutation planning | `890deec` |
| 11 | Gated lifecycle and provider mutation execution | `ecddd2a` |
| 12 | Provider verification, rollback helpers, safe execution controller | `eaa8a4d`, `9d7d0d0` |
| 13 | Centralized runtime mutation planning and state orchestration | `fb60f0a`, `ebc17f8` |
| 14 | Optional JSON output, storage-compatible provider plans, provider status contract | `196cff7`, `fe13a82` |
| 15 | TUI rewrite: tab manager, reusable widgets, honest telemetry and storage tabs | `8fe5e2c`, `b90c929` |
| 16 | TUI mutation wiring: container lifecycle, live refresh, audit ledger | `378a945` |
| 17 | Real catalog index, validation, `anvil catalog` CLI, catalog tab | `9e3828d`, `9bcac18` |
| 18 | Container API through `bollard`, structured inspection, client caching, CI smoke job | `a9d2e07`, `beb4669`, `4bedd75` |
| 19 | Docs and release alignment: architecture, roadmap, catalog, plugins, README, v0.2.0 | `956a3c9`, `971e067`, `9292ddb`, `37b137a`, `633fae8` |
| 20 | Init services read for real and mutable, the pull-before-create fix, meaningful exit codes, the dead code removed | `a6bd2a0`, `5b29fc3`, `a806dc0`, `9cf6d82`, `546399a` |

## Paths & Standards

| Type | Path |
|------|------|
| Config (root) | `/etc/anvil/config.toml` |
| Config (user) | `$XDG_CONFIG_HOME/anvil/config.toml` |
| Manifests | `/etc/anvil/manifest.toml` or dir of `*.toml` |
| Catalog index (root) | `/etc/anvil/catalog.toml` |
| Catalog index (user) | `$XDG_CONFIG_HOME/anvil/catalog.toml` |
| Plugins | `/etc/anvil/plugins`, `$XDG_CONFIG_HOME/anvil/plugins` |
| Logs (root) | `/var/log/anvil/anvil.log` |
| Logs (user) | `$XDG_STATE_HOME/anvil/anvil.log` |

- Provisioning manifests: `version = 1` (top-level, required)
- Plugin manifests: `api_version = 1`, `.wasm` files in plugin dirs
- Catalog index: `version = 1` (top-level, required)

## Verification Checklist

After changes, run:
1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`
4. Both release targets before publishing: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`

## Release

```sh
./scripts/install.sh              # install latest GitHub release
./scripts/install.sh --from-local # install locally built binary
```

Release binaries at `target/<target>/release/anvil`. The release history
is in `CHANGELOG.md`; the release notes the workflow publishes are in
`.github/workflows/release.yml` and have to be refreshed for each tag.
