# Changelog

Every release, in order. The version is the tag; the commits are the ones
the release was cut from.

## Unreleased

The interface stopped lagging: a keystroke is handled when it happens, and a
frame only builds what changed.

### Fixed

- The terminal is read on a thread of its own, so a keystroke is handled the
  moment it arrives. The event loop used to poll for input once per refresh
  tick and read a single event per tick, which put a whole second in front
  of every key with the default configuration.
- A held-down key scrolls as fast as the terminal repeats it, instead of one
  row per refresh tick.

### Changed

- The screen is redrawn when something changed — a handled key, a new
  telemetry sample, a refreshed host report — rather than on every pass
  through the event loop.
- Each tab rebuilds only what changed. The rows are built again when the
  host state moved, and the detail pane when the selected item changed; a
  frame that changes nothing builds nothing. The runtime tab went from
  ~150 µs per frame to ~260 ns.
- The detail pane follows the selected item by identity rather than by row,
  so a filter or a sort that puts another item under the cursor shows that
  item.

## 0.2.1

The init services are real, an install pulls the image it needs, the exit
code says what went wrong, and the dead code is gone.

### Added

- The init services are read for real. `systemctl list-units` and
  `rc-status --all` are parsed into the services the init system reports,
  with the state it reports.
- The runtime tab lists the services, shows their state in the detail pane,
  and offers start, stop and restart.
- `anvil runtime` prints the services, so the command-line report says the
  same thing the tab shows.
- The CLI exits with a code that says what has to change: `2` for a
  configuration problem, `3` for a permission problem, `1` for a runtime
  failure.

### Fixed

- A catalog install pulls the image before it creates the container. The
  Docker API does not fetch an image when a container is created — it
  answers "no such image" — so an install on a host that did not already
  have the image failed.
- A configuration file the user named explicitly and that is not there is
  reported as a configuration problem rather than an I/O problem.

### Changed

- The tree carries no `#[allow(dead_code)]` any more. Five blanket
  suppressions were hiding code that nothing called, including the
  `ContainerClient::pull` that the bug above lived in.

## 0.2.0

The first release of the rewritten Anvil: a real terminal interface, a real
application catalog, a real container API, and a documented safety model.

### Added

- Six tabs on a tab manager, with a reusable widget toolkit: a filterable
  and sortable list, a structured detail pane, an action menu, a
  confirmation dialog and a progress overlay.
- Mutations wired to the host: container and service lifecycle actions go
  through one controller — plan, confirm, execute, verify, record a receipt
  with a rollback plan.
- An application catalog: a versioned TOML index with validation, an
  `anvil catalog` CLI and a catalog tab.
- A container API through `bollard` over the daemon's Unix socket, with the
  command line kept as the fallback.
- A Wasm plugin inventory that scans sandboxed modules.
- `docs/architecture.md`, `docs/roadmap.md`, `docs/catalog.md` and
  `docs/plugins.md`.

### Notes

- The tag was moved twice before the release settled: the first cut carried
  a race in the container client, and the checksum files named the path
  they had in the build directory instead of the asset's own name. The
  published assets are the ones from the fixed tree.
