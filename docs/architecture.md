# Anvil Architecture

Anvil is a single-binary homelab management tool for Linux. It reads the state
of a host — storage, network, init system, container runtimes, telemetry — and
presents it as a terminal dashboard, a set of JSON reports, and a small
catalog of applications you can install.

This document describes what Anvil actually does today. Where a feature is
planned but not built, it says so explicitly rather than describing an
aspiration.

---

## 1. Design principles

**One static binary.** Anvil ships as a single file for `x86_64` and
`aarch64` Linux, built against musl. There is no runtime, no interpreter and
no service to install. A host can be inspected with a copied binary.

**Read by default, write only when asked.** Everything Anvil does is a read
until a mutation is explicitly requested. The CLI's inspection commands never
write. The `apply` command refuses to run without `--dry-run`. Real mutations
live behind two gates: the process must be root, and `ANVIL_ALLOW_MUTATION=1`
must be set in the environment.

**Honest data.** The interface never invents a number. Every value in the TUI
comes from a real read of the host, and when a read fails the failure is shown
as a diagnostic rather than being smoothed into an empty list. A mutation
receipt names the mechanism that carried it out — the Docker API or a command
line — instead of implying that a command was run when it was not.

**Traits at the boundary.** Everything that touches the host goes through
`TextReader`, `CommandRunner`, `ContainerClient` or a backend trait. That is
what lets the whole integration layer be tested without a daemon, a ZFS pool or
a root shell.

---

## 2. Shape of the program

```
                    ┌──────────────────────────────────────────┐
                    │             anvil (one binary)           │
                    └───────────────────┬──────────────────────┘
                                        │
          ┌─────────────┬───────────────┼───────────────┬─────────────┐
          ▼             ▼               ▼               ▼             ▼
      ┌────────┐  ┌──────────┐   ┌──────────┐   ┌──────────┐  ┌──────────┐
      │  CLI   │  │   TUI    │   │ Discovery│   │  Storage │  │  Plugins │
      │ (clap) │  │(ratatui) │   │          │   │          │  │ (wasmtime│
      └────┬───┘  └────┬─────┘   └────┬─────┘   └────┬─────┘  └────┬─────┘
           │           │              │              │             │
           │      ┌────▼──────────────▼──────────────▼─────────────▼────┐
           │      │                  AppState                           │
           │      │  (RwLock-guarded snapshot of the host)              │
           │      └───────────────────────┬─────────────────────────────┘
           │                              │
           │      ┌───────────────────────▼─────────────────────────────┐
           │      │        MutationController (root + env gated)       │
           │      │   plan → confirm → execute → verify → rollback      │
           │      └───────────────────────┬─────────────────────────────┘
           │                              │
      ┌────▼──────────────────────────────▼─────────────────────────────┐
      │                    Host boundary (traits)                       │
      │  TextReader · CommandRunner · ContainerClient · PluginBackend   │
      └────┬───────────────┬───────────────────┬────────────────────────┘
           ▼               ▼                   ▼
       /proc, /sys    docker.sock,        wasmtime
       /etc, /run     podman.sock,        sandbox
       systemctl      docker/podman CLI
```

Both the CLI and the TUI drive the same `AppState` and the same
`MutationController`. There is no privileged code path that only exists in the
TUI.

---

## 3. Source layout

