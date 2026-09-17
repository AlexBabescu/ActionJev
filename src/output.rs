use crate::{config::{env_first, Args, CiContext, Platform}, http::{checked_url, Http}, review::Report};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde_json::{json, Value};
use std::{fs::{self, OpenOptions}, io::Write, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

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
pub fn markdown(report: &Report) -> String {
    let status = if report.dry_run { "DRY RUN — no model review performed" } else if !report.complete { "INCOMPLETE — review budget or coverage limit reached" } else { "Completed within the configured scope" };
    let mut text = format!("{MARKER}\n## ActionJev review\n\n**{status}**\n\nCommit: `{}`\n\n{} files reviewed · {} actionable review candidates · {} uncertain candidates · {} API calls\n\n", report.head, report.scanned_files, report.actionable_count(), report.findings.len() - report.actionable_count(), report.api_calls);
    text.push_str("These are structured model judgments, not proven defects. Confidence describes the model's distribution, not a measured probability of correctness.\n\n");
    if !report.findings.is_empty() {
        text.push_str("| Evidence | Category / mechanism | Severity (0–4) | Support | Confidence | Disposition |\n|---|---|---:|---:|---:|---|\n");
        for f in &report.findings {
            let (side, start, lines) = if f.evidence.new_lines > 0 { ("new", f.evidence.new_start, f.evidence.new_lines) } else { ("old", f.evidence.old_start, f.evidence.old_lines) };
            let row = format!("| {} — {} lines {}–{} | {} / {} | {:.2} | {:.2} | {:.2} | {} |\n", escape(&f.path), side, start, start.saturating_add(lines.saturating_sub(1)), escape(&f.dimension), escape(&f.mechanism), f.severity, f.evidence_probability, f.confidence, if f.actionable { "Investigate" } else { "Uncertain; not gated" });
            if text.len() + row.len() > 48_000 { text.push_str("\nAdditional findings omitted from this comment; see report.json.\n"); break; }
            text.push_str(&row);
        }
    } else if !report.dry_run {
        text.push_str("No findings survived evidence selection. This is not a guarantee that the code is defect-free.\n");
    }
    text.push_str(&format!("\n{} files excluded or skipped. {} signals followed. JSON and question/answer traces contain the detailed review.\n", report.skipped.len(), report.followed_signals));
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
        Err(_) if ci.platform == Platform::Github => Some("github-actions[bot]".into()),
        Err(_) => None,
    };
    let comments_url = url(&["repos", owner, name, "issues", &number, "comments"])?;
    let mut existing = None;
    // Build pagination in the request URL; validate base before adding these trusted parameters.
    // The transport disallows arbitrary query strings, so use the unpaginated endpoint here.
    // A capped first page avoids unbounded reads; absent matches result in a fresh comment.
    let comments = http.json(Method::GET, &comments_url, &token, scheme, None, true)?;
    for entry in comments.as_array().context("invalid comments response")? {
        if identity.as_deref().is_some_and(|id| entry.pointer("/user/login").and_then(Value::as_str) == Some(id)) && entry.get("body").and_then(Value::as_str).is_some_and(|body| body.starts_with(MARKER)) {
            existing = entry.get("id").and_then(Value::as_u64); break;
        }
    }
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
}
