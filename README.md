# ActionJev

An independent **Rust** code-review action for **GitHub Actions and Gitea Actions**, powered by [TypeSafe Jev](https://typesafe.ai/).

Jev returns bounded typed decisions rather than prose. ActionJev supplies review questions, selects concrete evidence, and applies policy in Rust. It does not run a coding agent, execute the reviewed project, or ask a chat model to invent review comments.

```text
committed Git objects
  → per-file Noul risk screening
  → Choice evidence-region selection
  → focused Noul support + Choice mechanism + Score impact
  → confidence / severity policy in Rust
  → report.json + report.md + trace.jsonl + optional PR summary
```

Inspired by the problem explored by [devagrawal09/jev-review](https://github.com/devagrawal09/jev-review), but **not a port or copy**: source code, prompts, policy and action integration are independently implemented. The repository's existing Apache-2.0 license is retained.

## What is included

- PR changes computed as **merge-base(base, head) → head**, not GitHub's synthetic merge commit. Works with both platforms' checkout behavior when the required commits are present.
- Full-codebase scans of tracked, committed source; no working-tree or untracked-file reads.
- Rust, Python, JavaScript/TypeScript, Go, Java/Kotlin, C/C++, C#, Swift, Ruby, PHP, shell, SQL, Terraform, configuration files, and other common source extensions.
- Configurable typed review policy, concurrency, file/byte/follow-up budgets, and three separate thresholds.
- Explicit incomplete-scan handling; malformed responses and network failures never turn into clean reports.
- PR summary creation/update through native GitHub/Gitea REST endpoints, with author checks and stale-head protection.
- JSON reports and question/answer JSONL for supervisors, other agents, and policy evaluation.
- Offline contract tests with fake Jev, GitHub, and Gitea HTTP endpoints. No paid API calls in CI.

**Not included:** a web dashboard, generated natural-language explanations, automatic patches, inline review threads, compiler execution, repository indexing, or automatic approval/merge. Evidence is at hunk/region granularity, not an assertion that a particular line is defective. The tests category checks demonstrably flawed supplied tests, not missing test coverage across unseen files.

## GitHub: first use

Add a repository secret named `TYPESAFE_API_KEY`. Then create `.github/workflows/jev-review.yml`:

```yaml
name: Jev review
on:
  pull_request:
    types: [opened, synchronize, reopened, ready_for_review]
permissions:
  contents: read
  pull-requests: write
concurrency:
  group: jev-${{ github.repository }}-${{ github.event.pull_request.number }}
  cancel-in-progress: true
jobs:
  review:
    # Do not disclose a paid API credential to arbitrary fork workflows.
    if: ${{ !github.event.pull_request.draft && github.event.pull_request.head.repo.full_name == github.repository }}
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@stable
      - uses: AlexBabescu/ActionJev@feat/rust-jev-action
        id: jev
        with:
          typesafe-api-key: ${{ secrets.TYPESAFE_API_KEY }}
          fail-on: none
```

The feature-branch ref above works before merge. For production, replace **all action refs with reviewed commit SHAs**, including this action. There is no published `v1` tag or release binary yet. The initial repository is private: enable the appropriate GitHub private-action access setting before using it from another repository you own; do not make it public merely to get the example working.

The action builds its trusted Rust source with `cargo build --release --locked` by default. **Cold compilation is not the fast path.** On self-hosted runners, preinstall the binary and use `binary-path`; then no Rust compiler is needed during review jobs.

## Gitea and the preinstalled-binary path

Mirror this action repository to your Gitea instance, or make an explicitly authenticated trusted checkout available to the runner. A Gitea runner does not automatically receive permission to download a private GitHub action.

Build once in the runner-image build process:

```sh
# In a trusted checkout pinned to the ActionJev revision you reviewed:
cargo build --release --locked
install -m 0755 target/release/actionjev /usr/local/bin/actionjev
```

Your job image then needs **Bash, Git, and the binary**, not Node or Python for ActionJev. A checkout action may independently require Node in the runner's job image. The job container needs outbound access to TypeSafe and your Git API. No Docker socket, privileged mode, nested Docker daemon, or Docker-in-Docker is used by this action.

Example `.gitea/workflows/jev-review.yml` (replace the action host and choose your configured runner label):

```yaml
name: Jev review
on:
  pull_request:
    types: [opened, synchronize, reopened]
jobs:
  review:
    if: ${{ github.event.pull_request.head.repo.full_name == github.repository }}
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          persist-credentials: false
      - uses: https://git.example.com/AlexBabescu/ActionJev@feat/rust-jev-action
        with:
          platform: gitea
          binary-path: /usr/local/bin/actionjev
          typesafe-api-key: ${{ secrets.TYPESAFE_API_KEY }}
          token: ${{ secrets.JEV_BOT_TOKEN }}
          api-url: ${{ github.api_url }}
          fail-on: none
```

Use a dedicated, repository-scoped Gitea bot token that can read PR metadata, list/write issue comments, and read its own user identity. Set `comment: 'false'` to avoid all Git API calls and remove the need for a publishing token. If your server uses an internal plaintext URL, `allow-insecure-http: 'true'` is an explicit opt-in; prefer HTTPS. The standard trust roots must recognize the server certificate.

`runs-on: ubuntu-latest` is only a label in Gitea. Configure it to select the image containing the binary. Serialize review jobs per PR in your runner/supervisor if your Gitea version does not implement workflow concurrency. Head checks reduce stale updates, but the comment API cannot atomically compare a PR head and publish a comment.

## CLI

Runtime dependencies: Git plus the compiled Rust executable. The implementation uses Rustls for HTTPS rather than a system OpenSSL dependency.

```sh
export TYPESAFE_API_KEY='your-key'

# Committed PR/branch changes; unresolved or shallow refs are an error.
actionjev --repo /work/project --base origin/main --head HEAD

# Full tracked-codebase scan; the default head is HEAD.
actionjev --repo /work/project --mode codebase

# Preview exclusions and scope without API requests. This is not a clean review.
actionjev --repo /work/project --base origin/main --dry-run

# Optional CI gate; confidence and support thresholds also apply.
actionjev --repo /work/project --base origin/main --fail-on high

# Additional exclusion globs; repeat --exclude as needed.
actionjev --repo /work/project --base origin/main --exclude '**/generated/**'
```

GitHub/Gitea event files supply PR base/head/repository metadata in CI. CLI overrides take precedence. Outside a PR event, changes mode requires `--base`; it never guesses a potentially incorrect branch. `--head` defaults to `HEAD` outside a PR event. Full-codebase mode still reads committed Git objects, not the working tree.

### Policy and default budgets

| Input / CLI flag | Default | Meaning |
|---|---:|---|
| `concurrency` | 4 | Simultaneous model requests |
| `max-files` | 50 | Reviewed source files |
| `max-file-bytes` | 48,000 | Limit per file on either side |
| `max-total-bytes` | 1,000,000 | Source plus evidence bytes |
| `max-regions` | 32 | Diff hunks / source regions per file |
| `max-followups` | 24 | Screened signals followed further |
| `screen-threshold` | 0.65 | Noul probability needed to investigate |
| `evidence-threshold` | 0.80 | Noul support needed for an actionable candidate |
| `min-confidence` | 0.70 | Minimum of evidence, mechanism and severity confidence |
| `fail-on` | `none` | Optional severity gate |
| `allow-incomplete` | `false` | Whether budget-limited scans may exit successfully |
| `timeout-seconds` | 30 | Timeout per HTTP attempt |

The `comment` action input defaults to `true`; the CLI publishes only with `--comment`. For `mode: codebase`, set `comment: 'false'` in the action. `typesafe-url` is the **complete** evaluation endpoint, defaulting to `https://api.typesafe.ai/v1/systemone`; this is not an OpenAI-compatible API.

Severity is Jev's probability-weighted position on five descriptive levels: 0 no demonstrated harm, 1 narrow edge case, 2 limited ordinary-operation failure, 3 important-operation failure or protected-data compromise, 4 broad irreversible loss or unrestricted compromise. Gate comparisons use the unrounded score: `low >= 1`, `medium >= 2`, `high >= 3`, `critical >= 4`. These thresholds are starter policy, **not empirically validated accuracy guarantees**.

Edit [prompts/review.json](prompts/review.json) and rebuild to change the bundled policy. Alternatively, pass `policy` / `--policy` pointing to a trusted JSON file. Do not use an untrusted PR's policy in a privileged workflow. The report records a SHA-256 of the exact policy bytes and traces retain the questions/answers so another agent can evaluate policy changes. No prompts are changed automatically.

### Exit codes

| Code | Meaning |
|---:|---|
| 0 | Completed without a configured gate failure, or explicitly allowed incomplete/dry run |
| 1 | Operational/configuration/API error; not a clean review |
| 2 | Incomplete review due to a budget limit; CLI argument errors may also use this code |
| 3 | An actionable finding reached the configured severity threshold |

Reports are written before severity/incomplete exits and before PR publishing. An operational failure during model evaluation currently aborts without a partial report; inspect the failing job rather than interpreting absent output as success. Do not use `continue-on-error` to hide these statuses in a required check.

## Outputs and review logs

The action exposes `report-json`, `report-markdown`, `trace-jsonl`, `findings`, `complete`, and `gate-failed`. Report files live in a new runner temporary directory. A runner job summary is also written when its output file is available.

`report.json` contains the reviewed commit/merge base, screening matrix, thresholds, policy hash, exclusions/coverage limitations, findings with exact evidence regions, uncertainty, and API call/token counts. `trace.jsonl` contains each stage's questions and typed answers, including distributions and the actual returned model ID. It is an application audit log, not hidden model reasoning. Traces intentionally omit request state/source and credentials; **the report's evidence snippets still contain private source code**.

The action does not upload artifacts automatically. Persist the files using an artifact action supported by your GitHub/Gitea runner, or have your supervisor consume them locally. Retain reports with the same access controls as the repository.

## Security and limitations

Reviewed source is sent to the configured TypeSafe service. Secret-like filenames, key files, common credentials files, dependency locks, generated directories and unsupported extensions are excluded; symlinks and submodules are never followed. **This is not a secret scanner:** a credential embedded in a normal source file can still be sent. Review the scope and add exclusions before using it on sensitive repositories.

Only bounded Git object reads and built-in Git diff operations are used. No project build, shell script, package install, Git hook, textconv or external diff command is executed by the reviewer. Arguments are passed directly to Git, paths are literal, HTTP redirects are disabled, credentials are sent only to explicitly configured service origins, and HTTP response bodies are withheld from error logs. A Git or runner vulnerability is outside this protection boundary.

Do not run the action or its source-build step from an untrusted local PR checkout with privileged secrets. Prefer a reviewed external action SHA and a preinstalled binary. The examples skip fork PRs rather than using `pull_request_target` with an untrusted checkout. For external contributions, design a separate trusted fetch/review/publish workflow and approval policy.

The reviewer selects at most one evidence region for each followed file/category signal. It does not prove program correctness, analyze an entire dependency graph, or establish model accuracy. A full-codebase scan means all eligible committed files **within configured limits**, not exhaustive static analysis. Binary, unsupported and intentionally excluded files remain outside that scope. Raise budgets explicitly for larger repositories.

HTTP inference and reads use up to three attempts for transient errors, with exponential backoff and bounded `Retry-After` handling; an inference retry can incur another billed request. Comment creation is not retried automatically after ambiguous network failures. Comment discovery checks up to 20 pages and refuses to create a possible duplicate when that budget is exhausted. Use a dedicated bot identity consistently.

## Development

```sh
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
python3 tests/e2e.py target/release/actionjev
```

The Python test harness is a development dependency only. It creates temporary Git repositories and local mock services; no real model credential or paid API is needed. CI builds the Rust binary and runs these checks. See [docs/architecture.md](docs/architecture.md) for the data flow and [docs/verification.md](docs/verification.md) for what is and is not tested.

## Primary references

- [TypeSafe HTTP API](https://docs.typesafe.ai/api)
- [Choice, Score and Noul](https://docs.typesafe.ai/primitives)
- [Confidence semantics](https://docs.typesafe.ai/confidence)
- [GitHub issue-comment API](https://docs.github.com/en/rest/issues/comments)
- [Gitea API](https://docs.gitea.com/api/1.25/)
- [Reference experiment: jev-review](https://github.com/devagrawal09/jev-review)

License: [Apache-2.0](LICENSE).
