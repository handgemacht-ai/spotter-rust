#!/usr/bin/env bash
set -euo pipefail

root="${1:-target/install-smoke}"
rm -rf "$root"
cargo install --path . --locked --root "$root"
"$root/bin/spotter" --version | grep -qx "spotter $(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
"$root/bin/spotter" --help >/dev/null
