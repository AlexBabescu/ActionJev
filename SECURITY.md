# Public-repository CI and secret protection

## Required repository settings — apply these before merging the workflow changes

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

Both the existing live synthetic integration test and the new manual PR reviewer
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

After the workflow is merged and the settings above are configured:

1. Open **Actions → Jev PR review (manual) → Run workflow**.
2. Select **main**, enter an open PR number targeting main, and run it.
3. Approve the pending **jev-api** environment deployment after checking the
   workflow revision. The job reviews the PR and creates/updates its summary.

Or use an authenticated GitHub CLI with repository write access:

```sh
gh workflow run jev-review.yml --repo AlexBabescu/ActionJev --ref main -f pr-number=123
```

External PRs do not trigger this live workflow. It can deliberately review a
fork PR: only Git objects are fetched, not a working tree. It will refuse a
closed PR, a non-main target, invalid identifiers or a head changed during fetch.
The action rechecks the PR head before publishing its summary. A PR can still
change immediately after that check; every report records the reviewed SHA.

### Trust boundaries

- The only trigger is `workflow_dispatch`, which requires repository write access.
- The workflow job permits only this repository and the `main` execution ref.
  The independently configured environment branch rule is the enforcement layer.
- The preparation script is read from the trusted workflow commit, never a PR.
- The PR is fetched from a fixed public GitHub origin into a fresh **bare Git
  repository**, without checkout, hooks, submodules, test execution or builds.
- The reviewer uses the SHA-pinned, packaged v0.1.0 action commit
  `9ec4a7cd1fc7c879fc566d92ec5c889dfbc57b60`, not `./` from a PR or a mutable tag.
- No untrusted binary, artifact, cache, endpoint, shell command or policy is loaded.
- The TypeSafe key is passed only to the pinned review action, not the preparation
  script. A public Git fetch receives no inherited workflow credentials.
- Model output is data, never a shell command. Only fixed API endpoints are used.
- `fail-on: none` makes model findings advisory. Transport failures and incomplete
  reviews can still fail the job. The report states which commit was reviewed.

The Rust reviewer is unchanged. The Python standard-library preparation helper is
specific to this repository's GitHub-hosted workflow; it is not a new dependency
of the reusable Rust action or of Gitea consumers.

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
manual/main-only guards, protected-environment declarations, SHA pinning and
secret-free ordinary CI. A real temporary Git fixture verifies that PR files and
symlinks are stored as Git objects rather than checked out. These are regression
checks, not a proof of security or verification of the repository's UI settings.

```sh
python3 -m venv .venv-workflow-checks
.venv-workflow-checks/bin/pip install 'PyYAML==6.0.3'
.venv-workflow-checks/bin/python tests/workflow_security.py
```

## References

- [GitHub Actions repository settings](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository)
- [Environment protection rules and secrets](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments)
- [Secure use of Actions](https://docs.github.com/en/actions/reference/security/secure-use)
- [Manually running a workflow](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)
- [Approving fork workflows](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/approve-runs-from-forks)
