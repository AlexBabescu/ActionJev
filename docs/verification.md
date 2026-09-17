# Verification scope

The test suite is intentionally offline. Test credentials are non-secret placeholders sent only to loopback HTTP servers started by the test process.

## Automated checks

Rust unit tests cover diff line ranges/deletions, malformed hunk handling, secret-path exclusions, source chunk overlap, URL restrictions, retry-header parsing, typed-answer validation, probability validation, unknown Choice rejection, worker ordering/failure propagation, bundled policy parsing, Markdown escaping and API subpath preservation.

Python integration tests exercise the compiled binary against real temporary Git repositories and mocked Jev/GitHub/Gitea endpoints. They cover the full screen/locate/judge pipeline, token accounting, confidence gates, explicit `none`, dry runs, malformed API responses, overload/auth behavior, incomplete budgets, secret/symlink exclusion, ignoring untracked/worktree changes, unusual literal filenames, codebase mode, deletions, merge-base semantics, native comment routes, bot ownership, and stale-head suppression.

CI builds and tests without any TypeSafe key. The build artifact includes the binary, dependency lockfile and formatted source for reproducibility/inspection during initial development.

## Not established by these tests

- Live TypeSafe API compatibility against an authenticated account: the implementation follows the public HTTP specification, but no live key was available during implementation.
- Actual defect-detection precision/recall or empirical calibration of the starter thresholds.
- A production Gitea runner deployment, its particular image, private-action access policy, or token permissions.
- Every GitHub/Gitea server version, custom GitHub App bot identity, or private certificate authority.
- Exhaustive security against model prompt injection, compromised dependencies, Git vulnerabilities, or malicious runner configuration.

The first live deployment should begin with `fail-on: none`, a small known diff, restricted bot permissions, and manual inspection of the JSON report and PR summary. Keep ordinary compiler, test, lint and security checks; this action is an additional review signal, not their replacement.
