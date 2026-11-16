#!/usr/bin/env bash
# Pack the release build into a Fedora/RHEL .rpm package:
#
#   packaging/rpm.sh           → dist/tvdash-<version>-1.<arch>.rpm
#
# Requires: cargo install cargo-generate-rpm
# On the destination Fedora machine:
#   sudo dnf install ./tvdash-<version>-1.<arch>.rpm
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(dirname "$here")"
bin="$repo/target/release/tvdash"
if [[ ! -x "$bin" ]]; then
    echo "No release build at $bin. Build it first:  cargo build --release -j 2" >&2
    exit 1
fi

if ! command -v cargo-generate-rpm >/dev/null 2>&1; then
    echo "cargo-generate-rpm not found. Install it with:  cargo install cargo-generate-rpm" >&2
    exit 1
fi

out="$repo/dist"
mkdir -p "$out"
(cd "$repo" && cargo generate-rpm -p crates/app -o "$out")

rpm_file="$(ls -t "$out"/tvdash-*.rpm 2>/dev/null | head -1 || true)"
if [[ -n "$rpm_file" ]]; then
    echo "Wrote $rpm_file"
    echo "Install on Fedora:  sudo dnf install $rpm_file"
fi
