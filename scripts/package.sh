#!/usr/bin/env bash
set -euo pipefail
target="${1:?Rust target required}"
case "$target" in x86_64-unknown-linux-musl|aarch64-unknown-linux-musl) ;; *) exit 1;; esac
mkdir -p dist
asset="actionjev-$target.gz"
gzip -n -9 -c "target/$target/release/actionjev" > "dist/$asset"
(cd dist; sha256sum "$asset" > "$asset.sha256"; cat ./*.sha256 | LC_ALL=C sort > SHA256SUMS)
