#!/usr/bin/env bash
# Pack the release build into a self-contained archive for other machines:
#
#   packaging/bundle.sh        → dist/tvdash-<version>-x86_64.tar.gz
#
# On the other machine: tar xzf tvdash-*.tar.gz && tvdash-*/install.sh
# (no Rust or source needed there). See docs/deploying.md.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(dirname "$here")"
bin="$repo/target/release/tvdash"
if [[ ! -x "$bin" ]]; then
    echo "No release build at $bin. Build it first:  cargo build --release" >&2
    exit 1
fi

version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/Cargo.toml" | head -1)"
commit="$(git -C "$repo" rev-parse --short HEAD 2>/dev/null || echo unknown)"
name="tvdash-${version}-$(uname -m)"
out="$repo/dist"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

mkdir -p "$stage/$name" "$out"
install -m 755 "$bin" "$stage/$name/tvdash"
install -m 755 "$here/install.sh" "$stage/$name/install.sh"
install -m 644 "$here/tvdash.desktop.in" "$here/tvdash.svg" "$stage/$name/"
install -m 644 "$repo/docs/deploying.md" "$stage/$name/DEPLOYING.md"
echo "$commit" > "$stage/$name/COMMIT"

tar -C "$stage" -czf "$out/$name.tar.gz" "$name"
echo "Wrote $out/$name.tar.gz (commit $commit)"
echo "On the other machine:  tar xzf $name.tar.gz && $name/install.sh"
