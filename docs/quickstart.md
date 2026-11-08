# Anvil Quickstart

Anvil is a single static binary for homelab discovery, telemetry, storage/runtime inspection,
provisioning dry-runs, TUI operation, and plugin inventory.

## Build

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Target Linux only:

```sh
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

## Common Commands

```sh
cargo run -- --help
cargo run -- --read-only
cargo run -- discover
cargo run -- discover --json
cargo run -- storage
cargo run -- storage --json
cargo run -- runtime
cargo run -- runtime --json
cargo run -- plugins
cargo run -- plugins --json
cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run
cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run --json
```

## Paths

- Config: `/etc/anvil/config.toml`
- User config: `$XDG_CONFIG_HOME/anvil/config.toml`
- Manifests: `/etc/anvil/manifest.toml` or a directory containing `*.toml`
- Plugins: `/etc/anvil/plugins`, `$XDG_CONFIG_HOME/anvil/plugins`
- Logs: `/var/log/anvil/anvil.log`, otherwise `$XDG_STATE_HOME/anvil/anvil.log`

## Safety

- `apply` requires `--dry-run`.
- `discover`, `storage`, and `runtime` are read-only.
- Lifecycle operations remain gated by detected privileges and dry-run policy.
- Plugin manifests must use `api_version = 1` and point at trusted `.wasm` files.
