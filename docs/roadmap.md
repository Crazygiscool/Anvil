# Anvil roadmap

This is where Anvil is and where it is going. It is written against what the
code does today, not against a wish list.

---

## 1. What Anvil is

A single static binary for Linux that reads the state of a homelab host and
lets you change it, deliberately:

- **Discovery** of storage, network, init system and container runtimes
- **Telemetry** sampled from `/proc` and `/sys`
- **Storage inspection** of ZFS, Btrfs, LVM and mounts
- **Runtime management** of containers and init services
- **An application catalog** that installs real containers
- **A plugin inventory** that scans sandboxed Wasm modules
- **A declarative provisioning plan** that is always a dry run

Everything is read-only unless you explicitly ask for a mutation, and a
mutation needs root *and* `ANVIL_ALLOW_MUTATION=1`.

See [`architecture.md`](architecture.md) for how it is put together.

---

## 2. Status

| Area | Status |
|------|--------|
| Host discovery | Done |
| Telemetry sampling | Done |
| Storage inspection | Done (read-only) |
| Runtime inspection | Done — containers, the init system and its services |
| Container lifecycle mutations | Done, through the Docker API or the command line |
| Init service mutations | Done — the services are listed with their real state, and start/stop/restart go through `systemctl` or `rc-service` |
| Application catalog | Done — index, validation, CLI and TUI tab |
| Wasm plugin inventory | Done — sandboxed scan and report |
| Provisioning manifests | Done — parsing, validation and a dry-run plan |
| TUI | Done — six tabs with real data and real actions |
| Static musl release builds | Done for both targets |

---

## 3. What is deliberately not built

These are known gaps, not oversights. Each one is listed with what it would
take.

### Storage mutations

Storage is read, never written. There is no `zpool create`, no `zfs dataset`,
no snapshot, no LVM operation.

*To build it:* a set of storage backends behind a trait like the container
one, a dry-run planner for each destructive operation, and a verification step
that reads the pool back. The privilege gate already exists and would apply
unchanged.

### A daemon mode

There is no `anvil daemon`. The TUI is the only interactive mode; the CLI
commands are one-shot reports.

*To build it:* the mutation path is now proven end to end, so this is the
natural next milestone. It would need a supervisor-friendly lifecycle, a
socket or API for the TUI to talk to, and the audit ledger exposed over that
API. The `AppState` and `MutationController` are already separated from the
TUI, so most of the work is in the transport.

### A remote catalog index

The catalog is a local file. Anvil cannot fetch an index over the network.

*To build it:* a signed-index format (so a fetched index can be verified), a
TLS client, and a trust model for the signing key. The `bollard` dependency
already pulls in `rustls`, so the TLS client is close at hand — but the
signature format has to be designed first, and it is the design, not the
code, that is the hard part.

### A plugin host API

Plugins are sandboxed and reported, but they cannot ask Anvil for anything.
The `capabilities` allow-list exists and is enforced, but nothing is granted
because there is nothing to grant yet.

*To build it:* a small, read-only, versioned host API — most likely a set of
JSON payloads passed through Wasmtime's `linker` — with the capability check
driving which functions are linked into a given module. The `api_version`
check already exists and is what keeps an old plugin from running against a
newer host.

### Compose support

An entry describes one container. There is no Compose file support.

*To build it:* a second kind of catalog entry, or a separate manifest format,
that describes a set of containers and their dependencies. The install planner
would need to turn a set of containers into a set of plans with an ordering,
and the rollback story becomes harder: removing three containers is not one
inverse operation.

### Drive management and a process table

The telemetry tab shows counters. It does not list drives or processes.

*To build it:* the drive information is mostly available through the existing
storage inspection, but a process table needs a `/proc` walk that Anvil does
not currently do. Both are read-only additions, and both were deliberately
left out rather than approximated — the interface does not show a process list
it cannot actually collect.

---

## 4. Principles for new work

**A read is a read.** New data has to come from the host, not from a model of
what the host probably looks like. If a value cannot be collected, the
interface shows that it is unknown rather than a plausible number.

**A mutation is a plan, a confirmation, an execution, a verification and a
receipt.** The existing path in `src/runtime/mutation.rs` and
`src/runtime/controller.rs` is the path. New verbs plug into it rather than
around it.

**The receipt tells the truth.** If a change was carried out through the
Docker API, the receipt says `container_api`. If it was carried out through a
command line, it says `command`. If it was only planned, `executed` is
`false`.

**Tests do not need a host.** Every integration behaviour is behind a trait,
and the traits have mock implementations. A change that cannot be tested
without root or a daemon does not belong in this codebase.

**The release stays static.** Both musl targets must build and link without a
dynamic dependency before anything is published. `bollard` is pinned with
`rustls` for exactly this reason.

---

## 5. Contributing

The verification gate before every commit:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

And before a release, both release targets:

```sh
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
file target/*/release/anvil
```

The CI workflow runs the same gate, builds both targets with the musl cross
toolchain, checks that the binaries are statically linked, and runs the
container API test against a real daemon.