```
src/
├── main.rs                 Entry point: CLI parsing, mode dispatch, one-shot reports
├── cli.rs                  The clap command tree
├── access.rs               Privilege detection (root vs. read-only)
├── error.rs                The error taxonomy (`thiserror`)
├── logging.rs              File-only logging setup
│
├── config/                 Configuration model and loader
│   ├── model.rs            `Config` — version, refresh rates, manifest path
│   └── loader.rs           Reads and validates the config file
│
├── platform/               The host boundary traits
│   └── traits.rs           `TextReader`, `CommandRunner`, `OsTextReader`, `OsCommandRunner`
│
├── discovery/              Read-only host discovery
│   ├── model.rs            The report structures
│   ├── storage.rs          ZFS, Btrfs, LVM and mount discovery
│   ├── network.rs          Network interface enumeration
│   ├── runtime.rs          Container runtime and init system detection
│   └── collect.rs          The coordinator that assembles the report
│
├── storage/                Read-only storage inspection
│   ├── model.rs            Pools, datasets, subvolumes, volumes
│   └── inspect.rs          The inspectors and `find_binary`
│
├── telemetry/              Sampled host metrics
│   ├── model.rs            Samples, counters, derived rates
│   ├── collect.rs          Reads `/proc` and `/sys`
│   └── sampler.rs          The sampling loop
│
├── runtime/                Container runtimes, init systems and mutations
│   ├── model.rs            The inspection report structures
│   ├── container.rs        The `ContainerClient` trait, the registry and discovery
│   ├── container/
│   │   ├── bollard.rs      Docker API client (rustls, Unix socket)
│   │   ├── cli.rs          Command line fallback client
│   │   └── mock.rs         In-memory client used by the tests
│   ├── inspect.rs          Structured runtime inspection
│   ├── provider.rs         Provider integration (systemd, OpenRC, docker, podman)
│   ├── mutation.rs         Backends, plans, receipts, verification, rollback
│   └── controller.rs       The gated execution controller
│
├── catalog/                The application catalog
│   ├── model.rs            The index and entry structures
│   ├── index.rs            Built-in index, loading, merging, validation
│   └── install.rs           Entry → container spec → mutation plan
│
├── provision/              Declarative provisioning manifests
│   ├── model.rs            The manifest structures
│   └── plan.rs             Parsing, validation and the plan
│
├── plugins/                Wasm plugin inventory
│   ├── model.rs            The plugin manifest and report
│   └── engine.rs           The Wasmtime scanner
│
├── app/                    Application state and the runtime host
│   ├── state.rs            `AppState` and the mutation helpers
│   ├── event.rs            The event enum
│   └── runtime.rs          `AppRuntime` — bootstraps the state and the clients
│
└── tui/                    The terminal interface
    ├── mod.rs              The event loop, the tab bar, the `ActiveTab` enum
    ├── input.rs            The terminal reader thread
    ├── tabs/               One module per tab
    │   ├── mod.rs          The `Tab` trait and the `TabManager`
    │   ├── overview.rs     Host summary
    │   ├── discovery.rs    What was discovered
    │   ├── storage.rs      Pools, datasets and mounts
    │   ├── runtime.rs      Containers and init services, with mutations
    │   ├── telemetry.rs    Live metrics
    │   └── catalog.rs      The catalog, with installs
    └── widgets/            Reusable widgets
        ├── list.rs         Selectable list with filter and sort
        ├── detail.rs       Structured detail pane
        ├── dialog.rs       Action menu and confirmation dialog
        ├── progress.rs     Progress overlay
        └── cache.rs        What a tab has already built
```

---

## 4. The host boundary

Four traits separate Anvil from the machine it runs on.

### `TextReader` and `CommandRunner` (`src/platform/traits.rs`)

`TextReader` reads files and lists directories; `CommandRunner` runs a program
and captures its output. Every discovery, storage and telemetry collector takes
one of these, so a test can feed it fixtures instead of touching the host.

`OsTextReader` and `OsCommandRunner` are the real implementations.

### `ContainerClient` (`src/runtime/container.rs`)

```rust
pub trait ContainerClient: Send + Sync {
    fn runtime_name(&self) -> &str;
    fn is_reachable(&self) -> bool;
    fn daemon_status(&self) -> Option<String>;
    fn socket_path(&self) -> Option<&str>;
    fn binary_path(&self) -> Option<&str>;

    fn list_containers(&self) -> Result<Vec<ContainerSummary>>;
    fn inspect(&self, name: &str) -> Result<ContainerDetails>;
    fn start(&self, name: &str) -> Result<()>;
    fn stop(&self, name: &str) -> Result<()>;
    fn restart(&self, name: &str) -> Result<()>;
    fn remove(&self, name: &str) -> Result<()>;
    fn pull(&self, image: &str) -> Result<()>;
    fn create(&self, request: &ContainerCreateRequest) -> Result<String>;
}
```

The trait is deliberately synchronous: the rest of the integration layer is
synchronous, and a synchronous trait keeps every daemon call on the same code
path.

There are three implementations.

| Implementation | Transport | When it is used |
|---------------|-----------|-----------------|
| `BollardContainerClient` | Docker API over a Unix socket | The runtime exposes a socket |
| `CliContainerClient` | `docker` / `podman` command line | The runtime has no socket (e.g. rootless Podman without a service) |
| `MockContainerClient` | In-memory | The tests |

`discover_clients()` builds the registry once at start-up: for each runtime it
looks for a socket, and falls back to the command line client when the runtime
binary is installed but no socket is available. The registry is then reused,
so the daemon is not re-probed for its version on every refresh.

The Docker API client is asynchronous underneath. It owns a small tokio runtime
on a dedicated worker thread and hands requests to it, which is what lets the
synchronous trait stay honest — every call really goes to the daemon.

### `PluginBackend` (`src/plugins/engine.rs`)

The plugin scanner takes a `TextReader` and a list of directories and returns a
`PluginCatalog`. `WasmPluginEngine` is the real implementation.

---

## 5. Reading the host

### Discovery (`src/discovery/`)

