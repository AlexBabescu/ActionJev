# ActionJev

An independent **Rust code-review action for GitHub Actions and Gitea Actions**, powered by [TypeSafe Jev](https://typesafe.ai/).

Published tags bundle **checksum-verified static binaries for Linux x64 and ARM64**. Consumers do not install Rust, compile the reviewer, start Docker, or download separate release assets. The only user-created secret is `TYPESAFE_API_KEY`; PR comments use the platform's automatic job token.

Inspired by the problem explored by [devagrawal09/jev-review](https://github.com/devagrawal09/jev-review), but not a port or copy. Source, prompts, policy and integration are independently implemented under Apache-2.0.

## GitHub: import with your own key

Add a repository/action secret named `TYPESAFE_API_KEY`, then create `.github/workflows/jev-review.yml`:

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
    # Fork workflows do not receive your paid API key.
    if: ${{ !github.event.pull_request.draft && github.event.pull_request.head.repo.full_name == github.repository }}
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          persist-credentials: false
      - uses: AlexBabescu/ActionJev@v0
        id: jev
        with:
          typesafe-api-key: ${{ secrets.TYPESAFE_API_KEY }}
```

That is the entire setup. The automatically supplied `${{ github.token }}` is used for PR comments; do not create a PAT. An imported action cannot automatically read arbitrary repository secrets, so the explicit `with:` mapping is required. Each caller supplies and pays for its own TypeSafe key; the publisher's repository secret is never bundled or shared.

Start with the default `fail-on: none`. Add `fail-on: high` only after evaluating the reviewer on known changes. Use a release's **distribution commit SHA** instead of `@v0` for immutable action code, prompts and binary digests. `@v0.1.0` is the fixed first release; `@v0` is its moving major alias. For production, also pin checkout to a reviewed SHA.

## Gitea

Create `.gitea/workflows/jev-review.yml`, using a runner label configured on your instance:

```yaml
name: Jev review
on:
  pull_request:
    types: [opened, synchronize, reopened]
permissions:
  contents: read
  issues: write
  pull-requests: write
jobs:
  review:
    if: ${{ github.event.pull_request.head.repo.full_name == github.repository }}
    runs-on: ubuntu-latest
    steps:
      - uses: https://github.com/actions/checkout@v4
        with:
          fetch-depth: 0
          persist-credentials: false
      - uses: https://github.com/AlexBabescu/ActionJev@v0
        with:
          typesafe-api-key: ${{ secrets.TYPESAFE_API_KEY }}
          platform: gitea
          token: ${{ secrets.GITEA_TOKEN }}
```

`GITEA_TOKEN` is created by Gitea itself; **do not create another secret with that name**. Explicitly passing it makes the platform choice unambiguous. Owner/server permission limits still apply. For report-only use with read-only tokens, add `comment: 'false'`.

**Access:** arbitrary unrelated users can import these examples only when ActionJev is public. Publishing tags or releases does not change repository visibility. While private, configure supported private-action sharing or mirror to an accessible Gitea repository and change the action URL. **Mirror version tags and their objects, not just main:** the packaged binaries live in those tags. No separate GitHub download token is needed for a Gitea-hosted copy.

Runtime requirements: Linux x64/ARM64, Bash, Git, gzip, awk and sha256sum. A separate checkout action may require Node in a Gitea job image. ActionJev itself needs no Node/Python runtime, Cargo, Docker-in-Docker, Docker socket or privileged mode.

## Review workflow

```text
Committed Git objects
  -> per-file Noul risk screening
  -> Choice evidence-region selection
  -> focused Noul support + Choice mechanism + Score impact
  -> confidence/severity policy in Rust
  -> JSON, Markdown, JSONL and optional PR summary
```

Jev returns typed judgments, not generated review prose. ActionJev selects existing evidence and applies policy in Rust. Findings are **review candidates, not proven defects**. Confidence describes the model's answer distribution, not an empirically calibrated probability that a bug exists.

PR scope is `merge-base(base, head) -> head`, independent of GitHub's synthetic merge checkout. Both commit histories must be available: use `fetch-depth: 0`. Full-codebase mode reviews eligible tracked source at the selected commit. Working-tree edits, untracked files and symlink targets are not reviewed, and project code is never executed.

Categories include correctness, security, reliability, compatibility and demonstrably flawed supplied tests. Common Rust, Python, JavaScript/TypeScript, Go, Java/Kotlin, C/C++, Swift, shell, SQL, Terraform and configuration files are supported. There is no dashboard, generated prose, patch generation, automatic approval, inline review threads or compiler execution.

## Configuration

See [action.yml](action.yml) for the complete input/output contract.

| Input | Default | Purpose |
|---|---|---|
| `typesafe-api-key` | Required | Caller-owned API key, passed only to TypeSafe. |
| `token` | Automatic job token | Optional override for PR-comment authentication. |
| `mode` | `changes` | `changes` or full committed `codebase` scan. |
| `base`, `head` | PR payload | Explicit commit/ref overrides; `head` otherwise defaults to `HEAD`. |
| `path` | `.` | Checked-out Git repository. |
| `platform` | `auto` | `github`, `gitea`, `none` or auto-detection. |
| `api-url` | Runner context | Git service API, including `/api/v1` for Gitea. |
| `model` | `jev-latest` | TypeSafe model ID/alias. |
| `policy` | Embedded JSON policy | Explicit trusted policy file; never auto-loaded from reviewed code. |
| `concurrency` | `4` | Maximum simultaneous model requests. |
| `max-files` | `50` | Eligible file budget. |
| `max-file-bytes` | `48000` | Per-file source limit. |
| `max-total-bytes` | `1000000` | Total source plus evidence budget. |
| `max-regions` | `32` | Evidence regions per file. |
| `max-followups` | `24` | Screened signals investigated. |
| `screen-threshold` | `0.65` | Noul threshold for follow-up. |
| `evidence-threshold` | `0.8` | Support required for an actionable finding. |
| `min-confidence` | `0.7` | Choice and Score confidence required for gating. |
| `fail-on` | `none` | `none`, `low`, `medium`, `high`, `critical`. |
| `allow-incomplete` | `false` | Explicitly permit successful exit when a coverage budget is exhausted. |
| `comment` | `true` | Create/update a PR summary; disable for codebase scans. |
| `dry-run` | `false` | Inspect scope without calling Jev or posting comments. |
| `exclude` | Empty | Additional newline-separated glob exclusions. |
| `binary-path` | Bundled release binary | Optional absolute trusted executable path. |
| `build-from-source` | `false` | Explicit Cargo build of trusted action source. |

Only actionable findings can trigger a severity gate. Uncertain findings are reported but not gated. Budget-limited scans remain marked incomplete; API and validation failures are errors, not clean reviews.

The editable [policy](prompts/review.json) contains guidance, typed questions, mechanism choices and severity levels. Reports record its SHA-256; supervisors can inspect policy and question/answer traces without reverse-engineering an opaque agent session.

### Outputs

`report-json`, `report-markdown` and `trace-jsonl` are absolute paths in a fresh runner-temporary directory. `findings` counts actionable candidates; `complete` and `gate-failed` expose status. Markdown is also appended to the runner's job summary. PR comments are author-checked before update, paginated, and suppressed when the PR head has moved.

`report.json` contains source evidence and should be protected like the repository. `trace.jsonl` contains questions, typed answers, timing and usage, not credentials or source state. Upload reports as artifacts only under an appropriate access/retention policy; artifact upload is deliberately not a runtime dependency for Gitea compatibility.

## Local/source use

Source branches, including `main`, do not contain generated binary blobs. Use a published tag/distribution SHA for normal action imports. Explicit `build-from-source: 'true'` requires an installed Rust toolchain. A preinstalled `binary-path` overrides the bundled executable.

```bash
cargo build --release --locked
export TYPESAFE_API_KEY='your-key'
./target/release/actionjev --repo /path/to/repo --base origin/main --head HEAD
./target/release/actionjev --repo /path/to/repo --mode codebase --head HEAD
```

Secrets are read from environment variables, not command-line arguments. Exit status is `0` for success, `1` for runtime/configuration errors, `2` for a severity gate and `3` for an incomplete scan unless explicitly allowed. CLI argument parsing may also use exit status `2`.

## Security and verification

Eligible source is sent to the configured TypeSafe service. Common secret filenames and symlinks are excluded, but filename exclusions are **not a secret scanner**. Review TypeSafe's data handling before using private code. Reports with snippets remain sensitive.

Do not use `pull_request_target` to run an untrusted PR's workflow, build scripts, action implementation or policy with secrets. Keep the action pinned and the workflow trusted; do not run arbitrary project code in the credential-bearing job. Custom endpoints/policies are trusted inputs. Credentials are never committed or included in release assets.

CI runs 15 Rust unit tests, 23 offline Git/HTTP integration tests, launcher tests, Clippy, static builds and composite-action checks on both Linux architectures. Pull-request CI is offline. Trusted maintainer CI additionally uses the repository key for a small synthetic live fixture; it does not send the ActionJev codebase. Model-quality calibration and live Gitea deployment are separate from these checks.

See [architecture](docs/architecture.md), [distribution/release process](docs/distribution.md), [TypeSafe API](https://docs.typesafe.ai/api), [confidence semantics](https://docs.typesafe.ai/confidence), [Gitea action URLs](https://docs.gitea.com/usage/actions/comparison/) and [job-token permissions](https://docs.gitea.com/usage/actions/token-permissions/).

## License

[Apache-2.0](LICENSE).
