use anyhow::{bail, ensure, Context, Result};
use clap::{Parser, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::{env, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Mode { Changes, Codebase }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Platform { Auto, Github, Gitea, None }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum FailOn { None, Low, Medium, High, Critical }
impl FailOn {
    pub fn threshold(self) -> Option<f64> {
        match self { Self::None => None, Self::Low => Some(1.0), Self::Medium => Some(2.0), Self::High => Some(3.0), Self::Critical => Some(4.0) }
    }
}

/// Secrets are accepted only through environment variables, never CLI arguments.
#[derive(Parser)]
#[command(version, about)]
pub struct Args {
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,
    #[arg(long, value_enum, default_value = "changes")]
    pub mode: Mode,
    #[arg(long)]
    pub base: Option<String>,
    #[arg(long)]
    pub head: Option<String>,
    #[arg(long, value_enum, default_value = "auto")]
    pub platform: Platform,
    #[arg(long)]
    pub repository: Option<String>,
    #[arg(long)]
    pub pr_number: Option<u64>,
    #[arg(long)]
    pub api_url: Option<String>,
    #[arg(long)]
    pub event_path: Option<PathBuf>,
    #[arg(long, default_value = "https://api.typesafe.ai/v1/systemone")]
    pub typesafe_url: String,
    #[arg(long, default_value = "jev-latest")]
    pub model: String,
    #[arg(long)]
    pub policy: Option<PathBuf>,
    #[arg(long)]
    pub output_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 4)]
    pub concurrency: usize,
    #[arg(long, default_value_t = 50)]
    pub max_files: usize,
    #[arg(long, default_value_t = 48000)]
    pub max_file_bytes: usize,
    #[arg(long, default_value_t = 1000000)]
    pub max_total_bytes: usize,
    #[arg(long, default_value_t = 32)]
    pub max_regions: usize,
    #[arg(long, default_value_t = 24)]
    pub max_followups: usize,
    #[arg(long, default_value_t = 0.65)]
    pub screen_threshold: f64,
    #[arg(long, default_value_t = 0.8)]
    pub evidence_threshold: f64,
    #[arg(long, default_value_t = 0.7)]
    pub min_confidence: f64,
    #[arg(long, value_enum, default_value = "none")]
    pub fail_on: FailOn,
    #[arg(long)]
    pub allow_incomplete: bool,
    #[arg(long)]
    pub comment: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub allow_insecure_http: bool,
    #[arg(long, default_value_t = 30)]
    pub timeout_seconds: u64,
    /// Additional excluded glob. May be repeated; built-in secret exclusions remain active.
    #[arg(long)]
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CiContext {
    pub platform: Platform,
    pub repository: Option<String>,
    pub pr_number: Option<u64>,
    pub api_url: Option<String>,
    pub base: Option<String>,
    pub head: String,
}

pub fn env_first(names: &[&str]) -> Option<String> {
    names.iter().find_map(|n| env::var(n).ok().filter(|v| !v.is_empty()))
}

impl Args {
    pub fn validate(&self) -> Result<()> {
        ensure!((1..=32).contains(&self.concurrency), "concurrency must be 1..32");
        ensure!((1..=10000).contains(&self.max_files), "max-files must be 1..10000");
        ensure!((1024..=1000000).contains(&self.max_file_bytes), "max-file-bytes must be 1024..1000000");
        ensure!((1024..=100000000).contains(&self.max_total_bytes), "max-total-bytes must be 1024..100000000");
        ensure!((1..=128).contains(&self.max_regions), "max-regions must be 1..128");
        ensure!((1..=1000).contains(&self.max_followups), "max-followups must be 1..1000");
        ensure!((1..=300).contains(&self.timeout_seconds), "timeout-seconds must be 1..300");
        for value in [self.screen_threshold, self.evidence_threshold, self.min_confidence] {
            ensure!(value.is_finite() && (0.0..=1.0).contains(&value), "thresholds must be finite values in 0..1");
        }
        ensure!(!self.model.trim().is_empty(), "model cannot be empty");
        Ok(())
    }

    pub fn context(&self) -> Result<CiContext> {
        let event_path = self.event_path.clone().or_else(|| env_first(&["GITEA_EVENT_PATH", "GITHUB_EVENT_PATH"]).map(PathBuf::from));
        let event = if let Some(path) = event_path {
            ensure!(fs::metadata(&path)?.len() <= 2_000_000, "event payload exceeds limit");
            serde_json::from_slice::<Value>(&fs::read(path)?).context("invalid event JSON")?
        } else { Value::Null };
        let platform = match self.platform {
            Platform::Auto if env_first(&["GITEA_ACTIONS"]).is_some_and(|v| v == "true") => Platform::Gitea,
            Platform::Auto if env_first(&["GITHUB_ACTIONS"]).is_some_and(|v| v == "true") => Platform::Github,
            Platform::Auto => Platform::None,
            other => other,
        };
        let repository = self.repository.clone().or_else(|| env_first(&["GITEA_REPOSITORY", "GITHUB_REPOSITORY"]));
        if let Some(repo) = &repository {
            let parts: Vec<_> = repo.split('/').collect();
            ensure!(parts.len() == 2 && parts.iter().all(|p| !p.is_empty() && *p != "." && *p != ".." && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))), "repository must be owner/name");
        }
        let pr_number = self.pr_number.or_else(|| event.pointer("/pull_request/number").and_then(Value::as_u64)).or_else(|| {
            if event.get("pull_request").is_some() { event.get("number").and_then(Value::as_u64) } else { None }
        });
        if pr_number == Some(0) { bail!("PR number must be positive"); }
        let base = self.base.clone().or_else(|| event.pointer("/pull_request/base/sha").and_then(Value::as_str).map(str::to_owned));
        let head = self.head.clone().or_else(|| event.pointer("/pull_request/head/sha").and_then(Value::as_str).map(str::to_owned)).unwrap_or_else(|| "HEAD".into());
        let api_url = self.api_url.clone().or_else(|| env_first(&["GITEA_API_URL", "GITHUB_API_URL"])).or_else(|| match platform {
            Platform::Github => Some("https://api.github.com".into()),
            Platform::Gitea => env_first(&["GITEA_SERVER_URL", "GITHUB_SERVER_URL"]).map(|url| format!("{}/api/v1", url.trim_end_matches('/'))),
            _ => None,
        });
        if self.mode == Mode::Changes && base.is_none() { bail!("changes mode requires --base or a pull_request event; use checkout fetch-depth: 0"); }
        if self.comment {
            ensure!(!matches!(platform, Platform::None | Platform::Auto), "comments require --platform github or gitea");
            ensure!(repository.is_some() && pr_number.is_some() && api_url.is_some(), "comments require repository, PR number and API URL");
            ensure!(self.mode == Mode::Changes, "PR comments are available only in changes mode");
        }
        Ok(CiContext { platform, repository, pr_number, api_url, base, head })
    }
}
