# Architecture

ActionJev has a Rust CLI core and a Bash composite-action adapter. There is no persistent service, database, Docker dependency, or JavaScript runtime in the reviewer.

## Modules

| Module | Responsibility |
|---|---|
| `config.rs` | Typed CLI arguments, bounds validation, CI event/environment resolution |
| `git.rs` | Resolve immutable commit IDs; merge-base; bounded literal Git object/diff reads; exclusions; evidence regions |
| `http.rs` | Rustls HTTP transport; timeout and response limits; redirects disabled; retries; trusted pagination |
| `jev.rs` | TypeSafe `/v1/systemone`; typed response decoding; probability/option validation; question/answer traces |
| `review.rs` | Load/hash policy, bounded worker pool, screen/locate/judge orchestration, uncertainty and gate policy |
| `output.rs` | JSON/Markdown/JSONL files, runner outputs, native GitHub/Gitea PR comments |
| `scripts/` | Compile trusted action source or select a preinstalled binary; map action inputs to safe argv |

## Evidence and review stages

A changes scan resolves base and head to actual commit IDs and uses their merge base. Both platform event formats can provide the two refs. All source reads come from Git blobs, not the checked-out worktree. Deletions use the old blob; new/current files use the head blob. Renames are treated as delete/add, avoiding heuristic rename matching. Full-codebase mode walks the selected commit tree and partitions eligible source into overlapping 80-line regions with a 70-line stride.

Each file sends one request containing independent Noul questions for the configured categories. Signals above the screening threshold are ordered deterministically and a bounded number are followed. A Choice call selects an existing evidence-region ID or `none`. A subsequent call, focused on that selected evidence, independently evaluates support, defect mechanism, and impact. No question in a single batch is allowed to depend on another answer from that same batch.

The minimum of the three Choice/Score confidence values is retained as a conservative workflow rule, not a statistically calibrated combined probability. Noul support and initial risk probability remain separate values. Low-confidence candidates appear as uncertain and never fail a severity gate. A `none` evidence or mechanism answer suppresses the candidate. Source context is bounded and file-local; it does not silently pretend to understand unseen callers or tests.

## API boundaries

TypeSafe: `POST /v1/systemone`, `Authorization: Bearer`, with `model`, structured `state`, and typed `questions`. Answers must match question IDs, types, supplied Choice keys, valid probability ranges/distributions, and the configured Score range. Unknown or malformed answers are operational failures.

GitHub/Gitea: read the PR head; identify the token owner; paginate PR conversation comments; update the bot's marked comment or create one. GitHub uses `per_page`; Gitea uses `limit`. Both use `/repos/{owner}/{repo}/issues/{number}/comments` for PR conversation comments and `/repos/{owner}/{repo}/issues/comments/{id}` for edits. API base subpaths are preserved. A second PR-head read occurs just before publishing. An unavoidable final race remains; the workflow/supervisor should serialize reviews per PR.

The built-in GitHub Actions token may not access `/user`; its author fallback is `github-actions[bot]`. Custom GitHub App tokens that are not that bot should use a suitable user/bot token for predictable comment reuse. Gitea requires a readable bot identity rather than overwriting comments based solely on a marker.

## Resource and failure behavior

Defaults bound source/evidence to approximately 1 MB in total, at most 50 files, 32 regions per file, 24 follow-ups and four simultaneous requests. These are work bounds, not a measured peak-RSS guarantee. Git metadata reads have a 32 MB cap, model request serialization has a 500 KB cap, and HTTP responses have an 8 MB cap. Source/response serialization and TLS add memory above the input budget.

Exclusions are recorded. File/region/byte/follow-up limits make `complete=false` and normally exit 2. Operational failures exit 1. Configured severity gates exit 3 after reports are written. Absence of a report must never be interpreted as a clean review. All normal outputs are deterministic in structure; parallel trace arrival order and model output are not deterministic.

## Policy evaluation by other agents

Store the source revision, model returned in the trace, policy SHA-256, report and trace together. A supervisor can compare candidate outcomes against human labels, edit a trusted copy of `prompts/review.json`, and submit that change through normal review. The runner does not grant the model tool access or permit it to modify prompts, thresholds, code, or Git state.
