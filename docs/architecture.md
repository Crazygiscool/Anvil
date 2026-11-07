# Anvil Architecture (`ARCHITECTURE.md`)

Anvil is a zero-overhead, post-install homelab management and provisioning engine written in Rust. It functions as a lightweight terminal dashboard and post-install bootstrap daemon that automatically integrates with Linux system primitives, storage pools, container runtimes, and system services.

---

## 1. System Overview & Core Philosophy

Anvil operates on three core design principles:
* **Zero-Dependency Portability:** Compiles to a single static binary targeting `x86_64` and `aarch64` architectures.
* **Auto-Discovery & Passive Integration:** Discovers storage pools (ZFS/Btrfs/LVM), network interfaces, init systems (`systemd`/`OpenRC`), and container sockets (`/var/run/docker.sock`) without requiring mandatory initial setup.
* **Dual-Mode Execution:** Operates seamlessly in interactive TUI mode (driven by `ratatui` and `crossterm`) and headless daemon/provisioning mode.


```
              ┌────────────────────────────────────────┐
              │              Anvil TUI                 │
              │   (Ratatui Terminal Dashboard)         │
              └───────────────────┬────────────────────┘
                                  │ Event Loop / Keybinds
                                  ▼
              ┌────────────────────────────────────────┐
              │            Anvil Core Engine           │
              │       (State Manager & Bus)            │
              └──────┬────────────┬────────────┬───────┘
                     │            │            │
     ┌───────────────┘            │            └───────────────┐
     ▼                            ▼                            ▼
┌─────────────────┐          ┌──────────────────┐         ┌────────────────┐
│ System Telemetry│          │ Container & Init │         │ Storage & Pool │
│    Collector    │          │    Integrator    │         │  Orchestrator  │
└────────┬────────┘          └────────┬─────────┘         └───────┬────────┘
         │                            │                           │
         ▼                            ▼                           ▼
`/proc` & `/sys`            Docker / Podman Socket         ZFS / Btrfs / LVM
Network Interfaces           `systemd` / `OpenRC`          Mount Management

```

---

## 2. Directory & Module Layout(just a sample, you are to guide throught the actual repo structure to your own needs)

The codebase is organized into isolated, single-responsibility modules:

