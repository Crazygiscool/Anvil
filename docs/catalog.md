# The application catalog

Anvil ships a small catalog of applications you can install on a homelab host.
The catalog is **data**, not code: it is a TOML index file, and you can add
entries or replace the built-in ones without rebuilding Anvil.

An install is a real mutation. It goes through the same controller as every
other change Anvil makes, so the privilege gate, the receipt, the verification
and the rollback plan all apply.

---

## 1. Using the catalog

### List the entries

```sh
anvil catalog list
anvil catalog list --json
anvil catalog list --category database
```

The categories in the list are the categories that are actually in the index —
they are derived from the file, not from a hard-coded list.

### Show one entry

```sh
anvil catalog show nginx
anvil catalog show nginx --json
```

### Plan an install

```sh
anvil catalog install nginx --dry-run
```

This prints the action and the equivalent command line, and changes nothing:

```
catalog install id=nginx
  action: docker API: create and start container 'nginx' from nginx:alpine
  command: docker run -d --name nginx --restart unless-stopped -p 8080:80/tcp nginx:alpine
  result: planned (dry-run), nothing was changed
```

`--dry-run` needs no root and no environment variable. It only plans.

### Install it

```sh
sudo ANVIL_ALLOW_MUTATION=1 anvil catalog install nginx
```

Without `--dry-run` the install is a real mutation. It requires **both**:

- the process runs as **root**, and
- `ANVIL_ALLOW_MUTATION=1` is set in the environment.

Without root the command fails with a permission error and nothing is changed.
As root but without the environment variable the command succeeds but only
*plans* — it reports `result: planned (dry-run), nothing was changed` and the
host is untouched.

The environment variable is intentionally not exposed as a CLI flag, so it
cannot be switched on by accident in a script or a shell history.

When the runtime exposes a socket, the install is carried out through the
Docker API. When it does not, the equivalent command line is run instead. The
receipt says which one happened:

```json
{
  "execution": {
    "executed": true,
    "mechanism": "container_api"
  }
}
```

### Replace the local index

```sh
anvil catalog update --from-file ./my-catalog.toml
```

The file is parsed and validated first. An invalid file is rejected and the
existing index is left alone — `update` never writes a broken index.

The validated file is written to the default index path (`/etc/anvil/catalog.toml`
as root, `$XDG_CONFIG_HOME/anvil/catalog.toml` otherwise). At load time that
file is merged on top of the built-in index, so you only need to state the
entries you want to add or change.

---

## 2. Where the index lives

| User | Path |
|------|------|
| root | `/etc/anvil/catalog.toml` |
| non-root | `$XDG_CONFIG_HOME/anvil/catalog.toml`, then `~/.config/anvil/catalog.toml`, then `/etc/anvil/catalog.toml` |

The first file found is loaded and merged **on top of** the built-in index:

- an entry with a new id is added,
- an entry with a built-in id **replaces** the built-in one,
- every other built-in entry is kept.

So a user file only has to state the entries it cares about.

---

## 3. The index format

```toml
version = 1

[[apps]]
id = "nginx"
name = "nginx"
description = "HTTP server and reverse proxy"
category = "networking"
image = "nginx:alpine"
runtime = "docker"
restart_policy = "unless-stopped"

[[apps.ports]]
host = 8080
container = 80
protocol = "tcp"

[[apps.volumes]]
source = "nginx_data"
target = "/var/lib/nginx"
kind = "volume"

[[apps]]
id = "postgres"
name = "PostgreSQL"
description = "Relational database"
category = "database"
image = "postgres:17-alpine"
restart_policy = "unless-stopped"
env = { POSTGRES_PASSWORD = "change-me" }

[[apps.ports]]
host = 5432
container = 5432
protocol = "tcp"

[[apps.volumes]]
source = "postgres_data"
target = "/var/lib/postgresql/data"
kind = "volume"
```

> **TOML ordering matters.** In TOML, a key like `env = { ... }` belongs to the
> `[[apps]]` table it appears under. Once you open a sub-table such as
> `[[apps.ports]]`, every following key belongs to that sub-table. Put
> `env = { ... }` **before** the first `[[apps.ports]]` or `[[apps.volumes]]`
> of an entry, or it will be parsed as a port or a volume field and the index
> will be rejected.

### Top level

