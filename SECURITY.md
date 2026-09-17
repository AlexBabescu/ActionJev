# Security

## Report a vulnerability

Use [GitHub private vulnerability reporting](https://github.com/AlexBabescu/ActionJev/security/advisories/new)
for suspected credential exposure, authorization bypasses, or other security
issues in ActionJev. Include the affected revision, reproduction steps, and
expected versus actual behavior. Do not include live keys or private source.
Avoid a public issue until maintainers have assessed the report.

If a credential has been exposed, revoke or rotate it at its provider. Removing
it from the latest commit or deleting a comment does not invalidate it.

## Data sent and stored

Eligible source and selected evidence go to the configured TypeSafe endpoint.
GitHub or Gitea receives the rendered comment when publication is enabled.
`report.json` contains source evidence. Traces omit source state and credentials
but include paths, questions, and answers, so review their contents before sharing.
Filename exclusions do not detect secrets embedded in ordinary source files.

## Public-repository controls

The following setup describes this repository's maintainer-approved workflow.
Consumers should adapt it to their own ownership and contribution model. The
[setup guide](docs/setup.md) distinguishes automatic same-repository reviews from
reviews requiring protected-environment approval.

## Required repository settings

These controls live in GitHub settings, not in Git. Merely naming an environment
in YAML does **not** create its protection rules. An automatically created
`jev-api` environment would initially have no rules.

### 1. Require approval for every outside contributor

Open **Settings → Actions → General**. Under **Approval for running fork pull
request workflows from contributors**, select **Require approval for all external
contributors** and save. Do not choose either first-time-contributor option:
a previously merged contributor would otherwise no longer need approval.

This applies to fork `pull_request` workflows. It is not a sandbox, does not
protect persistent self-hosted runners, and does not gate `pull_request_target`.
A workflow/job `if:` condition in a PR-editable YAML file is not a substitute
for this repository-level setting. Approving a fork workflow allows code
execution; it does not grant repository secrets to an ordinary public-fork
`pull_request` run.

### 2. Put the TypeSafe key in a protected environment, not a repository secret

Create two environments in **Settings → Environments**:

| Environment | Required reviewer | Secrets | Allowed deployment branch |
|---|---|---|---|
| `jev-approval` | `AlexBabescu` | None | Exactly `main`, no tags |
| `jev-api` | None | `TYPESAFE_API_KEY` | Exactly `main`, no tags |

For both, use **Selected branches and tags** with a **Branch** rule for `main`.
Disable administrator bypass. On `jev-approval`, leave **Prevent self-review**
off so the solo maintainer can approve runs they manually request.

The trusted request job reads the PR author's immutable GitHub account ID from
the API. PRs authored by `AlexBabescu`, ID `695992`, skip the approval job. Every
other author, including bots and missing author data, requires `jev-approval`.
The workflow actor, command sender, and branch name never grant this exception.
The model job can run only after request validation and either approval or the
explicit owner exemption. Rejected, failed, and cancelled approvals cannot run it.

Keep the TypeSafe key only in `jev-api`. Remove any same-named repository or
organization secret exposed to this repository. GitHub cannot reveal an existing
secret value; use your securely stored original or rotate it when replacing a
key. Never put keys in issues, PRs, or workflow files.

When migrating from the older single-environment setup, create and protect
`jev-approval` first. Merge the workflow that enforces this gate before removing
the required reviewer from `jev-api`. Cancel superseded runs using the old approval
layout before the change. Do not remove the main-only deployment rule.

The main release workflow always requires `jev-approval` before its live model
test, regardless of who pushed. Offline PR checks have no TypeSafe credential.

### 3. Keep default workflow permissions read-only

In **Settings → Actions → General → Workflow permissions**, select **Read
repository contents and packages permissions**. Leave **Allow GitHub Actions to
create and approve pull requests** unchecked. The reviewer only updates a PR
conversation comment; it does not open or approve PRs. Its job explicitly requests
`pull-requests: write`; publishing explicitly requests `contents: write`.

### 4. Protect the code that will be trusted after merge

Use a branch ruleset for `main` requiring pull requests, preventing force pushes
and deletion, and requiring the offline CI checks. Review changes to workflows,
`action.yml`, Rust sources, scripts, prompts, Cargo manifests and lockfiles before
merging. CODEOWNERS identifies the maintainer; it only enforces approval when the
appropriate branch rule is enabled. A solo maintainer cannot provide an approving
review on their own PR, so do not require a second person's approval unless one
is available.

Do not attach this public repository to persistent self-hosted runners containing
credentials, host Docker access or private-network access. The provided GitHub
workflows use fresh GitHub-hosted runners. Job-level secret injection is not
isolation from an earlier malicious process on the same persistent machine.

## Review a PR in this repository

Opening, updating, or reopening any PR automatically requests a review, including
forks, drafts, and PRs targeting other branches. Marking a PR ready for review or
changing its base branch also requests a new review. New requests supersede older
reviews for the same PR and require fresh approval for non-owner authors.

The separate `jev-auto.yml` workflow uses `pull_request_target` only to dispatch
`jev-review.yml` at `main`. It does not check out code, execute PR files, consume
artifacts, or receive the TypeSafe secret. Only the PR number reaches a fixed
workflow dispatch command. Review implementation and policy always come from
trusted `main`, even when the PR targets another branch.

For an owner-authored PR, the bot starts the model review automatically. For any
other author, it posts a run link where Alex approves `jev-approval`. Results
replace the same bot comment. Approval applies to the run; the review fetches the
current PR head after the gate and checks it again before publishing.

Maintainers can still comment `/jev review` as the whole comment to request a
rerun. The command workflow checks current admin or maintain permission through
GitHub's API. Requesting someone else's PR does not bypass its approval gate.
Failures or rejected approvals update the pending comment when cleanup can run.
A force-cancel can stop cleanup; the workflow run remains authoritative.

You can also open Actions, select Jev PR review, and run it on `main` with a PR
number, or use the CLI:

```sh
gh workflow run jev-review.yml --repo AlexBabescu/ActionJev --ref main -f pr-number=123
```

Select the optional `calibrate` input to check the real model against four small
rounding examples, including broken and corrected code with and without a
fixture label. Calibration makes additional API calls under the same author-based approval
policy. It must detect the broken examples and avoid correctness findings on the
corrected examples before continuing to the PR review. It does not establish
general review accuracy.

### Trust boundaries

- The credential-bearing workflow accepts only `workflow_dispatch` on `main`.
  Do not add `pull_request_target`, `issue_comment`, or `workflow_run` to it.
- The separate `issue_comment` workflow executes default-branch code and only
  dispatches the fixed main workflow after a current admin/maintain check.
- Both environments independently restrict access to `main`. The separate
  approval environment gates non-owner PRs and live release tests.
- Checkout uses the immutable workflow commit, `${{ github.sha }}`, in `trusted/`.
  The job builds this approved source with `cargo build --release --locked`.
  No TypeSafe credential is passed to the build step. Dependencies and build
  scripts are part of the trusted code that must be reviewed before merge.
- The PR is fetched from a fixed GitHub origin into a bare repository, without
  checking out or executing its files, hooks, submodules or build scripts.
- The action, binary and policy come from `trusted/`, never from the PR. The
  workflow fixes both API endpoints and passes credentials only to the steps
  that need them. A public Git fetch receives no inherited credentials.
- Model output is data, never a shell command. Findings are advisory and the
  bot checks that the PR head is still current before publishing a result.
- Ordinary PR CI has no TypeSafe key. This self-review source build does not
  change how consumers use prebuilt, checksum-verified release binaries.

### Customize the review

Edit `prompts/review.json` through a reviewed PR. The repository workflow passes
that file from its trusted checkout, so edits take effect after merge without
changing a release pin. Do not accept a policy path, endpoint or executable path
from a PR comment or from files in the PR being reviewed.

The policy defines guidance, review questions, defect mechanisms and severity.
Some follow-up questions remain in `src/review.rs`. Thresholds and budgets are
workflow inputs to the action. Review screening details before changing a
threshold; lowering it increases API calls and can increase false positives.

## Limits and residual risk

There is no setting that makes a secret safe after giving it to arbitrary code.
A repository owner/admin, a compromised trusted maintainer or a malicious change
merged into main can alter a privileged workflow or trusted binary. Approval and
log masking cannot prevent deliberately malicious code from exfiltrating a key.
Use a separate TypeSafe key for this repository, set provider-side spend and rate
limits where available, and revoke/rotate immediately after suspected exposure.

Repository publication alone does not publish Actions secrets, but logs, committed
files and build artifacts should be audited. Source snippets are sent to TypeSafe;
reports/comments may contain public PR code. Filename exclusions are not a secret
scanner. Do not automatically execute model recommendations.

For Gitea consumers, do not assume GitHub environment protections or fork approval
policies exist or work identically. Configure its own runner permissions and
isolation before accepting external contributions.

## Verification

`tests/workflow_security.py` checks metadata validation, fixed-endpoint requests,
redirect refusal, response limits, credential-free Git subprocess environments,
manual/main-only guards, protected-environment declarations, trusted-source
paths and secret-free ordinary CI. `tests/review_commands.py` checks command
authorization, fixed dispatch targets and bot-owned comment updates. A real temporary Git fixture verifies that PR files and
symlinks are stored as Git objects rather than checked out. These are regression
checks, not a proof of security or verification of the repository's UI settings.

```sh
uv run --with PyYAML==6.0.3 python tests/workflow_security.py
uv run python tests/review_commands.py
```

## References

- [GitHub Actions repository settings](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository)
- [Environment protection rules and secrets](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments)
- [Secure use of Actions](https://docs.github.com/en/actions/reference/security/secure-use)
- [Manually running a workflow](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)
- [Approving fork workflows](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/approve-runs-from-forks)

## PR checks and main releases

`ci.yml` runs the offline security checks and both native test/package jobs on
PRs. `main.yml` runs on main pushes or manual dispatch, calls those same checks
as a reusable workflow, then requests approval for its live test and release.
Release jobs therefore do not appear as skipped checks on PRs. Build artifacts
stay within the caller's run; no privileged workflow downloads a fork's artifacts.

See [GitHub event documentation](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request_target)
for the distinction between `pull_request` and `pull_request_target`.
