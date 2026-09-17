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

The automatically supplied `${{ github.token }}` is used for PR comments; do not create a PAT. An imported action cannot automatically read arbitrary repository secrets, so the explicit `with:` mapping is required. Each caller supplies and pays for its own TypeSafe key; the publisher's repository secret is never bundled or shared.

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

Jev returns typed judgments, not generated review prose. Findings are **review candidates, not proven defects**. Confidence describes the model's answer distribution, not an empirically calibrated probability that a bug exists.

PR scope is `merge-base(base, head) -> head`, independent of GitHub's synthetic merge checkout. Both histories must be available: use `fetch-depth: 0`. Full-codebase mode reviews eligible tracked source at the selected commit. Working-tree edits, untracked files and symlink targets are not reviewed, and project code is never executed.

Categories include correctness, security, reliability, compatibility and demonstrably flawed supplied tests. Common Rust, Python, JavaScript/TypeScript, Go, Java/Kotlin, C/C++, Swift, shell, SQL, Terraform and configuration files are supported. There is no dashboard, generated prose, patch generation, automatic approval, inline review threads or compiler execution.

## Configuration and outputs

[action.yml](action.yml) documents every input and output. Important defaults:

| Setting | Default |
|---|---|
| Review mode / model | `changes` / `jev-latest` |
| Simultaneous requests | `concurrency: 4` |
| File / byte limits | 50 files, 48000 bytes per file, 1000000 total bytes |
| Evidence / follow-up limits | 32 regions per file, 24 follow-ups |
| Screening / evidence / confidence thresholds | 0.65 / 0.8 / 0.7 |
| Severity gate | `fail-on: none` |
| PR summary | `comment: true` |
| Incomplete scans allowed | `allow-incomplete: false` |

Override `path`, `base`, `head`, `platform`, `api-url` and `pr-number` as needed. `mode: codebase` requires `comment: 'false'`. `dry-run: 'true'` inspects scope without API calls. `exclude` accepts newline-separated globs in addition to non-disableable built-in secret exclusions.

Only actionable findings trigger a severity gate. Uncertain findings are reported but not gated. Budget-limited scans remain marked incomplete; API failures are errors, not clean reviews. The editable [policy](prompts/review.json) contains guidance, questions, mechanism choices and severity levels. Reports record its SHA-256. Custom policies must be explicitly supplied as trusted inputs.

Outputs `report-json`, `report-markdown` and `trace-jsonl` are absolute paths in a fresh runner-temporary directory. `findings` counts actionable candidates; `complete` and `gate-failed` expose status. Markdown is also appended to the job summary. PR comments are author-checked before update, paginated, and suppressed when the PR head has moved.

`report.json` contains source evidence and should be protected like the repository. `trace.jsonl` contains questions, typed answers, timing and usage, not credentials or source state. Upload reports only under an appropriate artifact access/retention policy; artifact upload is deliberately not a dependency of the action.

## Local/source use

Source branches, including `main`, do not contain generated binary blobs. Use a published tag/distribution SHA for normal imports. Explicit `build-from-source: 'true'` requires an installed Rust toolchain. A preinstalled absolute `binary-path` overrides the bundled executable.

```bash
cargo build --release --locked
export TYPESAFE_API_KEY='your-key'
./target/release/actionjev --repo /path/to/repo --base origin/main --head HEAD
./target/release/actionjev --repo /path/to/repo --mode codebase --head HEAD
```

Secrets are read from environment variables, not CLI arguments. Exit status is `0` for success, `1` for runtime/configuration errors, `2` for incomplete scans unless allowed, and `3` for a severity gate. CLI argument parsing can also use exit status `2`.

## Security and verification

Eligible source is sent to TypeSafe. Common secret filenames and symlinks are excluded, but filename exclusions are **not a secret scanner**. Review provider data handling before using private code. Reports with snippets remain sensitive.

Do not use `pull_request_target` to run an untrusted PR's workflow, build scripts, action implementation or policy with secrets. Keep the action pinned and the workflow trusted; do not run arbitrary project code in the credential-bearing job. Custom endpoints/policies are trusted inputs. Credentials are never committed or included in release assets.

CI covers 15 Rust unit tests, 23 offline Git/HTTP tests, 13 launcher tests (BusyBox-specific coverage skips when unavailable), Clippy, native static builds and composite checks on both Linux architectures. PR CI is offline. Trusted maintainer CI uses the repository key only for a tiny synthetic live fixture, not the ActionJev codebase. Model-quality calibration and a live Gitea deployment are separate checks.

See [verification](docs/verification.md), [architecture](docs/architecture.md), [distribution/releases](docs/distribution.md), [TypeSafe API](https://docs.typesafe.ai/api), [confidence semantics](https://docs.typesafe.ai/confidence), [Gitea action URLs](https://docs.gitea.com/usage/actions/comparison/) and [job-token permissions](https://docs.gitea.com/usage/actions/token-permissions/).

## License

[Apache-2.0](LICENSE).
