#!/usr/bin/env bash
set -euo pipefail
if [[ -n "${ACTIONJEV_BINARY_PATH:-}" ]]; then
  [[ "$ACTIONJEV_BINARY_PATH" == /* && -x "$ACTIONJEV_BINARY_PATH" ]] || { echo 'binary-path must point to an absolute, trusted executable' >&2; exit 1; }
  binary="$ACTIONJEV_BINARY_PATH"
else
  command -v cargo >/dev/null || { echo 'Install a Rust toolchain in the runner, or provide binary-path to a preinstalled ActionJev binary.' >&2; exit 1; }
  # Build from the trusted action checkout, not the reviewed repository's .cargo/config.
  cd "$ACTIONJEV_ACTION_PATH"
  export CARGO_TARGET_DIR="$ACTIONJEV_ACTION_PATH/target"
  cargo build --release --locked
  binary="$CARGO_TARGET_DIR/release/actionjev"
fi
[[ "$binary" != *$'\n'* && "$binary" != *$'\r'* ]] || exit 1
printf 'binary=%s\n' "$binary" >> "${GITHUB_OUTPUT:-${GITEA_OUTPUT:?Missing action output file}}"
