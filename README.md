# Anvil

A single static binary for Linux that reads the state of a homelab host and
lets you change it — deliberately.

Anvil discovers storage, network, init system and container runtimes, samples
telemetry, inspects pools and containers, installs applications from a
catalog, scans sandboxed WebAssembly plugins, and plans declarative
provisioning manifests. It runs as a terminal dashboard or as a set of
one-shot JSON reports.

Everything is read-only unless you explicitly ask for a mutation, and a
mutation needs **root** *and* `ANVIL_ALLOW_MUTATION=1` in the environment.

```
x86_64-unknown-linux-musl   aarch64-unknown-linux-musl   static, no runtime dependencies
```

---

## Install

Linux only, from the latest GitHub release:

```sh
curl -fsSL https://raw.githubusercontent.com/Crazygiscool/Anvil/main/scripts/install.sh | sh
```

Or from a locally built binary:

```sh
cargo build --release --target x86_64-unknown-linux-musl
./scripts/install.sh --from-local
```

Docker and Podman themselves are installed through your distribution's
package manager. Anvil can do it for you:

```sh
sh scripts/install-runtimes.sh --dry-run
sh scripts/install-runtimes.sh --runtime podman --execute --yes
```

---

## Use it

### The dashboard

```sh
anvil
```

Six tabs, all backed by real reads of the host:

| Tab | Shows |
|-----|-------|
| Overview | Host summary: system, discovery, storage, runtime, telemetry and recent mutations |
| Discovery | What the discovery pass found, with its diagnostics |
| Storage | ZFS pools, Btrfs subvolumes, LVM volumes and mounts |
| Runtime | Containers and init services, with start/stop/restart/remove |
| Telemetry | Live CPU, memory, disk and network counters |
| Catalog | The application catalog, with install and uninstall |

Press `h` for the controls line:

```
q quit | Tab/Shift+Tab switch tabs | h help | a actions | / filter | ? tab help
```

`Tab` and `Shift+Tab` switch tabs, `q` quits, and `?` opens the help for the
current tab.

### The reports

```sh
anvil discover          # host discovery
anvil storage           # storage inspection
anvil runtime           # container runtimes and the init system
anvil plugins           # Wasm plugin inventory
```

Every report has a `--json` form:

```sh
anvil runtime --json | jq '.container_runtimes'
```

The reports say what they found and nothing else. `anvil runtime` lists the
services the init system reports, with the state it reports:

```
container_runtimes=0 init=systemd init_detected=true services=136
  service sshd.service running
  service cron.service dead
```

A command that fails exits with a code that says what has to change: `2` for a
configuration problem, `3` for a permission problem, `1` for a runtime
failure.

### The catalog

```sh
anvil catalog list
anvil catalog list --category database
anvil catalog show nginx
```

Plan an install — this changes nothing and needs no privileges:

```sh
anvil catalog install nginx --dry-run
```

```
catalog install id=nginx
  action: docker API: create and start container 'nginx' from nginx:alpine
  command: docker run -d --name nginx --restart unless-stopped -p 8080:80/tcp nginx:alpine
  result: planned (dry-run), nothing was changed
```

Actually install it — this needs root **and** the environment variable:

```sh
sudo ANVIL_ALLOW_MUTATION=1 anvil catalog install nginx
```

The catalog is a TOML file you own. Add your own applications to
`/etc/anvil/catalog.toml`; see [`docs/catalog.md`](docs/catalog.md).

### Provisioning manifests

`apply` is always a dry run. It refuses to run without `--dry-run`:

```sh
anvil apply --manifest /etc/anvil/manifest.toml --dry-run
anvil apply --manifest /etc/anvil/manifest.toml --dry-run --json
```

---

## Safety model

| | |
|---|---|
| `discover`, `storage`, `runtime`, `plugins`, `catalog list/show` | Always read-only |
| `catalog install --dry-run` | Only plans; needs no privileges |
| `catalog install` | Needs root **and** `ANVIL_ALLOW_MUTATION=1` |
| TUI mutations | Same gate as the CLI |
| `apply` | Requires `--dry-run`; never changes the host |

`--read-only` forces read-only mode even for root. The environment variable
is deliberately not exposed as a CLI flag, so it cannot be switched on by
accident.

Every mutation produces a receipt that names the mechanism that carried it
out — the Docker API or a command line — and a verification that checks the
change actually took effect.

---

## Configuration

| | |
|---|---|
| Config (root) | `/etc/anvil/config.toml` |
| Config (user) | `$XDG_CONFIG_HOME/anvil/config.toml` |
| Manifests | `/etc/anvil/manifest.toml`, or a directory of `*.toml` |
| Catalog index (root) | `/etc/anvil/catalog.toml` |
| Catalog index (user) | `$XDG_CONFIG_HOME/anvil/catalog.toml` |
| Plugins | `/etc/anvil/plugins`, `$XDG_CONFIG_HOME/anvil/plugins` |
| Logs (root) | `/var/log/anvil/anvil.log` |
| Logs (user) | `$XDG_STATE_HOME/anvil/anvil.log` |

In TUI mode the logs go to the file only — never to stdout, which would
corrupt the display.

---

## Documentation

| | |
|---|---|
| [`docs/quickstart.md`](docs/quickstart.md) | Build and first steps |
| [`docs/architecture.md`](docs/architecture.md) | How it is put together |
| [`docs/roadmap.md`](docs/roadmap.md) | What is built, and what is not |
| [`docs/catalog.md`](docs/catalog.md) | The catalog index format and CLI |
| [`docs/plugins.md`](docs/plugins.md) | The plugin manifest and the sandbox |

---

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

The release targets need a musl C toolchain (`musl-tools`, or
`messense/rust-musl-cross-action` on CI):

```sh
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
file target/*/release/anvil
```

Release binaries are written to `target/<target>/release/anvil`.
