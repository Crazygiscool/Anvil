#!/bin/sh
# Native package-manager installer for Anvil's optional Linux container runtimes.
set -eu

runtime=both
execute=false
assume_yes=false
skip_daemon=false
docker_group=false
working_user=

usage() {
    cat <<'EOF'
Usage: install-runtimes.sh [options]

Options:
  --runtime docker|podman|both   Runtime(s) to install. Default: both.
  --execute                      Apply the planned package and service commands.
  --dry-run                      Show commands only. This is the default.
  --yes                          Answer package-manager prompts automatically.
  --skip-daemon                  Do not enable or start container daemons.
  --with-docker-group            Add the target user to the docker group.
  --help                         Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --runtime)
            if [ "$#" -lt 2 ]; then
                echo "Missing argument to --runtime" >&2
                exit 2
            fi
            runtime="$2"
            shift 2
            ;;
        --runtime=*)
            runtime="${1#--runtime=}"
            shift
            ;;
        --execute)
            execute=true
            shift
            ;;
        --dry-run)
            execute=false
            shift
            ;;
        --yes)
            assume_yes=true
            shift
            ;;
        --skip-daemon)
            skip_daemon=true
            shift
            ;;
        --with-docker-group)
            docker_group=true
            shift
            ;;
        --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 2
            ;;
    esac
done

case "$runtime" in
    docker|podman|both) ;;
    *)
        echo "Unsupported runtime selection: $runtime" >&2
        exit 2
        ;;
esac

if [ "$(id -u)" -ne 0 ]; then
    if [ "$execute" = "true" ]; then
        echo "Root privileges are required to execute native package installation." >&2
        exit 1
    fi
fi

os_id=""
os_like=""
if [ -r /etc/os-release ]; then
    . /etc/os-release
    os_id="${ID:-}"
    os_like="${ID_LIKE:-}"
fi

package_manager=""
case "$os_id $os_like" in
    *debian*|*ubuntu*) package_manager="apt-get" ;;
    *fedora*|*rhel*|*centos*) package_manager="dnf" ;;
    *suse*) package_manager="zypper" ;;
    *arch*|*manjaro*) package_manager="pacman" ;;
    *alpine*) package_manager="apk" ;;
esac

if [ "$package_manager" = "" ] || ! command -v "$package_manager" >/dev/null 2>&1; then
    if command -v dnf >/dev/null 2>&1; then
        package_manager="dnf"
    elif command -v yum >/dev/null 2>&1; then
        package_manager="yum"
    elif command -v apt-get >/dev/null 2>&1; then
        package_manager="apt-get"
    elif command -v zypper >/dev/null 2>&1; then
        package_manager="zypper"
    elif command -v pacman >/dev/null 2>&1; then
        package_manager="pacman"
    elif command -v apk >/dev/null 2>&1; then
        package_manager="apk"
    fi
fi

if [ "$package_manager" = "" ] || ! command -v "$package_manager" >/dev/null 2>&1; then
    echo "No supported native package manager was found." >&2
    exit 1
fi

os_string() {
    if [ -n "$os_id" ]; then
        echo "$os_id (like ${os_like:-unknown})"
    else
        echo "unknown"
    fi
}

docker_package() {
    case "$package_manager" in
        apt-get) printf '%s' "docker.io" ;;
        *) printf '%s' "docker" ;;
    esac
}

podman_package() {
    printf '%s' "podman"
}

packages=""
case "$runtime" in
    docker|both) packages="$packages $(docker_package)" ;;
esac
case "$runtime" in
    podman|both) packages="$packages $(podman_package)" ;;
esac
packages=$(printf '%s' "$packages" | sed 's/^ //')

echo "Anvil runtime installation"
echo "OS: $(os_string)"
echo "Package manager: $package_manager"
echo "Runtime: $runtime"
echo "Packages: $packages"
echo "Mode: $([ "$execute" = "true" ] && echo execute || echo dry-run)"

run_command() {
    if [ "$execute" = "true" ]; then
        if [ "$assume_yes" = "true" ]; then
            case "$package_manager" in
                apt-get) apt-get update && apt-get install -y $packages ;;
                dnf|yum) "$package_manager" install -y $packages ;;
                zypper) zypper --non-interactive install $packages ;;
                pacman) pacman -Sy --noconfirm $packages ;;
                apk) apk add -y $packages ;;
            esac
        else
            case "$package_manager" in
                apt-get) apt-get update && apt-get install $packages ;;
                dnf|yum) "$package_manager" install $packages ;;
                zypper) zypper install $packages ;;
                pacman) pacman -S $packages ;;
                apk) apk add $packages ;;
            esac
        fi
    else
        echo "Planned package command:"
        case "$package_manager" in
            apt-get) printf '  apt-get update\n  apt-get install' ;;
            dnf|yum) printf '  %s install' "$package_manager" ;;
            zypper) printf '  zypper install' ;;
            pacman) printf '  pacman -S' ;;
            apk) printf '  apk add' ;;
        esac
        for package in $packages; do printf ' %s' "$package"; done
        printf '\n'
    fi
}

run_command

if [ "$execute" = "false" ]; then
    echo "No changes applied. Re-run with --execute."
    exit 0
fi

daemon_command() {
    target="$1"
    if command -v systemctl >/dev/null 2>&1; then
        printf 'systemctl enable --now %s' "$target"
    elif command -v rc-update >/dev/null 2>&1 && command -v rc-service >/dev/null 2>&1; then
        printf 'rc-update add %s default && rc-service %s start' "$target" "$target"
    else
        printf 'none'
    fi
}

if [ "$skip_daemon" != "true" ]; then
    case "$runtime" in
        docker|both)
            if [ "$execute" = "true" ]; then
                daemon_command=$(daemon_command "docker")
                if [ "$daemon_command" != "none" ]; then
                    eval "$daemon_command"
                fi
            else
                echo "Planned docker service command: $(daemon_command "docker")"
            fi
            ;;
    esac

    case "$runtime" in
        podman|both)
            if command -v systemctl >/dev/null 2>&1 && systemctl list-unit-files podman.service >/dev/null 2>&1; then
                if [ "$execute" = "true" ]; then
                    daemon_command=$(daemon_command "podman.service")
                    if [ "$daemon_command" != "none" ]; then
                        eval "$daemon_command"
                    fi
                else
                    echo "Planned podman service command: $(daemon_command "podman.service")"
                fi
            fi
            ;;
    esac
fi

if [ "$docker_group" = "true" ]; then
    if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER:-}" != "root" ]; then
        if command -v usermod >/dev/null 2>&1; then
            if [ "$execute" = "true" ]; then
                usermod -aG docker "$SUDO_USER"
            else
                echo "Planned docker group command: usermod -aG docker $SUDO_USER"
            fi
        else
            echo "usermod is unavailable; skipped Docker group assignment."
        fi
    else
        echo "No target user was detected for Docker group assignment."
    fi
fi

if [ "$runtime" = "docker" ] || [ "$runtime" = "both" ]; then
    if command -v docker >/dev/null 2>&1; then
        echo "Docker: $(docker --version)"
    fi
fi
if [ "$runtime" = "podman" ] || [ "$runtime" = "both" ]; then
    if command -v podman >/dev/null 2>&1; then
        echo "Podman: $(podman --version)"
    fi
fi
EOF
chmod +x scripts/install-runtimes.sh
sh -n scripts/install-runtimes.sh
echo "syntax-ok"
sh scripts/install-runtimes.sh --runtime docker --dry-run | head -20