`discovery::all()` assembles a `DiscoveryReport` from the storage, network and
runtime collectors. Everything is read-only and every probe that fails
produces a `Diagnostic` instead of an exception, so one broken subsystem does
not hide the rest of the host.

### Storage inspection (`src/storage/`)

Reads ZFS pools and datasets, Btrfs subvolumes, LVM volumes and the mount
table. It reports what exists; it does not create pools or datasets. There is
no storage mutation path.

### Runtime inspection (`src/runtime/inspect.rs`)

`inspect_runtime_with_clients()` walks the client registry and asks each
runtime for its container list. The containers are structured data, not parsed
command output. The init system is still detected through the command line,
because `systemd` and OpenRC have no API worth speaking here.

The init system is read for real, not counted: `systemctl list-units` and
`rc-status --all` are parsed into a list of services with the state the init
system itself reports, so the runtime tab can list them and the receipt can
name the unit that was touched.

The report distinguishes three states: the daemon answered, the daemon is not
reachable, or the runtime is not installed at all.

### Telemetry (`src/telemetry/`)

Samples CPU counters, memory, disk and network counters from `/proc` and
`/sys`, and derives rates between samples. The telemetry tab shows these
numbers; it does not list processes, because Anvil does not collect process
data and would have to invent it.

---

## 6. Changing the host

All mutations go through one path.

```
MutationTarget  +  MutationBackend  +  MutationVerb
                        │
                        ▼
                  MutationPlan          (what would happen, and how)
                        │
                        ▼
              MutationGateway           (privilege + mode gate)
                        │
                        ▼
        MutationController::execute()   (runs it, verifies it, plans a rollback)
                        │
                        ▼
                 ControllerReceipt       (what happened, and by which mechanism)
```

### The privilege gate

`MutationGateway` refuses to plan a mutation unless the process is root. It
also carries the mode: `DryRun` or `Execute`. `Execute` requires both root and
`ANVIL_ALLOW_MUTATION=1`; the environment variable is deliberately not exposed
as a CLI flag, so a shell history or a script cannot silently enable it.

`--read-only` forces the read-only tier even for root.

### The plan

A `MutationPlan` knows the command line that would do the same thing and, for
container work, the API call that will actually be made:

```rust
pub enum ContainerApiAction {
    Start  { name: String },
    Stop   { name: String },
    Restart{ name: String },
    Remove { name: String },
    Create { spec: ContainerSpec },
}
```

`plan.describe()` returns the API call when there is one, so the confirmation
dialog shows what will really happen rather than a command line that will not
be run.

### The receipt

```rust
pub struct MutationExecutionReceipt {
    pub executed: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub mechanism: ExecutionMechanism,   // Command or ContainerApi
}
```

The mechanism is what makes the receipt trustworthy. It records whether the
change was carried out by the daemon API or by a command line.

### Verification and rollback

After a successful execution the controller verifies the result — by asking
the daemon whether the container is really running, or by asking `systemctl`
whether the unit is active — and builds a rollback plan for the inverse verb.
The verification result is part of the receipt, so a mutation that appeared to
succeed but did not take effect is visible.

---

## 7. The catalog

The catalog is a data file, not code. `src/catalog/index.rs` holds a built-in
index; a user file in `/etc/anvil/catalog.toml` or
`$XDG_CONFIG_HOME/anvil/catalog.toml` is merged on top of it, replacing
entries with the same id and adding new ones.

An entry becomes a `ContainerSpec`, the spec becomes a `MutationTarget`, and
the backend produces the same `MutationPlan` as every other mutation. The
privilege gate, the receipt, the verification and the rollback therefore apply
to a catalog install exactly as they do to a container restart.

`anvil catalog install --dry-run` plans without touching the host and without
needing root. Without `--dry-run` it goes through the controller, so it needs
root and `ANVIL_ALLOW_MUTATION=1`.

The TUI's catalog tab reads the same index. The installed state of an entry
is derived from the runtime inspection — a container whose name matches the
entry id — and is rebuilt on every refresh, so it always reflects the host.

See [`catalog.md`](catalog.md) for the index format.

---

## 8. The terminal interface

The TUI is built on a `TabManager` that owns an ordered list of tabs and tracks
the active one through the `ActiveTab` enum. That enum is the single source of
truth for tab identity: the tab bar and the rendered view cannot disagree.

| Tab | Shows |
|-----|-------|
| Overview | Host summary: system, discovery, storage, runtime, telemetry and recent mutations |
| Discovery | What the discovery pass found, with the diagnostics |
| Storage | Pools, datasets, subvolumes, volumes and mounts |
| Runtime | Containers and init services, with start/stop/restart/remove actions |
| Telemetry | Live CPU, memory, disk and network counters |
| Catalog | The catalog, with install and uninstall actions |

