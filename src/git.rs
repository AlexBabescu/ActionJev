use crate::config::{Args, CiContext, Mode};
use anyhow::{bail, ensure, Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
use std::{collections::BTreeMap, io::Read, path::Path, process::{Command, Stdio}};

#[derive(Clone, Debug, Serialize)]
pub struct Region {
    pub id: String,
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub text: String,
}
#[derive(Clone, Serialize)]
pub struct FileInput { pub path: String, pub source: String, pub regions: Vec<Region> }
#[derive(Clone, Debug, Serialize)]
pub struct Skipped { pub path: String, pub reason: String, pub incomplete: bool }
#[derive(Serialize)]
pub struct Snapshot {
    pub head: String,
    pub merge_base: Option<String>,
    pub files: Vec<FileInput>,
    pub skipped: Vec<Skipped>,
}
#[derive(Clone)]
struct Entry { mode: String, oid: String, size: usize }

/// Read a bounded amount of stdout; never invoke a shell, diff driver or textconv.
fn git(repo: &Path, args: &[&str], limit: usize) -> Result<Vec<u8>> {
    let mut child = Command::new("git")
        .arg("--no-pager").arg("-C").arg(repo)
        .args(["-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false"])
        .env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0").env("GIT_LITERAL_PATHSPECS", "1")
        .env_remove("GIT_EXTERNAL_DIFF").env_remove("GIT_CONFIG_COUNT")
        .args(args).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()
        .context("cannot run Git")?;
    let mut bytes = Vec::new();
    let read = child.stdout.take().context("missing Git stdout")?.take(limit as u64 + 1).read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > limit {
        let _ = child.kill(); let _ = child.wait();
        bail!("Git output exceeded the configured limit or could not be read");
    }
    ensure!(child.wait()?.success(), "Git command failed; ensure commits are present (checkout fetch-depth: 0), repository ownership is correct and Git is installed");
    Ok(bytes)
}
fn resolve(repo: &Path, reference: &str) -> Result<String> {
    let rev = format!("{reference}^{{commit}}");
    let value = String::from_utf8(git(repo, &["rev-parse", "--verify", "--end-of-options", &rev], 256)?)?.trim().to_owned();
    ensure!((value.len() == 40 || value.len() == 64) && value.bytes().all(|b| b.is_ascii_hexdigit()), "Git returned an invalid commit ID");
    Ok(value)
}
fn tree(repo: &Path, commit: &str) -> Result<BTreeMap<String, Entry>> {
    let data = git(repo, &["ls-tree", "-r", "-l", "-z", commit, "--"], 32_000_000)?;
    let mut result = BTreeMap::new();
    for record in data.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let text = std::str::from_utf8(record).context("non-UTF-8 Git path is not supported")?;
        let (meta, path) = text.split_once('\t').context("invalid Git tree entry")?;
        let fields: Vec<_> = meta.split_whitespace().collect();
        ensure!(fields.len() == 4, "invalid Git tree metadata");
        result.insert(path.into(), Entry { mode: fields[0].into(), oid: fields[2].into(), size: fields[3].parse().unwrap_or(0) });
    }
    Ok(result)
}
fn excludes(extra: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for pattern in ["**/.env", "**/.env.*", "**/*.pem", "**/*.key", "**/*.p12", "**/*.pfx", "**/id_rsa*", "**/id_ed25519*", "**/.npmrc", "**/.pypirc", "**/credentials*", "**/secrets.*", "**/*.lock", "**/package-lock.json", "**/pnpm-lock.yaml", "**/yarn.lock", "**/vendor/**", "**/node_modules/**", "**/dist/**", "**/build/**", "**/target/**", "**/*.min.js", "**/*.map"] {
        b.add(Glob::new(pattern)?);
    }
    for p in extra { b.add(Glob::new(p).context("invalid exclusion glob")?); }
    Ok(b.build()?)
}
fn source_path(path: &str) -> bool {
    let p = Path::new(path);
    let name = p.file_name().and_then(|v| v.to_str()).unwrap_or("");
    if matches!(name, "Dockerfile" | "Containerfile" | "Makefile" | "Justfile" | "Jenkinsfile") { return true; }
    matches!(p.extension().and_then(|v| v.to_str()).unwrap_or("").to_ascii_lowercase().as_str(),
        "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "go" | "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "kt" | "kts" | "swift" | "rb" | "php" | "sh" | "bash" | "zsh" | "sql" | "tf" | "hcl" | "yaml" | "yml" | "toml" | "json" | "vue" | "svelte" | "ex" | "exs" | "scala" | "clj" | "lua" | "dart" | "sol")
}
fn range(text: &str) -> Result<(usize, usize)> {
    let (start, count) = text.split_once(',').unwrap_or((text, "1"));
    Ok((start.parse()?, count.parse()?))
}
pub fn diff_regions(diff: &str) -> Result<Vec<Region>> {
    let mut regions: Vec<Region> = Vec::new();
    for line in diff.split_inclusive('\n') {
        if let Some(header) = line.strip_prefix("@@ -") {
            let (old, rest) = header.split_once(" +").context("invalid diff hunk")?;
            let (new, _) = rest.split_once(" @@").context("invalid diff hunk range")?;
            let (old_start, old_lines) = range(old)?;
            let (new_start, new_lines) = range(new)?;
            regions.push(Region { id: format!("r{}", regions.len()), old_start, old_lines, new_start, new_lines, text: line.into() });
        } else if let Some(r) = regions.last_mut() { r.text.push_str(line); }
    }
    Ok(regions)
}
fn source_regions(source: &str) -> Vec<Region> {
    let lines: Vec<_> = source.lines().collect();
    (0..lines.len()).step_by(70).map(|start| {
        let end = (start + 80).min(lines.len());
        Region { id: format!("r{}", start / 70), old_start: 0, old_lines: 0, new_start: start + 1, new_lines: end - start, text: lines[start..end].join("\n") }
    }).collect()
}
pub fn collect(args: &Args, ci: &CiContext) -> Result<Snapshot> {
    let head = resolve(&args.repo, &ci.head)?;
    let merge_base = if args.mode == Mode::Changes {
        let base = resolve(&args.repo, ci.base.as_deref().context("missing base")?)?;
        Some(String::from_utf8(git(&args.repo, &["merge-base", &base, &head], 256)?)?.trim().to_owned())
    } else { None };
    let head_tree = tree(&args.repo, &head)?;
    let old_tree = match &merge_base { Some(b) => tree(&args.repo, b)?, None => BTreeMap::new() };
    let paths: Vec<String> = if let Some(base) = &merge_base {
        let bytes = git(&args.repo, &["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--name-only", "-z", base, &head, "--"], 32_000_000)?;
        bytes.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|p| String::from_utf8(p.to_vec()).context("non-UTF-8 Git path")).collect::<Result<_>>()?
    } else { head_tree.keys().cloned().collect() };
    let excluded = excludes(&args.exclude)?;
    let mut snapshot = Snapshot { head: head.clone(), merge_base: merge_base.clone(), files: Vec::new(), skipped: Vec::new() };
    let mut total = 0;
    for path in paths {
        let mut skip = |reason: &str, incomplete| snapshot.skipped.push(Skipped { path: path.clone(), reason: reason.into(), incomplete });
        if excluded.is_match(&path) { skip("excluded", false); continue; }
        if !source_path(&path) { skip("unsupported_extension", false); continue; }
        let entry = head_tree.get(&path).or_else(|| old_tree.get(&path)).context("missing tree entry")?;
        if !matches!(entry.mode.as_str(), "100644" | "100755") { skip("symlink_or_submodule", false); continue; }
        if snapshot.files.len() >= args.max_files { skip("max_files", true); continue; }
        if entry.size > args.max_file_bytes || old_tree.get(&path).is_some_and(|e| e.size > args.max_file_bytes) { skip("max_file_bytes", true); continue; }
        let bytes = git(&args.repo, &["cat-file", "blob", &entry.oid], args.max_file_bytes)?;
        if bytes.contains(&0) { skip("binary", false); continue; }
        let source = match String::from_utf8(bytes) { Ok(s) => s, Err(_) => { skip("non_utf8_source", false); continue; } };
        let regions = if let Some(base) = &merge_base {
            let patch = String::from_utf8(git(&args.repo, &["diff", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", "--unified=5", base, &head, "--", &path], args.max_file_bytes * 4 + 4096)?)?;
            diff_regions(&patch)?
        } else { source_regions(&source) };
        if regions.len() > args.max_regions { skip("max_regions", true); continue; }
        if regions.is_empty() { skip("no_text_regions", false); continue; }
        let bytes = source.len() + regions.iter().map(|r| r.text.len()).sum::<usize>();
        if total + bytes > args.max_total_bytes { skip("max_total_bytes", true); continue; }
        total += bytes;
        snapshot.files.push(FileInput { path, source, regions });
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn diff_line_ranges() {
        let p = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -2,0 +3,2 @@ fn x\n+a\n+b\n@@ -8 +10 @@\n-x\n+y\n";
        let r = diff_regions(p).unwrap();
        assert_eq!(r.len(), 2); assert_eq!(r[0].new_start, 3); assert_eq!(r[0].old_lines, 0); assert_eq!(r[1].new_lines, 1);
    }
    #[test] fn removed_file() {
        let r = diff_regions("@@ -1,2 +0,0 @@\n-a\n-b\n").unwrap();
        assert_eq!(r[0].new_lines, 0); assert_eq!(r[0].old_start, 1);
    }
    #[test] fn malformed_hunk_fails() { assert!(diff_regions("@@ -oops +1 @@\n").is_err()); }
    #[test] fn secrets_excluded_at_root_and_below() {
        let e = excludes(&[]).unwrap();
        for p in [".env", ".env.production", "a/.env", "a/private.key", "credentials.json", "Cargo.lock"] { assert!(e.is_match(p), "{p}"); }
        assert!(!e.is_match("src/main.rs"));
    }
    #[test] fn source_chunks_overlap() {
        let s = (0..150).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        let r = source_regions(&s); assert_eq!(r.len(), 3); assert_eq!(r[1].new_start, 71);
    }
}
