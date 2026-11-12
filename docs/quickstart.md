# Anvil Quickstart

Anvil is a single static binary for Linux that inspects a homelab host,
presents it as a terminal dashboard, and performs real — but deliberately
gated — mutations.

---

## Build

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

The release targets need a musl C toolchain. On Arch that is `musl-tools`;
on CI it is `messense/rust-musl-cross-action`:

```sh
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
file target/*/release/anvil
```

Both binaries should report `statically linked`.

---

## First look

Run the dashboard:

```sh
cargo run
```

The six tabs — Overview, Discovery, Storage, Runtime, Telemetry, Catalog —
are all backed by real reads of the host. Press `h` for the controls line:

```
q quit | Tab/Shift+Tab switch tabs | h help | a actions | / filter | ? tab help
```

`h` shows the global controls, `?` shows the help for the current tab, and
`Tab` / `Shift+Tab` move between tabs.

---

## Common commands

```sh
cargo run -- --help
cargo run -- --read-only            # force read-only, even as root
cargo run -- discover               # host discovery
cargo run -- discover --json
cargo run -- storage                # storage inspection
cargo run -- storage --json
cargo run -- runtime                # container runtimes and the init system
cargo run -- runtime --json
cargo run -- plugins                # Wasm plugin inventory
cargo run -- plugins --json
cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run
cargo run -- apply --manifest /etc/anvil/manifest.toml --dry-run --json
```

---

## The catalog

```sh
cargo run -- catalog list
cargo run -- catalog list --category database
cargo run -- catalog show nginx
cargo run -- catalog install nginx --dry-run
```

A dry run only plans — it needs no privileges and changes nothing:

```
catalog install id=nginx
  action: docker API: create and start container 'nginx' from nginx:alpine
  command: docker run -d --name nginx --restart unless-stopped -p 8080:80/tcp nginx:alpine
  result: planned (dry-run), nothing was changed
```

A real install needs **root** and `ANVIL_ALLOW_MUTATION=1`:

```sh
sudo ANVIL_ALLOW_MUTATION=1 anvil catalog install nginx
```

Add your own applications by writing a catalog index — see
[`catalog.md`](catalog.md) for the format and the validation rules.

---

## Paths

| | |
|---|---|
| Config | `/etc/anvil/config.toml` |
| User config | `$XDG_CONFIG_HOME/anvil/config.toml` |
| Manifests | `/etc/anvil/manifest.toml`, or a directory containing `*.toml` |
| Catalog index | `/etc/anvil/catalog.toml` |
| User catalog index | `$XDG_CONFIG_HOME/anvil/catalog.toml` |
| Plugins | `/etc/anvil/plugins`, `$XDG_CONFIG_HOME/anvil/plugins` |
| Logs | `/var/log/anvil/anvil.log`, otherwise `$XDG_STATE_HOME/anvil/anvil.log` |

---

## Safety

- `apply` requires `--dry-run` and never changes the host.
- `discover`, `storage`, `runtime`, `plugins` and `catalog list/show` are
  read-only.
- A mutation needs root **and** `ANVIL_ALLOW_MUTATION=1`. The variable is
  not exposed as a CLI flag.
- `--read-only` forces read-only mode even for root.
- In TUI mode the logs go to the log file only, never to stdout.
- Plugin manifests must use `api_version = 1`, name a relative `.wasm`
  library inside their own directory, and ask only for capabilities the host
  allows.

---

## Next

- [`architecture.md`](architecture.md) — how it is put together
- [`roadmap.md`](roadmap.md) — what is built, and what is not
- [`catalog.md`](catalog.md) — the catalog index format
- [`plugins.md`](plugins.md) — the plugin manifest and the sandbox
