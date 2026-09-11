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

sha256_of() {
    if command -v sha256sum > /dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum > /dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        fail "no sha256 tool available, refusing to install unverified"
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
base="https://github.com/$REPO/releases/download/$version"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "lore: downloading $version for $triple"
download "$base/$archive" "$work/$archive" || fail "could not download $base/$archive"

# The archive travels over https from a host nobody here controls the contents
# of after the fact. The checksums are published with the release, so verifying
# costs one more request and turns a swapped asset into a refusal.
download "$base/SHA256SUMS" "$work/SHA256SUMS" || fail "could not download the checksums"

# GNU sha256sum separates with two spaces and its binary mode with a space and
# a star, so both are accepted.
expected="$(sed -n "s/^\([0-9a-f]\{64\}\)[ *][ *]*$archive\$/\1/p" "$work/SHA256SUMS")"
[ -n "$expected" ] || fail "$archive is not listed in SHA256SUMS"

actual="$(sha256_of "$work/$archive")"
[ "$expected" = "$actual" ] || fail "checksum mismatch for $archive, refusing to install"

tar xzf "$work/$archive" -C "$work"

mkdir -p "$BIN_DIR"
install -m 755 "$work/lore-$version-$triple/lore" "$BIN_DIR/lore"

echo "lore: installed to $BIN_DIR/lore"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "lore: add $BIN_DIR to your PATH, then run: lore setup" ;;
esac

command -v lore > /dev/null 2>&1 && echo "lore: now run: lore setup"