| Field | Required | Meaning |
|-------|----------|---------|
| `version` | yes | The index format version. Must be `1`. |
| `apps` | no | The list of entries. Defaults to empty. |

### An entry (`[[apps]]`)

| Field | Required | Default | Meaning |
|-------|----------|---------|---------|
| `id` | yes | — | The identifier. Also the container name. |
| `name` | yes | — | The display name. |
| `description` | no | `""` | One line about the app. |
| `category` | yes | — | The grouping used by the filter. |
| `image` | yes | — | The container image reference. |
| `runtime` | no | `"docker"` | `docker` or `podman`. |
| `restart_policy` | no | `"unless-stopped"` | The restart policy. |
| `ports` | no | `[]` | Published ports. |
| `volumes` | no | `[]` | Volume mounts. |
| `env` | no | `{}` | Environment variables. |

### A port (`[[apps.ports]]`)

| Field | Required | Default | Meaning |
|-------|----------|---------|---------|
| `host` | yes | — | The host port. Must be greater than zero. |
| `container` | yes | — | The container port. Must be greater than zero. |
| `protocol` | no | `"tcp"` | `tcp`, `udp` or `sctp`. |

### A volume (`[[apps.volumes]]`)

| Field | Required | Default | Meaning |
|-------|----------|---------|---------|
| `source` | yes | — | A named volume, or an absolute host path for a bind. |
| `target` | yes | — | The absolute path inside the container. |
| `kind` | no | `"volume"` | `volume` (named) or `bind` (host path). |

---

## 4. Validation

An index is validated when it is parsed and again after it is merged, so a
broken overlay can never reach the interface. The rules are deliberately strict
— the index is data that will end up as a container definition.

**Identifiers.** `id` and `category` must be lowercase ASCII letters and
digits separated by single dashes. They may not start or end with a dash.

**Names.** `name` must not be empty.

**Images.** An image reference may not contain whitespace, control characters
or shell metacharacters (`;&|$` and friends), and may not start with a dash.

**Runtimes.** `runtime` must be `docker` or `podman`.

**Restart policies.** One of `no`, `always`, `unless-stopped`, `on-failure`,
`unless-stopped:5` or `on-failure:5`.

**Ports.** Both port numbers must be greater than zero; the protocol must be
`tcp`, `udp` or `sctp`.

**Volumes.** The target must be an absolute path. A `volume` source may not
contain `/` or `:` and must be lowercase alphanumeric with dashes or
underscores. A `bind` source must be an absolute path and may not be one of
the protected system paths:

```
/  /bin  /boot  /dev  /etc  /lib  /lib64  /proc  /root
/sbin  /sys  /usr  /var/lib/docker  /var/lib/containers
```

**Environment variables.** The name must be a valid identifier (letters,
digits and underscores, not starting with a digit). The value may not contain
a null byte.

**Duplicates.** Two entries with the same `id` are an error.

---

## 5. The built-in entries

| id | Category | Image |
|----|----------|-------|
| `portainer` | management | `portainer/portainer-ce:latest` |
| `nginx` | networking | `nginx:alpine` |
| `postgres` | database | `postgres:17-alpine` |
| `redis` | database | `redis:7-alpine` |
| `prometheus` | monitoring | `prom/prometheus:latest` |
| `gitea` | devops | `gitea/gitea:latest` |

The built-in entries are a starting point, not a recommendation for every
host. In particular `postgres` ships with `POSTGRES_PASSWORD = "change-me"`,
which you should override in your own index before installing it anywhere
that matters.

---

## 6. The catalog in the TUI

The **Catalog** tab reads the same index the CLI reads.

- The list shows the entries of the merged index, with a category filter
  derived from the index itself.
- An entry is marked as installed when a container with the entry's `id`
  exists. The state is derived from the runtime inspection on every refresh,
  so it always reflects the host — it is not a flag the tab flips for itself.
- **Install** and **Uninstall** go through the mutation controller. The
  confirmation dialog shows the exact API call or command, and says whether
  this session will really change the host or only plan the change.
- A broken index file is reported in the tab instead of being hidden behind an
  empty list.

---

## 7. What the catalog does not do

- **No remote index.** The index is a local file. Fetching one over the
  network needs a signed-index format and a TLS client, and neither exists
  yet.
- **No Compose.** An entry describes one container. There is no support for a
  Compose file, and no support for a multi-container application.
- **No image building.** The image must already exist in a registry.
