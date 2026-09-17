# Public-repository CI and secret protection

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

Open **Settings → Environments → New environment** and create **`jev-api`**.

- Set **Required reviewers** to `AlexBabescu` (or another trusted maintainer).
- Set **Deployment branches and tags → Selected branches and tags**. Add a
  **Branch** rule for exactly **`main`**. Do not add tags or `refs/pull/*`.
- Deselect **Allow administrators to bypass configured protection rules**.
- For a solo maintainer, leave **Prevent self-review** off so you can approve
  a manually dispatched run you started. Turn it on only with a second reviewer.
- Add **`TYPESAFE_API_KEY`** under **Environment secrets**.

Then delete the same-named **repository** secret under **Settings → Secrets and
variables → Actions** and remove any organization-level copy exposed to this
repository. Do not keep a fallback outside the protected environment. A workflow
without the protected environment must not be able to read this credential.
GitHub does not reveal existing secret values; use the original value you stored
securely or issue a replacement key in TypeSafe. Never commit or paste the key
into an issue, pull request or workflow.

Both the live synthetic integration test and the PR reviewer
reference `jev-api`. A main-branch release now waits for approval of the live job,
since publishing already depends on that job. PR test/package jobs do not use
this environment and receive no TypeSafe credential.

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

A maintainer with the repository admin or maintain role can comment `/jev review`
on an open PR targeting `main`. The command must be the whole comment. Edited
comments and commands from other users do not start a review.

The command workflow checks the author's current repository permission through
GitHub's API and dispatches `jev-review.yml` at `main`. It has no environment or
TypeSafe secret. It only needs read access to code and write access to Actions.

The review workflow posts a comment with a link to its run. Open that link and
approve `jev-api` after inspecting the workflow revision. The bot updates the
same comment with its findings, screening scores and reviewed commit. If a review
fails or approval is rejected, a separate job without the TypeSafe key updates
the comment. Force-cancelling a workflow can also stop that cleanup job; use the
run link to check the authoritative status.

You can also open Actions, select Jev PR review, and run it on `main` with a PR
number, or use the CLI:

```sh
gh workflow run jev-review.yml --repo AlexBabescu/ActionJev --ref main -f pr-number=123
```

Select the optional `calibrate` input to check the real model against four small
rounding examples, including broken and corrected code with and without a
fixture label. Calibration makes additional API calls behind the same approval
gate. It must detect the broken examples and avoid correctness findings on the
corrected examples before continuing to the PR review. It does not establish
general review accuracy.

### Trust boundaries

- The credential-bearing workflow accepts only `workflow_dispatch` on `main`.
  Do not add `pull_request_target`, `issue_comment`, or `workflow_run` to it.
- The separate `issue_comment` workflow executes default-branch code and only
  dispatches the fixed main workflow after a current admin/maintain check.
- The environment independently restricts access to `main` and requires approval.
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
Use a separate TypeSafe key for this repository, set provider-side spend/rate
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