Each tab implements the `Tab` trait, which optionally exposes an action menu, a
confirmation dialog and a progress overlay. The manager gives those overlays
priority over the tab's own key handling, so a modal dialog cannot be
dismissed by a key the tab would otherwise consume.

The widgets (`src/tui/widgets/`) are a general-purpose toolkit: a filterable
and sortable list, a structured detail pane, an action menu, a confirmation
dialog and a progress overlay. They are not catalog-specific, and an entry
point that no tab uses does not stay in the tree: the compiler says so, and
the build fails on the warning.

### The event loop

The loop in `src/tui/mod.rs` waits on three things at once: the terminal, the
state events, and the timers.

The terminal is read on a thread of its own (`src/tui/input.rs`), which pushes
the events into a channel. A keystroke is therefore handled the moment it
arrives, rather than at the next timer tick, and a held-down key scrolls as
fast as the terminal repeats it.

The screen is redrawn when something changed, not on a timer. The telemetry
task and the host-refresh task announce their work through `AppEvent::StateChanged`,
and the loop draws a frame when that event arrives or when a key was handled.

### What a frame builds

A redraw used to rebuild everything the tab shows: the runtime tab turned its
services into rows again, and the detail pane was formatted from scratch. That
work does not belong between a keypress and the screen.

`AppState` carries a generation counter for each source it holds
(`host_generation`, `telemetry_generation`, `mutation_generation`), and each
tab keeps a `BuildCache` (`src/tui/widgets/cache.rs`) that remembers what it
already built. A frame that changes nothing builds nothing: the rows are built
again when a generation moved, and the detail pane when the selected item
changed. The key is the identity of the selected item, not its row, because a
filter or a sort can put a different item under the same cursor.

---

## 9. The terminal interface's counterpart: the CLI

```
anvil                          # TUI
anvil --read-only              # TUI, forced read-only
anvil discover [--json]
anvil storage [--json]
anvil runtime [--json]
anvil plugins [--json]
anvil apply --manifest FILE --dry-run [--json]
anvil catalog list [--json] [--category NAME]
anvil catalog show ID [--json]
anvil catalog install ID [--dry-run] [--json]
anvil catalog update --from-file FILE
```

The inspection commands print a report or the same report as JSON. `apply`
requires `--dry-run` and refuses to run without it.

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | The command succeeded |
| 1 | A runtime failure: an I/O error, a command that failed, an error from the host |
| 2 | A configuration error: a missing or broken config file, a manifest with the wrong version |
| 3 | A permission error: the session is not allowed to make the change it was asked to make |

The code says what has to change before the command can work again: a
configuration problem needs a different file, a permission problem needs a
different session, and a runtime failure needs the host to behave differently.

---

## 10. Plugins

Anvil scans plugin directories for `*.toml` manifests, loads the `.wasm` file
each manifest points at, and reports what it found. The scan is sandboxed by
Wasmtime, the manifest declares an `api_version` that must match the host's,
and a capability allow-list rejects a manifest that asks for something the
host does not offer.

**The plugin host does not expose host functions yet.** A plugin module is
instantiated with an empty import list and a fuel budget, and its entry point
is called with no arguments. Anything that needs host data — a disk usage
figure, a container list — is not available to a plugin today. See
[`plugins.md`](plugins.md).

---

## 11. Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` | Terminal layout and rendering |
| `crossterm` | Terminal mode, raw input, events |
| `tokio` | The async runtime behind the event loop, the timers and the refresh task |
| `bollard` | The Docker API client (rustls, no native-tls, so the musl targets stay static) |
| `futures-util` | Driving the image-pull stream |
| `serde` / `serde_json` / `toml` | Config, manifests, the catalog index and the JSON reports |
| `clap` | The command tree |
| `wasmtime` | The sandboxed plugin runtime |
| `thiserror` / `anyhow` | Error types and context |
| `tracing` / `tracing-subscriber` | File-only logging |

There is no `sysinfo` dependency: the telemetry collectors read `/proc` and
`/sys` directly.

---

## 12. What is not built yet

Stated plainly, so the documentation does not overpromise:

- **No storage mutations.** ZFS pools, datasets and snapshots are read, never
  written.
- **No daemon mode.** `anvil daemon` does not exist; the TUI is the only
  interactive mode.
- **No remote catalog.** The index is a local file. Fetching one over the
  network needs a signed-index format and a TLS client, and neither is built.
- **No plugin host API.** Plugins are sandboxed but cannot ask the host for
  anything.
- **No drive management or process table.** The telemetry tab deliberately
  shows counters instead of inventing a process list.
- **No Compose support.** The catalog installs single containers.
