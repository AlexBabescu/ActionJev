#!/usr/bin/env bash
set -euo pipefail
args=(--repo "$AJ_PATH" --mode "$AJ_MODE" --platform "$AJ_PLATFORM"
  --typesafe-url "$AJ_TYPESAFE_URL" --model "$AJ_MODEL"
  --concurrency "$AJ_CONCURRENCY" --max-files "$AJ_MAX_FILES"
  --max-file-bytes "$AJ_MAX_FILE_BYTES" --max-total-bytes "$AJ_MAX_TOTAL_BYTES"
  --max-regions "$AJ_MAX_REGIONS" --max-followups "$AJ_MAX_FOLLOWUPS"
  --screen-threshold "$AJ_SCREEN_THRESHOLD" --evidence-threshold "$AJ_EVIDENCE_THRESHOLD"
  --min-confidence "$AJ_MIN_CONFIDENCE" --fail-on "$AJ_FAIL_ON" --timeout-seconds "$AJ_TIMEOUT")
[[ -z "$AJ_BASE" ]] || args+=(--base "$AJ_BASE")
[[ -z "$AJ_HEAD" ]] || args+=(--head "$AJ_HEAD")
[[ -z "$AJ_REPOSITORY" ]] || args+=(--repository "$AJ_REPOSITORY")
[[ -z "$AJ_PR_NUMBER" ]] || args+=(--pr-number "$AJ_PR_NUMBER")
[[ -z "$AJ_API_URL" ]] || args+=(--api-url "$AJ_API_URL")
[[ -z "$AJ_POLICY" ]] || args+=(--policy "$AJ_POLICY")
flag() {
  case "$2" in
    true) args+=("$1");;
    false) ;;
    *) printf 'Invalid boolean for %s\n' "$1" >&2; exit 1;;
  esac
}
flag --allow-incomplete "$AJ_ALLOW_INCOMPLETE"
flag --comment "$AJ_COMMENT"
flag --dry-run "$AJ_DRY_RUN"
flag --allow-insecure-http "$AJ_ALLOW_HTTP"
while IFS= read -r pattern; do
  pattern="${pattern%$'\r'}"
  [[ -z "$pattern" ]] || args+=(--exclude "$pattern")
done <<< "$AJ_EXCLUDE"
# A fresh directory outside the source checkout prevents a PR from planting report symlinks.
output="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/actionjev.XXXXXXXX")"
args+=(--output-dir "$output")
exec "$ACTIONJEV_BINARY" "${args[@]}"
