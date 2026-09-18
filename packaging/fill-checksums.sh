#!/bin/sh
# Fills the Homebrew formula and the Scoop manifest with the checksums of a
# published release, ready to copy into their own repositories.
#
#   packaging/fill-checksums.sh 0.1.0
#
# Reads SHA256SUMS from the release and writes the two files in place. The
# release has to be published, not a draft: draft assets cannot be downloaded
# without a token.

set -eu

REPO="alpcakin/lore"
here="$(cd "$(dirname "$0")" && pwd)"

version="${1:-}"
[ -n "$version" ] || { echo "usage: $0 <version>" >&2; exit 2; }
version="${version#v}"

sums="$(mktemp)"
trap 'rm -f "$sums"' EXIT
curl -fsSL "https://github.com/$REPO/releases/download/v$version/SHA256SUMS" -o "$sums" \
    || { echo "could not download SHA256SUMS for v$version, is the release published?" >&2; exit 1; }

sum_for() {
    sed -n "s/^\([0-9a-f]\{64\}\)[ *][ *]*lore-v$version-$1\.[a-z.]*$/\1/p" "$sums" | head -n 1
}

fill() {
    target="$1"
    checksum="$(sum_for "$target")"
    [ -n "$checksum" ] || { echo "no checksum for $target in SHA256SUMS" >&2; exit 1; }
    echo "$checksum"
}

mac_arm="$(fill aarch64-apple-darwin)"
mac_intel="$(fill x86_64-apple-darwin)"
linux_arm="$(fill aarch64-unknown-linux-gnu)"
linux_intel="$(fill x86_64-unknown-linux-musl)"
windows="$(fill x86_64-pc-windows-msvc)"

formula="$here/homebrew/lore.rb"
manifest="$here/scoop/lore.json"

# The formula lists the four archives in a fixed order, so each placeholder or
# stale checksum is replaced by position.
awk -v v="$version" -v a="$mac_arm" -v b="$mac_intel" -v c="$linux_arm" -v d="$linux_intel" '
    /^  version "/ { sub(/"[^"]*"/, "\"" v "\"") }
    /^      sha256 "/ {
        n++
        sum = (n == 1) ? a : (n == 2) ? b : (n == 3) ? c : d
        sub(/"[^"]*"/, "\"" sum "\"")
    }
    { print }
' "$formula" > "$formula.tmp" && mv "$formula.tmp" "$formula"

sed -e "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" \
    -e "s#download/v[0-9][^/]*/lore-v[0-9][^-]*-x86_64-pc-windows-msvc.zip#download/v$version/lore-v$version-x86_64-pc-windows-msvc.zip#" \
    -e "s/\"extract_dir\": \"lore-v[0-9][^\"]*\"/\"extract_dir\": \"lore-v$version-x86_64-pc-windows-msvc\"/" \
    -e "s/\"hash\": \"[^\"]*\"/\"hash\": \"$windows\"/" \
    "$manifest" > "$manifest.tmp" && mv "$manifest.tmp" "$manifest"

echo "filled $formula and $manifest for v$version"
echo "copy them to alpcakin/homebrew-tap Formula/lore.rb and alpcakin/scoop-bucket bucket/lore.json"
