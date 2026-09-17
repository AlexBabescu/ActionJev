# Setup

[Documentation home](../README.md#documentation)

## Choose a workflow

The automatic examples below are for repositories where contributors with branch
write access are trusted to change workflows. They skip fork PRs. A condition in
PR-editable YAML is not an approval boundary for someone who can modify that YAML.

For public contributions, use the maintainer-approved pattern in
[SECURITY.md](../SECURITY.md): dispatch a workflow from protected `main`, approve
the environment, and read PR files as Git objects without executing them. This
repository's workflow is specific to ActionJev and must be adapted before reuse.

## GitHub

Add a Actions repository secret named `TYPESAFE_API_KEY`, then create `.github/workflows/jev-review.yml`:

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
    # This example reviews branches in the same repository only.
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

The automatically supplied `${{ github.token }}` is used for PR comments; do not create a PAT. An imported action cannot automatically read arbitrary repository secrets, so the explicit `with:` mapping is required. Each caller supplies its own TypeSafe key; the publisher's repository secret is never bundled or shared.

Start with the default `fail-on: none`. Add `fail-on: high` only after evaluating the reviewer on known changes. Use a release's **distribution commit SHA** instead of `@v0` for immutable action code, prompts and binary digests. `@v0.1.0` is the first release; `@v0` tracks the published major version. For production, also pin checkout to a reviewed SHA.

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

If you mirror ActionJev to your Gitea instance, mirror version tags and their
reachable objects, not only `main`. The packaged binaries live in distribution
tags. Update the action URL to your mirror. See [distribution](distribution.md)
for repository access and release details.

Runtime requirements: Linux x64/ARM64, Bash, Git, gzip, awk and sha256sum. A separate checkout action may require Node in a Gitea job image. ActionJev itself needs no Node/Python runtime, Cargo, Docker-in-Docker, Docker socket or privileged mode.

## Check the setup

Start with `dry-run: 'true'` to inspect selected files and exclusions without
model calls or a PR comment. Then remove it and review a small known change with
`fail-on: none`. Confirm the reviewed commit, coverage status, and bot author.

If the job cannot write a comment, check the job token's permissions or use
`comment: 'false'` to keep the report in the job summary. A successful dry run
checks Git scope, not API credentials or model accuracy.

See [configuration](configuration.md) for report-only and full-codebase scans,
and [review results](review-results.md) for scores and request counts.
