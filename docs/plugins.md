# Plugins

Anvil can scan a directory of WebAssembly modules and report what it finds.
This is the **plugin inventory**: it loads each module in a Wasmtime sandbox,
records its exports, and calls its entry point.

> **The plugin host does not expose host functions yet.** A plugin module runs
> in the sandbox, but it cannot ask Anvil for anything — no disk usage, no
> container list, no configuration. The section
> [The host API](#5-the-host-api) below says exactly what exists today and
> what does not.

---

## 1. Using the plugin inventory

```sh
anvil plugins
anvil plugins --json
```

```
api_version=1 plugins=1 diagnostics=0
plugin name=demo version=0.1.0 api_version=1 invoked=true entry_result=0 exports=anvil_entry
```

The inventory is read-only. It never writes to the plugin directories and it
never loads a module outside them.

---

## 2. Where plugins live

| User | Directories, in order |
|------|-----------------------|
| root | `/etc/anvil/plugins` |
| non-root | `$XDG_CONFIG_HOME/anvil/plugins`, `~/.config/anvil/plugins`, `/etc/anvil/plugins` |

Each directory is scanned for `*.toml` manifest files. The manifests are
processed in sorted order, so the report is stable between runs.

---

## 3. The manifest format

Each plugin lives in its own directory, with a manifest and a `.wasm` file:

```
/etc/anvil/plugins/
└── demo/
    ├── plugin.toml
    └── demo.wasm
```

`plugin.toml`:

```toml
name = "demo"
version = "0.1.0"
api_version = 1
library = "demo.wasm"
capabilities = ["telemetry", "read-only"]
entrypoint = "anvil_entry"
```

| Field | Required | Default | Meaning |
|-------|----------|---------|---------|
| `name` | yes | — | The display name of the plugin. |
| `version` | no | `""` | The plugin's own version string. |
| `api_version` | no | `0` | The host API version the plugin was built against. |
| `library` | yes | — | The `.wasm` file, relative to the manifest. |
| `capabilities` | no | `[]` | The capabilities the plugin asks for. |
| `entrypoint` | no | `"anvil_entry"` | The exported function that is called. |

### The `api_version`

A plugin declares the host API version it was written against. The host
provides `PLUGIN_API_VERSION = 1`. A manifest with a different `api_version`
is rejected with a diagnostic:

```
plugin requires API 2, but this host provides 1
```

The version is checked *before* the module is compiled or run, so an
incompatible plugin never gets to execute anything.

### The `library` path

The library path must be **relative** and must not contain `..`. A manifest
that points at an absolute path or tries to climb out of its directory is
rejected:

```
plugin library path must be relative and safe
```

This keeps a plugin from reaching a file outside the plugin directory.

### The `capabilities`

A capability is a request for a permission. The host has an allow-list, and a
manifest that asks for anything outside it is rejected:

```rust
const ALLOWED: &[&str] = &["discovery", "storage", "runtime", "telemetry", "read-only"];
```

A manifest that asks for `network`, for example, is rejected with:

```
plugin capability is not allowed: network
```

Today the capability list is validated but nothing is granted, because the
host exposes no functions to grant it to. The list is the contract that the
host API will be built against.

---

## 4. The entry point contract

The scanner looks for the function named by `entrypoint` (by default
`anvil_entry`) among the module's exports. If it is there, the module is
instantiated and the function is called.

The expected signature is:

```wat
(func (export "anvil_entry") (result i64))
```

The function takes no arguments and returns a single `i64`. The returned
value is reported as `entry_result`.

The scanner records:

| Field | Meaning |
|-------|---------|
| `exports` | Every export the module has, by name |
| `invoked` | Whether the entry point was found **and** called successfully |
| `entry_result` | The `i64` the entry point returned, or nothing |

If the entry point is missing, the plugin is still reported, with a
diagnostic:

```
entrypoint anvil_entry is not exported
```

If the call fails, the failure is reported as a diagnostic as well:

```
entrypoint failed: ...
```

A broken plugin never fails the inventory. It is reported with its
diagnostics, and the other plugins are still scanned.

---

## 5. The host API

**There is no host API yet.**

The module is instantiated with an empty import list:

```rust
wasmtime::Instance::new(&mut store, &module, &[])
```

That means a plugin cannot import anything from Anvil. A module that tries to
import a host function fails to instantiate, and the scanner reports it:

```
instantiate failed: ...
```

### What a plugin can do today

- Compute, using the Wasmtime sandbox.
- Return a value from its entry point, which Anvil reports.
- Be sandboxed against runaway execution: the store is given a fuel budget of
  1,000,000 units, so a module that loops forever runs out of fuel instead of
  hanging the scan.

### What a plugin cannot do today

- Read any host state — no disk usage, no container list, no telemetry.
- Perform any I/O — there is no WASI, no filesystem, no network.
- Change anything. The inventory is read-only by construction.

### What the API will look like

The capabilities in the allow-list — `discovery`, `storage`, `runtime`,
`telemetry`, `read-only` — are the intended shape of the API: a set of
read-only views over the host, granted explicitly per plugin and versioned
through `api_version`.

That API is not implemented. When it lands, a plugin will be able to request
one of those capabilities and receive read-only data in return, and the
`api_version` check above is what keeps an old plugin from running against a
newer host.

---

## 6. The report

`anvil plugins --json` returns the full report:

```json
{
  "api_version": 1,
  "plugins": [
    {
      "name": "demo",
      "version": "0.1.0",
      "api_version": 1,
      "path": "/etc/anvil/plugins/demo/plugin.toml",
      "library": "/etc/anvil/plugins/demo/demo.wasm",
      "exports": ["anvil_entry"],
      "capabilities": ["telemetry", "read-only"],
      "invoked": true,
      "entry_result": 0,
      "diagnostics": []
    }
  ],
  "diagnostics": []
}
```

`diagnostics` at the top level holds the problems found while scanning the
directories themselves (an unreadable directory, for example). The
`diagnostics` inside a plugin hold the problems with that plugin.

---

## 7. Writing a plugin

A minimal plugin needs three things:

1. **A `.wasm` module** that exports `anvil_entry` returning an `i64`.
2. **A manifest** with a matching `api_version` and a relative `library`.
3. **A directory** under one of the plugin directories.

The module has to be built for the `wasm32-unknown-unknown` target, because
Anvil does not provide any imports:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
```

A plugin that imports anything will not instantiate. Until the host API
lands, a plugin is a self-contained computation whose result Anvil reports.

---

## 8. Security notes

- The module is compiled by Wasmtime, which validates the WebAssembly binary
  before running it. A malformed module is rejected with a diagnostic.
- The plugin cannot import host functions, cannot use WASI, and has no file or
  network access.
- The library path is constrained to the plugin's own directory.
- The capability allow-list rejects a manifest that asks for more than the
  host offers.
- The fuel budget bounds the work a module can do.
- The scan never writes anything.

The plugin sandbox is a real boundary, but it is a narrow one today: a plugin
can compute and report, and nothing more.
