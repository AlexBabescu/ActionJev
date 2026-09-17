# Verification

[Documentation home](../README.md#documentation)

See the [CI history](https://github.com/AlexBabescu/ActionJev/actions) for results tied to exact commits. The current workflow tests native Linux x64 and ARM64, then gates release publication on a real TypeSafe call sequence against a tiny synthetic authorization-regression fixture.

## Offline checks

- `cargo test --locked --all-targets`: Rust unit tests.
- `cargo clippy --locked --all-targets -- -D warnings`.
- `cargo build --release --locked --target <native-musl-target>`.
- `uv run --no-project python tests/e2e.py <binary>`: Git and HTTP-contract integration tests, including screening-stage comment diagnostics.
- `uv run --no-project python tests/installer.py`: launcher tests, including checksum tampering, architecture routing, explicit source builds, Gitea outputs and BusyBox checksums. The BusyBox-specific case is skipped when BusyBox is not installed.
- `uv run --no-project --with PyYAML==6.0.3 python tests/workflow_security.py`: workflow trust boundaries.
- `uv run --no-project python tests/review_commands.py`: command authorization and comment ownership.
- Real composite-action execution with both bundled and explicitly source-built binaries; report outputs and lockfile immutability are checked.

The HTTP tests use local fake services, not real keys. They cover typed Jev responses, thresholds, uncertain findings, transient/auth failures, byte/file/follow-up budgets, merge-base and literal-path behavior, deleted files, secret/symlink exclusions, worktree isolation, GitHub/Gitea API routes, comment pagination/ownership and stale-head suppression. Mock tests do not establish provider interoperability or model accuracy.

## Live check

Only trusted non-PR runs receive the maintainer repository's `TYPESAFE_API_KEY`. The live job creates a separate temporary Git repository with a synthetic three-line authorization function and reviews one change using a one-category policy. It verifies Noul, Choice and Score responses through the compiled Rust reviewer and composite action. The fixture code is never executed. Credentials are not printed or uploaded, and the ActionJev codebase is not sent by this test.

## Review quality calibration

The manual PR workflow accepts `calibrate: true`. It runs
`scripts/calibrate-review.py` with the approved binary and policy, behind the
`jev-api` approval gate. Four synthetic inputs compare broken and corrected
rounding with and without an intentionally-flawed label. The job summary records
correctness scores and whether an actionable finding matches the expected result.
Fixtures are never executed and their source and API credentials are not logged.

The [recorded calibration run](https://github.com/AlexBabescu/ActionJev/actions/runs/35241086870)
on 17 September 2026 failed. The revised policy produced uncertain assessments
for both broken rounding examples, but neither became actionable. The corrected
examples produced no findings. This shows that the revised policy noticed
suspicious code; it does not establish reliable defect detection.

A [later PR review](https://github.com/AlexBabescu/ActionJev/actions/runs/35242792054)
completed with seven model calls and three uncertain category assessments of the
same calculation. Workflow success means the pipeline completed, not that it
correctly classified the deliberate defect. Run calibration again after model,
policy, or threshold changes. These small fixtures do not measure general review
accuracy.

## Release checks

`dist/SOURCE_COMMIT` records the tested source commit; the distribution tag has its own commit containing compressed static binaries and `SHA256SUMS`. The default launcher checks its selected binary archive against this committed manifest before execution. CI also imports the published `AlexBabescu/ActionJev@v0` action as a separate downstream job without installing Rust.

Model-quality calibration, adversarial prompt robustness and a deployment on a live Gitea runner remain separate validation tasks. The action does not claim that distribution confidence equals measured bug-detection accuracy.
