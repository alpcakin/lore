#!/bin/sh
# Installs lore into ~/.local/bin. No administrator rights, nothing outside
# your home directory.
#
#   curl -fsSL https://raw.githubusercontent.com/alpcakin/lore/main/packaging/install.sh | sh

set -eu

REPO="alpcakin/lore"
BIN_DIR="${LORE_BIN_DIR:-$HOME/.local/bin}"

fail() {
    echo "lore: $1" >&2
    exit 1
}

target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os-$arch" in
        Darwin-arm64) echo "aarch64-apple-darwin" ;;
        Darwin-x86_64) echo "x86_64-apple-darwin" ;;
        Linux-aarch64 | Linux-arm64) echo "aarch64-unknown-linux-gnu" ;;
        # The musl build runs whatever the host's glibc turns out to be.
        Linux-x86_64) echo "x86_64-unknown-linux-musl" ;;
        *) fail "no prebuilt binary for $os on $arch, try: cargo install cmdlore" ;;
    esac
}

download() {
    if command -v curl > /dev/null 2>&1; then
        curl -fsSL "$1" -o "$2"
    elif command -v wget > /dev/null 2>&1; then
        wget -qO "$2" "$1"
    else
        fail "neither curl nor wget is available"
    fi
}

version="${LORE_VERSION:-}"
if [ -z "$version" ]; then
    version="$(download "https://api.github.com/repos/$REPO/releases/latest" /dev/stdout \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)"
fi
[ -n "$version" ] || fail "could not work out the latest version"

triple="$(target)"
archive="lore-$version-$triple.tar.gz"
url="https://github.com/$REPO/releases/download/$version/$archive"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "lore: downloading $version for $triple"
download "$url" "$work/$archive" || fail "could not download $url"
tar xzf "$work/$archive" -C "$work"

mkdir -p "$BIN_DIR"
install -m 755 "$work/lore-$version-$triple/lore" "$BIN_DIR/lore"

echo "lore: installed to $BIN_DIR/lore"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "lore: add $BIN_DIR to your PATH, then run: lore setup" ;;
esac

command -v lore > /dev/null 2>&1 && echo "lore: now run: lore setup"
