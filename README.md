# Anvil

Zero-overhead homelab provisioning and telemetry TUI and headless daemon, compiled as a
single static binary.

## Commands

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Run modes:

```sh
cargo run -- --help
cargo run
cargo run -- discover
cargo run -- discover --json
cargo run -- storage
cargo run -- runtime
cargo run -- plugins
cargo run -- plugins --json
cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run
```

## Release

Target Linux only:

```sh
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

Release binaries are written to `target/<target>/release/anvil`. Move them into place with
your package manager, service manager, or other provisioning flow.

## Verification

After code changes, run format, clippy, and tests before building release artifacts:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```