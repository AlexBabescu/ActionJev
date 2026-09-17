# Contributing

ActionJev accepts focused fixes, documentation improvements, and reproducible
bug reports. Use [GitHub issues](https://github.com/AlexBabescu/ActionJev/issues)
for public bugs and proposals. For vulnerabilities or exposed credentials, follow
[SECURITY.md](SECURITY.md#report-a-vulnerability).

## Development setup

You need Git and a Rust toolchain with Clippy. Python test scripts use the standard
library except the workflow-security suite, which also needs PyYAML. The commands
below use `uv` for Python dependencies. You do not need a TypeSafe key for offline
development or PR checks.

```sh
git clone https://github.com/AlexBabescu/ActionJev.git
cd ActionJev
cargo build --locked
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
uv run --no-project python tests/e2e.py target/debug/actionjev
uv run --no-project python tests/installer.py
uv run --no-project --with PyYAML==6.0.3 python tests/workflow_security.py
uv run --no-project python tests/review_commands.py
```

Run the checks relevant to your change. Installer tests use shell utilities and
skip the BusyBox-specific case if BusyBox is unavailable. CI builds and checks
both supported Linux architectures. See [verification](docs/verification.md) for
the distinction between offline checks, live integration, and model calibration.

## Make a change

1. Create a branch from `main` and keep the change focused on one problem.
2. Follow the surrounding code style. Add a focused test when behavior changes.
3. Update the relevant documentation when changing inputs, output semantics,
   review policy, or trust boundaries.
4. Open a PR with the problem, resulting behavior, and checks you ran. Include
   limitations or tests you could not run.

Do not commit keys, source-containing review reports, generated distribution
binaries, or local output directories. Release automation creates packaged
binaries separately. Keep dependencies and lockfile updates deliberate.

Changes to prompts, dependencies, workflow permissions, endpoints, credential
handling, and code executed with secrets need particular care. PR CI is offline;
maintainers approve any live model run after checking the trusted revision.

## Report a bug

Include the source revision or release, runner OS and architecture, sanitized
configuration, expected behavior, actual behavior, and a minimal reproduction.
For a model-quality report, include the model ID and policy hash when available,
and explain the concrete input or contract that demonstrates the issue.

Remove secrets and private code before sharing logs or reports. A failed
integration test and a missed defect are different problems; specify which one
you observed. Do not attach raw `report.json` files from private repositories.

## Project structure

The [architecture guide](docs/architecture.md) maps the Rust modules and review
stages. Composite-action inputs live in [action.yml](action.yml), the policy in
[prompts/review.json](prompts/review.json), and runtime adapters in `scripts/`.
See [distribution](docs/distribution.md) before proposing release changes.

Contributions are covered by the project's [Apache-2.0 license](LICENSE).
