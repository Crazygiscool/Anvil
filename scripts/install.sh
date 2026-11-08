#!/bin/sh
# Quick installer for Anvil release binaries on supported Linux targets.
set -eu

repo="Crazygiscool/Anvil"
install_dir="${INSTALL_DIR:-${HOME:-/usr/local}/.local/bin}"
force=false

case "${1:-}" in
    --help|-h)
        cat <<'EOF'
Usage: install.sh [--from-local]

Installs the Anvil binary into INSTALL_DIR, defaulting to $HOME/.local/bin.

Anvil targets statically linked Linux binaries. Non-Linux systems are not currently
supported.
EOF
        exit 0
        ;;
    --from-local)
        force=true
        ;;
    "")
        ;;
    *)
        echo "Unknown option: $1" >&2
        exit 2
        ;;
esac

os=$(uname -s)
arch=$(uname -m)

if [ "$os" != "Linux" ]; then
    echo "Anvil does not provide release binaries for ${os}." >&2
    echo "Supported Linux targets: x86_64 and aarch64." >&2
    exit 1
fi

case "$arch" in
    x86_64)
        artifact="anvil-x86_64-unknown-linux-musl"
        ;;
    aarch64|arm64)
        artifact="anvil-aarch64-unknown-linux-musl"
        ;;
    *)
        echo "Unsupported Linux architecture: ${arch}" >&2
        exit 1
        ;;
esac

binary_path=""
if [ "$force" = "true" ]; then
    for candidate in \
        "target/x86_64-unknown-linux-musl/release/anvil" \
        "target/${arch}-unknown-linux-musl/release/anvil"
    do
        if [ -x "$candidate" ]; then
            binary_path="$candidate"
            break
        fi
    done
    if [ -z "$binary_path" ]; then
        echo "No local release binary was found." >&2
        exit 1
    fi
else
    url="https://github.com/${repo}/releases/latest/download/${artifact}"
    tmp=$(mktemp)
    cleanup() {
        rm -f "$tmp"
    }
    trap cleanup EXIT

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$tmp"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$url" -O "$tmp"
    else
        echo "curl or wget is required for download installation." >&2
        exit 1
    fi

    binary_path="$tmp"
fi

mkdir -p "$install_dir"
install -m 0755 "$binary_path" "$install_dir/anvil"

echo "Installed anvil to $install_dir/anvil"
case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
        echo "Start a new shell or add $install_dir to PATH to use anvil."
        ;;
esac
