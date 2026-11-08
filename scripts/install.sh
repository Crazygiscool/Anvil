#!/bin/sh
set -eu

repo="Crazygiscool/Anvil"
repo_url="https://github.com/${repo}.git"
install_dir="${INSTALL_DIR:-${HOME:-/usr/local}/.local/bin}"
force=false
use_source=false

case "${1:-}" in
    --help|-h)
        printf 'Usage: install.sh [--from-local]\n\nBy default, install the latest GitHub release. If it cannot be downloaded and Cargo is available, Anvil is built from source.\n'
        exit 0
        ;;
    --from-local)
        force=true
        ;;
    --from-source)
        use_source=true
        ;;
    "")
        ;;
    *)
        printf 'Unknown option: %s\n' "$1" >&2
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
if [ "$force" = true ]; then
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

    downloaded=false
    if command -v curl >/dev/null 2>&1; then
        if curl -fsSL "$url" -o "$tmp" 2>/dev/null; then
            downloaded=true
        fi
    elif command -v wget >/dev/null 2>&1; then
        if wget -q "$url" -O "$tmp"; then
            downloaded=true
        fi
    else
        echo "curl or wget is required for download installation." >&2
        exit 1
    fi

    if [ "$downloaded" = "true" ]; then
        binary_path="$tmp"
    else
        if [ "$use_source" = "false" ]; then
            if command -v cargo >/dev/null 2>&1; then
                use_source=true
            else
                echo "Could not download Anvil and Cargo is unavailable." >&2
                exit 1
            fi
        fi
        if [ "$use_source" = "true" ]; then
            source_dir=$(mktemp -d)
            source_cleanup() {
                rm -rf "$tmp" "$source_dir"
            }
            trap source_cleanup EXIT
            git clone --depth 1 "$repo_url" "$source_dir/anvil"
            rm -f "$source_dir/anvil/rust-toolchain.toml"
            (
            cd "$source_dir/anvil"
            rm -f rust-toolchain.toml
            export PATH="$HOME/.cargo/bin:$PATH"
            cargo build --release
        )
        binary_path="$source_dir/anvil/target/release/anvil"
    fi
fi
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


