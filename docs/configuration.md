# Configuration

[Documentation home](../README.md#documentation)

[action.yml](../action.yml) defines every action input and output. The action and
CLI share the review engine, but their defaults differ for comments: the action
publishes by default, while the CLI requires `--comment`.

## Scope and budgets

| Input | Default | Meaning |
|---|---|---|
| `mode` | `changes` | Review the diff, or use `codebase` for eligible tracked source. |
| `model` | `jev-latest` | TypeSafe model ID or alias. |
| `concurrency` | `4` | Maximum simultaneous model requests. |
| `max-files` | `50` | Maximum files reviewed. |
| `max-file-bytes` | `48000` | Per-file source limit on either side of a change. |
| `max-total-bytes` | `1000000` | Combined source and evidence byte budget. |
| `max-regions` | `32` | Evidence regions per file. |
| `max-followups` | `24` | Category assessments selected for follow-up. |
| `timeout-seconds` | `30` | Per-request timeout. |
| `allow-incomplete` | `false` | Whether exhausted review budgets may exit successfully. |

PR changes are measured from the merge base of `base` and `head` to `head`, not
from the platform's synthetic merge commit. Fetch both histories, usually with
`fetch-depth: 0`. The reviewer ignores worktree changes, untracked files, and
symlink targets.

Set `path`, `base`, or `head` to override the repository path or inferred refs.
Common Rust, Python, JavaScript/TypeScript, Go, Java/Kotlin, C/C++, Swift, shell,
SQL, Terraform, and configuration files are eligible. See
[git.rs](../src/git.rs) for the exact extension and exclusion rules.

Add newline-separated globs with `exclude`. Built-in secret exclusions remain
active. Excluded files are recorded; budget exhaustion marks the review incomplete.
Allowing an incomplete exit does not change that report status.

## Thresholds and gates

| Input | Default | Purpose |
|---|---|---|
| `screen-threshold` | `0.65` | Minimum category score for follow-up. |
| `evidence-threshold` | `0.8` | Minimum support for an actionable assessment. |
| `min-confidence` | `0.7` | Minimum retained Choice/Score confidence. |
| `fail-on` | `none` | Severity gate: `none`, `low`, `medium`, `high`, or `critical`. |

An actionable assessment must pass both support and confidence thresholds and
have impact of at least 1. The gate thresholds are 1, 2, 3, and 4 for low through
critical. Uncertain assessments do not fail the gate. Operational errors and
incomplete reviews can still fail the job with `fail-on: none`.

Lowering screening thresholds can increase requests. Lowering support or
confidence thresholds changes which assessments count as actionable. Evaluate
both broken and correct examples before changing these settings.

## Customize the policy

Start with [prompts/review.json](../prompts/review.json). Its fields define:

| Field | Purpose |
|---|---|
| `version` | Policy schema version, currently `1`. |
| `guidance` | Shared instructions added to review questions. |
| `dimensions` | Category IDs and their screening questions. |
| `mechanisms` | Predefined defect choices, including `none`. |
| `severity_levels` | Ordered descriptions used for impact scoring. |

Keep the default five severity levels if you want the existing gate thresholds
and report scale to retain their meaning. Jev chooses from these definitions;
adding a mechanism does not enable it to write a diagnosis or generate a patch.
Stage-specific follow-up questions also live in [review.rs](../src/review.rs).

For a released action, pass `policy: /absolute/path/to/review.json`. Obtain that
file from a trusted revision. Never load a policy from the PR under review. The
report records the policy's SHA-256 so results can be tied to the exact policy.
The built-in policy is bundled with the binary.

## Reports and comments

| Output | Contents |
|---|---|
| `report-json` | Absolute path to `report.json`, with scope, signals, assessments, and source evidence. |
| `report-markdown` | Absolute path to the rendered review summary. |
| `trace-jsonl` | Absolute path to questions, typed answers, timing, and usage. |
| `findings` | Number of actionable category assessments. |
| `complete` | Whether the configured scope was completed. |
| `gate-failed` | Whether an actionable assessment reached the requested severity gate. |

Reports are created in a fresh runner-temporary directory. Markdown is appended
to the job summary. Trace records omit request source state and credentials,
but reports contain source snippets and should receive the same access protection
as the repository. Artifact uploads are a caller choice.

Use `comment: 'false'` for read-only tokens or `mode: codebase`. Use
`dry-run: 'true'` to inspect scope without model requests or comment publication.
With comments enabled, the reviewer updates its own marked conversation comment
and checks that the PR head is still current before publishing.

## Run locally

Build from source with a Rust toolchain and Git:

```sh
cargo build --release --locked
./target/release/actionjev --help
./target/release/actionjev --repo /path/to/repo --base origin/main --head HEAD --dry-run
```

For a real review, provide `TYPESAFE_API_KEY` through your environment or secret
manager, then omit `--dry-run`. The CLI never accepts the key as an argument.
For example, in Bash you can enter it without putting its value in shell history:

```bash
read -r -s -p 'TypeSafe API key: ' TYPESAFE_API_KEY
export TYPESAFE_API_KEY
printf '\n'
./target/release/actionjev --repo /path/to/repo --base origin/main --head HEAD
unset TYPESAFE_API_KEY
```

For a full-codebase scan, use `--mode codebase --head HEAD` instead of `--base`.
To use source in a composite action, explicitly select `build-from-source: 'true'`
with Cargo installed, or supply an absolute trusted `binary-path`.

| Exit code | Meaning |
|---|---|
| `0` | Completed without a failing gate, or incomplete coverage explicitly allowed. |
| `1` | Runtime or configuration failure. |
| `2` | Incomplete review unless allowed, or CLI argument parsing error. |
| `3` | Severity gate failed after reports were written. |