```text
anvil/
├── Cargo.toml
├── ARCHITECTURE.md
├── src/
│   ├── main.rs                  # Entry point, CLI args parsing, mode dispatch
│   ├── app.rs                   # Central application state & state machine
│   ├── config/
│   │   ├── mod.rs               # Configuration parser & defaults initializer
│   │   └── manifest.rs          # TOML manifest structures (`d.conf/*.toml`)
│   ├── discovery/
│   │   ├── mod.rs               # Auto-detection coordinator
│   │   ├── storage.rs           # ZFS, Btrfs, LVM, and filesystem scanner
│   │   ├── network.rs           # Interface enumerator (`sysfs`, `/proc/net/dev`)
│   │   └── runtime.rs           # Container socket and init system detector
│   ├── telemetry/
│   │   ├── mod.rs               # Metrics collector pipeline
│   │   ├── cpu.rs               # CPU utilization & load averages
│   │   ├── memory.rs            # RAM & Swap allocation statistics
│   │   └── io.rs                # Disk I/O & throughput metrics
│   ├── storage/
│   │   ├── mod.rs               # Storage management interface
│   │   ├── zfs.rs               # ZFS pool operations and dataset controls
│   │   └── btrfs.rs             # Btrfs subvolume & snapshot manager
│   ├── runtime/
│   │   ├── mod.rs               # Container management abstraction
│   │   ├── docker.rs            # Docker Engine API client via Unix socket
│   │   └── systemd.rs           # Systemd unit health and controller
│   ├── ui/
│   │   ├── mod.rs               # Main render engine & terminal event loop
│   │   ├── layout.rs            # Flexible grid and tab bar frame setup
│   │   ├── tabs/
│   │   │   ├── mod.rs           # `AnvilTab` trait definition
│   │   │   ├── overview.rs      # System health & metric sparklines view
│   │   │   ├── containers.rs    # Docker/Podman container manager view
│   │   │   ├── storage.rs       # Filesystem and snapshot controller view
│   │   │   └── services.rs      # Endpoint health ping & service status view
│   │   └── widgets/             # Reusable UI component wrappers
│   ├── plugin/
│   │   ├── mod.rs               # Plugin runtime manager
│   │   └── wasm.rs              # Wasmtime sandbox integration layer
│   └── error.rs                 # Global error taxonomy using `thiserror`

```

---

## 3. Core Module Specifications

### 3.1. Discovery Engine (`src/discovery/`)

On initialization, Anvil probes host OS subsystems:

* **Storage Scanner:** Checks for `/dev/zfs`, `/sys/fs/btrfs`, and active `/proc/mounts`.
* **Network Enumerator:** Inspects `/sys/class/net/` to identify physical and wireless interfaces (e.g., `wlp2s0`, `eth0`), filtering out virtual bridges and loopbacks by default.
* **Runtime Prober:** Checks for available control sockets (`/var/run/docker.sock`, `/run/podman/podman.sock`) and tests DBus connections for `systemd`.

### 3.2. State & Event Loop (`src/app.rs` & `src/ui/`)

Anvil uses an asynchronous event-driven loop backed by `tokio` and `crossterm`:

1. **Input Handling Task:** Listens for terminal input events (key presses, resizes) and pushes action signals to the core event channel.
2. **Telemetry Update Loop:** Collects host hardware metrics at configurable intervals (default: 1000ms) without blocking the UI thread.
3. **Render Task:** Executes `ratatui::Terminal::draw()` on state change or tick event.

### 3.3. Extensibility & Plugin Architecture (`src/ui/tabs/` & `src/plugin/`)

#### Compile-Time Extensions (`AnvilTab` Trait)

Native features and new UI sections implement the `AnvilTab` trait:

```rust
pub trait AnvilTab: Send + Sync {
    /// Unique tab key identifier
    fn id(&self) -> &'static str;
    
    /// Display title for the top tab bar
    fn title(&self) -> &str;
    
    /// Poll metrics or underlying runtime state
    fn tick(&mut self, state: &AppState) -> Result<(), AnvilError>;
    
    /// Process keyboard events when tab is active
    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Option<Action>;
    
    /// Render tab view frame onto the terminal canvas
    fn render(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect);
}

```

#### Dynamic Runtime Plugins (Wasm Engine)

External monitoring modules are compiled into WebAssembly modules (`.wasm`) and loaded via `wasmtime`. Plugins communicate with Anvil through a restricted Host API exposed via standard JSON ABI payloads.

---

## 4. Storage & Provisioning Engine

Anvil handles post-install system bootstrapping through declarative TOML manifests stored in `/etc/anvil/d.conf/*.toml`.

```
               ┌───────────────────────────┐
               │   `/etc/anvil/d.conf/`    │
               │   Declarative Manifests   │
               └─────────────┬─────────────┘
                             │
                             ▼
               ┌───────────────────────────┐
               │    Anvil Provisioner      │
               └─────────────┬─────────────┘
                             │
            ┌────────────────┼────────────────┐
            ▼                ▼                ▼
    ┌───────────────┐┌───────────────┐┌───────────────┐
    │ Storage Pools ││ Systemd Units ││  Containers   │
    │ (ZFS / Btrfs) ││ Registration  ││ Auto-spinup   │
    └───────────────┘└───────────────┘└───────────────┘

```

---

## 5. Key Cargo Dependencies

| Crate | Purpose |
| --- | --- |
| `ratatui` | Terminal user interface layout and rendering framework |
| `crossterm` | Terminal manipulation, raw mode control, and event listening |
| `tokio` | Asynchronous runtime for async I/O, ticks, and process executions |
| `sysinfo` | Cross-platform host hardware statistics collection |
| `bollard` | Pure-Rust Docker Daemon API client via Unix sockets |
| `serde` / `toml` | Serialization/deserialization of configuration files and manifests |
| `thiserror` / `anyhow` | Ergonomic error handling and context propagation |
| `wasmtime` | Embedded WebAssembly runtime for dynamic Wasm plugins |

```

---
