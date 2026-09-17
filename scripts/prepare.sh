#!/usr/bin/env bash
# The reviewed checkout is never a source of binaries or build configuration.
set -euo pipefail
umask 077
fail() { printf 'ActionJev: %s\n' "$*" >&2; exit 1; }
action_path="${ACTIONJEV_ACTION_PATH:?Missing trusted action path}"
build="${ACTIONJEV_BUILD_FROM_SOURCE:-false}"
[[ "$build" == true || "$build" == false ]] || fail 'build-from-source must be true or false'
if [[ -n "${ACTIONJEV_BINARY_PATH:-}" ]]; then
  [[ "$ACTIONJEV_BINARY_PATH" == /* && -x "$ACTIONJEV_BINARY_PATH" ]] || fail 'binary-path must be an absolute trusted executable'
  binary="$ACTIONJEV_BINARY_PATH"
elif [[ "$build" == true ]]; then
  command -v cargo >/dev/null || fail 'build-from-source requires a Rust toolchain'
  cd "$action_path"
  export CARGO_TARGET_DIR="$action_path/target"
  cargo build --release --locked
  binary="$CARGO_TARGET_DIR/release/actionjev"
else
  case "$(uname -s)/$(uname -m)" in
    Linux/x86_64|Linux/amd64) target=x86_64-unknown-linux-musl ;;
    Linux/aarch64|Linux/arm64) target=aarch64-unknown-linux-musl ;;
    *) fail 'Bundled binaries support Linux x64/ARM64. Use binary-path or build-from-source for another platform.' ;;
  esac
  asset="actionjev-$target.gz"
  [[ -f "$action_path/dist/$asset" && -f "$action_path/dist/SHA256SUMS" ]] || fail 'No packaged binary. Use a published version tag (v0), not main, or explicitly set build-from-source: true.'
  command -v sha256sum >/dev/null || fail 'sha256sum is required'
  command -v gzip >/dev/null || fail 'gzip is required'
  expected="$(awk -v name="$asset" '$2 == name { hash=$1; count++ } END { if (count != 1) exit 1; print hash }' "$action_path/dist/SHA256SUMS")" || fail 'Missing or duplicate package checksum'
  [[ "$expected" =~ ^[0-9a-f]{64}$ ]] || fail 'Invalid package checksum'
  # The digest is committed alongside the action, not downloaded from a mutable release.
  (cd "$action_path/dist"; printf '%s  %s\n' "$expected" "$asset" | sha256sum --check --status) || fail 'Packaged binary checksum mismatch'
  directory="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/actionjev-bin.XXXXXXXX")"
  trap 'rm -rf -- "$directory"' EXIT
  binary="$directory/actionjev"
  gzip -cd -- "$action_path/dist/$asset" > "$binary"
  chmod 700 "$binary"
  "$binary" --version
  trap - EXIT
fi
[[ "$binary" != *$'\n'* && "$binary" != *$'\r'* ]] || fail 'Invalid binary path'
printf 'binary=%s\n' "$binary" >> "${GITHUB_OUTPUT:-${GITEA_OUTPUT:?Missing action output file}}"
