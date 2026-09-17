# Read a review

[Documentation home](../README.md#documentation)

A review is a set of model assessments of committed code. It is not a test run,
a compiler result, or proof that the patch is safe. Start with the reviewed commit
and coverage status, then inspect the linked code for each concern.

## What Jev can tell you

Jev returns structured answers. ActionJev supplies category questions, candidate
code regions, mechanism choices, and impact levels. Jev selects or scores those
options. Rust renders the answers into tables and applies the decision thresholds.

A label such as `incorrect_calculation` names a suspected mechanism. It does not
explain the exact input that fails or provide a proposed fix. The reviewer cannot
generate that prose. Use the selected code region as a starting point for human
investigation, and verify a suspected defect with a concrete example or test.

Several categories can select the same region and mechanism. They are separate
assessments and do not establish that several distinct bugs exist. The comment groups matching file regions and mechanisms into one concern; the
JSON report retains all category-level records. A group is actionable when at
least one of its individual assessments meets every threshold.

## Scores

| Value | How to read it |
|---|---|
| Screening | Whether a file/category pair should receive follow-up. Higher means more suspicion, not better code quality. |
| Support | Whether the focused evidence supports the category's concern. |
| Confidence | The minimum confidence reported for evidence selection, mechanism selection, and impact scoring. |
| Impact | A model score on the configured severity scale, 0 through 4 by default. |

Screening bars are not test coverage, a project health grade, or a measured bug
probability. Decimal precision does not establish accuracy. Confidence describes
the returned answer distributions, not agreement between independent reviewers.

An assessment is actionable only when support, confidence, and impact all pass
their thresholds. Other reported assessments remain uncertain. Neither status
removes the need to inspect the evidence.

## Why one file can use seven API calls

| Stage | Requests in this example |
|---|---:|
| Screen one file across all five categories | 1 |
| Locate evidence for three categories above the threshold | 3 |
| Judge the three selected regions | 3 |
| Total | 7 |

In general, model calls equal files screened, plus categories followed up, plus
selected regions judged. If location returns `none`, that category has no judge
request. If no category passes screening, the file uses only one request.

`api_calls` counts completed logical model requests; `http_attempts` separately
counts transport attempts, including retries. GitHub/Gitea requests to read or
update a PR comment are not included in these model counts. Calibration runs
make additional model calls outside the PR report.

## No findings, incomplete reviews, and failures

| Result | Meaning and next step |
|---|---|
| No categories pass screening | No detailed follow-up occurred. Inspect scores before drawing conclusions. |
| Follow-up produces no finding | Evidence selection or mechanism selection returned `none`. |
| Uncertain findings | The model reported a mechanism but did not meet all actionability thresholds. Inspect the code. |
| Incomplete | A file, byte, region, or follow-up budget was reached. Inspect exclusions and limits. |
| Failed workflow | Check the run for approval rejection, credentials, transport, or configuration errors. A missing report is not a clean review. |
| Dry run | Only Git scope and exclusions were checked. No model review occurred. |

## Common questions

### Why is the comment authored by my account?

The author comes from the token used to publish it. The built-in GitHub job token
publishes as `github-actions[bot]`. A local `gh` command or personal token posts
as its authenticated user. Keep the automatic token for bot-authored workflow
comments; changing the Markdown cannot change its author.

### Why does the bot still show a pending review?

Open the run link. In this repository reviews of non-owner PRs wait for Alex to approve
`jev-approval`. Owner-authored PRs start automatically.
Normal failures update the pending comment, but force-cancelling a workflow can
also stop its cleanup job. The workflow run is the authoritative status.

### Why did a known bug remain uncertain?

The pipeline can work correctly while the model misses a defect or scores it
below the thresholds. See the recorded [calibration outcome](verification.md#review-quality-calibration).
Changing the formatting does not improve detection. Keep a mix of known broken
and correct cases when evaluating a new model, policy, or threshold.
