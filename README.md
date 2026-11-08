# Anvil

Zero-overhead homelab provisioning and telemetry TUI and headless daemon with docker and podman management, compiled as a
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
## Quick Install

Linux only, direct from the latest GitHub release:

```sh
./scripts/install.sh
```

To test a locally built release binary before publishing:

```sh
cargo build --release --target x86_64-unknown-linux-musl
./scripts/install.sh --from-local
```

## Native Container Runtimes

Install Docker and/or Podman through the Linux host's package manager:

```sh
sh scripts/install-runtimes.sh --dry-run
sh scripts/install-runtimes.sh --runtime podman --execute --yes
```

See `scripts/install-runtimes.sh --help` for supported distributions and daemon options.
```sh
curl -fsSL https://raw.githubusercontent.com/Crazygiscool/Anvil/main/scripts/install.sh | sh
wget -qO- https://raw.githubusercontent.com/Crazygiscool/Anvil/main/scripts/install.sh | sh
```
