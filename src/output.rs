use crate::{config::{env_first, Args, CiContext, Platform}, http::{checked_url, Http}, review::{Finding, Report}};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs::{self, OpenOptions}, io::Write, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

pub const MARKER: &str = "<!-- actionjev:review:v1 -->";
fn escape(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"),
            '@' => out.push_str("@\u{200b}"), '\n' | '\r' => out.push(' '),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '!' | '|' => { out.push('\\'); out.push(c); }
            c if c.is_control() => out.push(' '), c => out.push(c),
        }
    }
    out
}
fn github_repository() -> Option<String> {
    let repo = std::env::var("GITHUB_REPOSITORY").ok()?;
    (std::env::var("GITHUB_SERVER_URL").as_deref() == Ok("https://github.com")
        && repo.split('/').count() == 2 && repo.split('/').all(|part| !part.is_empty())
        && repo.bytes().all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))).then_some(repo)
}
fn evidence_link(repo: Option<&str>, commit: Option<&str>, path: &str, start: usize, end: usize) -> Option<String> {
    let mut url = reqwest::Url::parse("https://github.com").ok()?;
    let repo = repo?;
    let commit = commit?;
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) { return None; }
    url.path_segments_mut().ok()?.extend(repo.split('/')).push("blob").push(commit).extend(path.split('/'));
    url.set_fragment(Some(&format!("L{start}-L{end}")));
    // Markdown link destinations must not contain literal parentheses.
    Some(url.to_string().replace('(', "%28").replace(')', "%29").replace('|', "%7C"))
}
fn score_bar(score: f64) -> String {
    let filled = (score.clamp(0.0, 1.0) * 10.0).round() as usize;
    format!("`{}{}`", "█".repeat(filled), "░".repeat(10 - filled))
}
// Group matching regions and mechanisms for display only. Preserve every assessment
// in report.json and keep gate decisions based on individual assessments.
fn concern_groups(report: &Report) -> Vec<Vec<&Finding>> {
    let mut groups: Vec<Vec<&Finding>> = Vec::new();
    for finding in &report.findings {
        let existing = groups.iter_mut().find(|group| {
            let first = group[0];
            first.path == finding.path && first.mechanism == finding.mechanism
                && first.evidence.old_start == finding.evidence.old_start
                && first.evidence.old_lines == finding.evidence.old_lines
                && first.evidence.new_start == finding.evidence.new_start
                && first.evidence.new_lines == finding.evidence.new_lines
        });
        if let Some(group) = existing { group.push(finding); } else { groups.push(vec![finding]); }
    }
    groups
}
pub fn markdown(report: &Report) -> String {
    let groups = concern_groups(report);
    let actionable = groups.iter().filter(|group| group.iter().any(|f| f.actionable)).count();
    let status = if report.dry_run { "Dry run. No model review performed." } else if !report.complete { "Review incomplete. A budget or coverage limit was reached." } else if report.scanned_files == 0 { "No eligible files to review." } else if report.actionable_count() > 0 { "Review complete. Potential issues need your attention." } else if !report.findings.is_empty() { "Review complete. Potential concerns need verification." } else { "Review complete. No issues reported within this scope." };
    let repo = github_repository();
    let mut text = format!("{MARKER}\n## ActionJev review\n\n**{status}**\n\n| Files reviewed | Actionable concerns | Needs verification | Files skipped |\n|---:|---:|---:|---:|\n| {} | {} | {} | {} |\n\nReviewed commit: `{}`\n\n", report.scanned_files, actionable, groups.len() - actionable, report.skipped.len(), report.head);
    if let (Some(repo), Ok(run)) = (repo.as_deref(), std::env::var("GITHUB_RUN_ID")) {
        if !run.is_empty() && run.bytes().all(|b| b.is_ascii_digit()) {
            text.push_str(&format!("[View workflow run](https://github.com/{repo}/actions/runs/{run})\n\n"));
        }
    }
    if !report.findings.is_empty() {
        text.push_str("### Potential concerns\n\nJev selects predefined categories and code regions. It does not generate a written diagnosis or a failing example. Matching regions and mechanisms are grouped below; a group is not a confirmed bug.\n\n| Location | Suspected mechanism | Categories | Status |\n|---|---|---|---|\n");
        for group in &groups {
            let f = group[0];
            let (side, start, lines) = if f.evidence.new_lines > 0 { ("new", f.evidence.new_start, f.evidence.new_lines) } else { ("old", f.evidence.old_start, f.evidence.old_lines) };
            let end = start.saturating_add(lines.saturating_sub(1));
            let label = format!("{}, {side} lines {start} to {end}", escape(&f.path));
            let commit = if side == "new" { Some(report.head.as_str()) } else { report.merge_base.as_deref() };
            let location = evidence_link(repo.as_deref(), commit, &f.path, start, end).map_or_else(|| label.clone(), |url| format!("[{label}]({url})"));
            let categories = group.iter().map(|f| escape(&f.dimension)).collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", ");
            let row = format!("| {} | {} | {} | {} |\n", location, escape(&f.mechanism.replace('_', " ")), categories, if group.iter().any(|f| f.actionable) { "Actionable" } else { "Needs verification" });
            if text.len() + row.len() > 48_000 { text.push_str("\nAdditional findings omitted from this comment; see report.json.\n"); break; }
            text.push_str(&row);
        }
    } else if !report.dry_run && report.scanned_files > 0 {
        if report.followed_signals == 0 {
            text.push_str("No potential issues passed screening. The reviewer did not perform evidence selection or a detailed follow-up.\n");
        } else {
            text.push_str("The reviewer investigated potential issues but did not establish a defect mechanism from the supplied evidence.\n");
        }
    }
    if !report.signals.is_empty() {
        let mut categories = BTreeMap::<&str, f64>::new();
        for signal in &report.signals {
            categories.entry(&signal.dimension).and_modify(|score| *score = score.max(signal.probability)).or_insert(signal.probability);
        }
        text.push_str("\n<details>\n<summary>Screening scores and API usage</summary>\n\nHighest model score in each category. Bars show screening scores, not verified bugs or measured defect probabilities.\n\n| Category | Signal | Score / 1 |\n|---|---|---:|\n");
        let mut categories: Vec<_> = categories.into_iter().collect();
        categories.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
        for (category, score) in categories {
            let row = format!("| {} | {} | {:.3} |\n", escape(category), score_bar(score), score);
            if text.len() + row.len() > 52_000 { text.push_str("\nAdditional categories omitted.\n"); break; }
            text.push_str(&row);
        }
        text.push_str(&format!("\nFollow-up threshold: **{:.3}**. Category assessments followed up: {}. Model API calls: {}.\n", report.thresholds["screen"].as_f64().unwrap_or_default(), report.followed_signals, report.api_calls));
        text.push_str(&format!("\nScores at or above {:.3} qualify for follow-up, subject to the review budget. These are model scores, not measured bug probabilities.\n\n| File | Category | Score |\n|---|---|---:|\n", report.thresholds["screen"].as_f64().unwrap_or_default()));
        for s in &report.signals {
            let row = format!("| {} | {} | {:.3} |\n", escape(&s.path), escape(&s.dimension), s.probability);
            if text.len() + row.len() > 54_000 { text.push_str("\nAdditional scores omitted.\n"); break; }
            text.push_str(&row);
        }
        text.push_str(&format!("\nPolicy SHA-256: `{}`\n\n</details>\n", report.policy_sha256));
    }
    if !report.findings.is_empty() {
        text.push_str("\n<details>\n<summary>Category assessments and thresholds</summary>\n\n| File | Category | Mechanism | Impact / 4 | Support | Confidence |\n|---|---|---|---:|---:|---:|\n");
        for f in &report.findings {
            let row = format!("| {} | {} | {} | {:.2} | {:.3} | {:.3} |\n", escape(&f.path), escape(&f.dimension), escape(&f.mechanism.replace('_', " ")), f.severity, f.evidence_probability, f.confidence);
            if text.len() + row.len() > 59_000 { text.push_str("\nAdditional scores omitted.\n"); break; }
            text.push_str(&row);
        }
        text.push_str(&format!("\nAn actionable assessment requires support of at least {:.3}, confidence of at least {:.3}, and impact of at least 1.000. A group is actionable if at least one assessment meets all thresholds. Other groups need verification.\n\n</details>\n", report.thresholds["evidence"].as_f64().unwrap_or_default(), report.thresholds["confidence"].as_f64().unwrap_or_default()));
    }
    text.push_str("\nReview findings need human verification. A review with no findings does not prove the code is correct.\n");
    text
}
fn create(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path).context("cannot create report file; use a new output directory")?;
    f.write_all(bytes)?; Ok(())
}
pub fn write_reports(args: &Args, report: &Report, traces: &[Value]) -> Result<PathBuf> {
    let directory = args.output_dir.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("actionjev-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos())));
    if directory.exists() { ensure!(!fs::symlink_metadata(&directory)?.file_type().is_symlink(), "output directory must not be a symlink"); } else { fs::create_dir_all(&directory)?; }
    let directory = fs::canonicalize(directory)?;
    create(&directory.join("report.json"), &serde_json::to_vec_pretty(report)?)?;
    let md = markdown(report);
    create(&directory.join("report.md"), md.as_bytes())?;
    let mut trace_data = Vec::new();
    for trace in traces { serde_json::to_writer(&mut trace_data, trace)?; trace_data.push(b'\n'); }
    create(&directory.join("trace.jsonl"), &trace_data)?;
    if let Some(path) = env_first(&["GITEA_STEP_SUMMARY", "GITHUB_STEP_SUMMARY"]) { OpenOptions::new().append(true).create(true).open(path)?.write_all(md.as_bytes())?; }
    if let Some(path) = env_first(&["GITEA_OUTPUT", "GITHUB_OUTPUT"]) {
        let mut f = OpenOptions::new().append(true).create(true).open(path)?;
        for (key, value) in [
            ("report-json", directory.join("report.json").display().to_string()),
            ("report-markdown", directory.join("report.md").display().to_string()),
            ("trace-jsonl", directory.join("trace.jsonl").display().to_string()),
            ("findings", report.actionable_count().to_string()),
            ("complete", report.complete.to_string()),
            ("gate-failed", report.gate_failed(args).to_string()),
        ] {
            ensure!(!value.contains(['\n', '\r']), "output value contains newline");
            writeln!(f, "{key}={value}")?;
        }
    }
    Ok(directory)
}
fn api_path(base: &str, segments: &[&str], allow_http: bool) -> Result<String> {
    let mut url = checked_url(base, allow_http)?;
    { let mut p = url.path_segments_mut().map_err(|_| anyhow::anyhow!("invalid API base URL"))?; p.pop_if_empty(); for s in segments { p.push(s); } }
    Ok(url.into())
}
pub fn comment(args: &Args, ci: &CiContext, report: &Report) -> Result<bool> {
    let token = env_first(&["ACTIONJEV_TOKEN", "GITEA_TOKEN", "GITHUB_TOKEN"]).context("comment requested but ACTIONJEV_TOKEN is missing")?;
    let base = ci.api_url.as_deref().context("missing API URL")?;
    let repo = ci.repository.as_deref().context("missing repository")?;
    let (owner, name) = repo.split_once('/').context("invalid repository")?;
    let number = ci.pr_number.context("missing PR number")?.to_string();
    let http = Http::new(args.timeout_seconds, args.allow_insecure_http)?;
    let scheme = if ci.platform == Platform::Gitea { "token" } else { "Bearer" };
    let url = |segments: &[&str]| api_path(base, segments, args.allow_insecure_http);
    let pr_url = url(&["repos", owner, name, "pulls", &number])?;
    let latest = http.json(Method::GET, &pr_url, &token, scheme, None, true)?;
    if latest.pointer("/head/sha").and_then(Value::as_str) != Some(report.head.as_str()) { return Ok(false); }
    let identity = match http.json(Method::GET, &url(&["user"])? , &token, scheme, None, false) {
        Ok(user) => user.get("login").and_then(Value::as_str).map(str::to_owned),
        // GitHub's built-in installation token cannot call GET /user.
        Err(_) if ci.platform == Platform::Github => Some("github-actions[bot]".into()),
        Err(_) => None,
    };
    // Without an authenticated identity, do not edit a comment on the strength of a marker alone.
    ensure!(identity.is_some(), "cannot identify comment author; use a bot token permitted to read its own user");
    let comments_url = url(&["repos", owner, name, "issues", &number, "comments"])?;
    let mut existing = None;
    for page in 1..=20 {
        let comments = http.page(&comments_url, &token, scheme, page, ci.platform == Platform::Gitea)?;
        let entries = comments.as_array().context("invalid comments response")?;
        for entry in entries {
            if identity.as_deref().is_some_and(|id| entry.pointer("/user/login").and_then(Value::as_str) == Some(id)) && entry.get("body").and_then(Value::as_str).is_some_and(|body| body.starts_with(MARKER)) {
                existing = Some(entry.get("id").and_then(Value::as_u64).context("invalid comment ID")?); break;
            }
        }
        if existing.is_some() || entries.is_empty() { break; }
        ensure!(page < 20, "comment pagination limit reached; refusing to create a possible duplicate");
    }
    // Recheck after pagination. The API has no atomic compare-head-and-comment operation;
    // serialize review jobs per PR to avoid concurrent create/update races.
    let latest = http.json(Method::GET, &pr_url, &token, scheme, None, true)?;
    if latest.pointer("/head/sha").and_then(Value::as_str) != Some(report.head.as_str()) { return Ok(false); }
    let body = json!({"body":markdown(report)});
    if let Some(id) = existing {
        http.json(Method::PATCH, &url(&["repos", owner, name, "issues", "comments", &id.to_string()])?, &token, scheme, Some(&body), true)?;
    } else {
        http.json(Method::POST, &comments_url, &token, scheme, Some(&body), false)?;
    }
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn markdown_escapes_untrusted_filenames() { let s = escape("<script>@team [x](url)|\n"); assert!(!s.contains("<script>")); assert!(!s.contains("@team")); assert!(!s.contains('\n')); }
    #[test] fn api_subpath_preserved() { assert_eq!(api_path("https://git.example/gitea/api/v1/", &["repos","a","b"], false).unwrap(), "https://git.example/gitea/api/v1/repos/a/b"); }
    #[test] fn evidence_links_preserve_markdown_boundaries() {
        let url = evidence_link(Some("a/b"), Some(&"a".repeat(40)), "src/a (b)|.rs", 2, 4).unwrap();
        assert!(url.ends_with("src/a%20%28b%29%7C.rs#L2-L4"));
        assert!(evidence_link(Some("a/b"), Some("main"), "file.rs", 1, 1).is_none());
    }
    #[test] fn uncertain_calibration_result_is_not_presented_as_clean() {
        use crate::{config::Mode, git::Region, review::{Finding, Signal}};
        // Recorded neutral-broken calibration result from run 35241086870.
        let mut report = Report {
            schema_version: 1, tool_version: "0.1.1".into(), mode: Mode::Changes,
            model_requested: "jev-latest".into(), policy_sha256: "b90c4833ab528ff95f266d2cc40607912366f03e7fc7a5c7db67ed7028913422".into(),
            head: "cbb3dc1cfa6f563d9d4990d5a324081e5d5dfc77".into(), merge_base: None,
            complete: true, dry_run: false, scanned_files: 1, skipped: vec![],
            signals: [("correctness", 0.73), ("security", 0.03), ("reliability", 0.04), ("compatibility", 0.60), ("tests", 0.22)].into_iter().map(|(dimension, probability)| Signal { path: "pages.py".into(), dimension: dimension.into(), probability }).collect(),
            followed_signals: 1, findings: vec![Finding {
                path: "pages.py".into(), dimension: "correctness".into(), mechanism: "incorrect_calculation".into(),
                mechanism_description: "Arithmetic, rounding, indexing or a unit conversion produces a result contrary to the visible contract for a concrete valid input.".into(),
                risk_probability: 0.73, evidence_probability: 0.37, confidence: 0.40, severity: 0.71,
                actionable: false, evidence: Region { id: "r0".into(), old_start: 0, old_lines: 0, new_start: 1, new_lines: 7, text: String::new() },
            }], api_calls: 3, http_attempts: 3, input_tokens: 0, output_tokens: 0,
            thresholds: json!({"screen":0.65,"evidence":0.8,"confidence":0.7,"fail_on":"none"}),
        };
        let body = markdown(&report);
        assert!(body.contains("Potential concerns need verification"));
        assert!(!body.contains("No issues reported"));
        assert!(body.contains("| 1 | 0 | 1 | 0 |"));
        assert!(body.contains("███████░░░"));
        assert!(body.contains("| pages.py | correctness | incorrect calculation | 0.71 | 0.370 | 0.400 |"));
        let mut duplicate = report.findings[0].clone();
        duplicate.dimension = "tests".into();
        report.findings.push(duplicate);
        let grouped = markdown(&report);
        assert!(grouped.contains("| 1 | 0 | 1 | 0 |"));
        assert!(grouped.contains("| correctness, tests | Needs verification |"));
        assert_eq!(grouped.matches("new lines 1 to 7").count(), 1);
        assert!(grouped.find("<details>").unwrap() < grouped.find("███████░░░").unwrap());
        report.findings[1].actionable = true;
        assert!(markdown(&report).contains("| 1 | 1 | 0 | 0 |"));
        report.findings[1].evidence.new_start = 20;
        assert_eq!(concern_groups(&report).len(), 2);
        report.findings[1].evidence.new_start = 1;
        report.findings[1].mechanism = "other".into();
        assert_eq!(concern_groups(&report).len(), 2);
        println!("COMMENT_PREVIEW_START\n{body}COMMENT_PREVIEW_END");
    }
}
