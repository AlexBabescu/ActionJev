use crate::{config::{Args, Mode}, git::{FileInput, Region, Skipped, Snapshot}, jev::Jev};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, BTreeSet}, sync::{atomic::{AtomicUsize, Ordering}, Mutex}, thread};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dimension { pub id: String, pub statement: String }
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: u32, pub guidance: String, pub dimensions: Vec<Dimension>,
    pub mechanisms: BTreeMap<String, String>, pub severity_levels: Vec<String>,
}
pub fn load_policy(args: &Args) -> Result<(Policy, String)> {
    let bytes = match &args.policy {
        Some(path) => { ensure!(std::fs::metadata(path)?.len() <= 100_000, "policy exceeds 100 KB"); std::fs::read(path)? },
        None => include_bytes!("../prompts/review.json").to_vec(),
    };
    let policy: Policy = serde_json::from_slice(&bytes).context("invalid policy JSON")?;
    ensure!(policy.version == 1 && (1..=20).contains(&policy.dimensions.len()), "unsupported policy version or dimension count");
    ensure!(policy.severity_levels.len() == 5, "policy must define exactly five severity levels (0..4)");
    ensure!(policy.mechanisms.contains_key("none") && policy.mechanisms.len() >= 2, "policy mechanisms must include none and at least one defect mechanism");
    let mut ids = BTreeSet::new();
    for d in &policy.dimensions {
        ensure!(!d.id.is_empty() && d.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && ids.insert(&d.id), "dimension IDs must be unique ASCII identifiers");
        ensure!(!d.statement.trim().is_empty(), "dimension statement cannot be empty");
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    Ok((policy, hash))
}
#[derive(Clone, Serialize)]
pub struct Signal { pub path: String, pub dimension: String, pub probability: f64 }
#[derive(Clone, Serialize)]
pub struct Finding {
    pub path: String, pub dimension: String, pub mechanism: String,
    pub mechanism_description: String, pub risk_probability: f64,
    pub evidence_probability: f64, pub confidence: f64, pub severity: f64,
    pub actionable: bool, pub evidence: Region,
}
#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32, pub tool_version: String, pub mode: Mode,
    pub model_requested: String, pub policy_sha256: String,
    pub head: String, pub merge_base: Option<String>,
    pub complete: bool, pub dry_run: bool,
    pub scanned_files: usize, pub skipped: Vec<Skipped>,
    pub signals: Vec<Signal>, pub followed_signals: usize,
    pub findings: Vec<Finding>, pub api_calls: usize, pub http_attempts: usize,
    pub input_tokens: u64, pub output_tokens: u64,
    pub thresholds: Value,
}
impl Report {
    pub fn actionable_count(&self) -> usize { self.findings.iter().filter(|f| f.actionable).count() }
    pub fn gate_failed(&self, args: &Args) -> bool {
        !self.dry_run && args.fail_on.threshold().is_some_and(|t| self.findings.iter().any(|f| f.actionable && f.severity >= t))
    }
}
fn parallel<T: Sync, R: Send, F: Fn(&T) -> Result<R> + Sync>(items: &[T], concurrency: usize, f: F) -> Result<Vec<R>> {
    let next = AtomicUsize::new(0);
    let output = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..concurrency.min(items.len()) {
            let next = &next; let output = &output; let f = &f;
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= items.len() { break; }
                let result = f(&items[index]);
                output.lock().expect("worker lock poisoned").push((index, result));
            });
        }
    });
    let mut values = output.into_inner().map_err(|_| anyhow::anyhow!("worker lock poisoned"))?;
    values.sort_by_key(|v| v.0);
    values.into_iter().map(|(_, r)| r).collect()
}
fn instructions(policy: &Policy, question: &str) -> Value { json!({"question":question,"boundaries":policy.guidance}) }
fn state(file: &FileInput, mode: Mode) -> Value { json!({"mode":mode,"file":file.path,"source_at_reviewed_commit":file.source,"regions":file.regions}) }

pub fn run(args: &Args, snapshot: Snapshot, policy: &Policy, policy_hash: String) -> Result<(Report, Vec<Value>)> {
    let mut report = Report {
        schema_version: 1, tool_version: env!("CARGO_PKG_VERSION").into(), mode: args.mode,
        model_requested: args.model.clone(), policy_sha256: policy_hash,
        head: snapshot.head, merge_base: snapshot.merge_base,
        complete: !snapshot.skipped.iter().any(|s| s.incomplete), dry_run: args.dry_run,
        scanned_files: 0, skipped: snapshot.skipped, signals: Vec::new(), followed_signals: 0,
        findings: Vec::new(), api_calls: 0, http_attempts: 0, input_tokens: 0, output_tokens: 0,
        thresholds: json!({"screen":args.screen_threshold,"evidence":args.evidence_threshold,"confidence":args.min_confidence,"fail_on":args.fail_on}),
    };
    if args.dry_run || snapshot.files.is_empty() { return Ok((report, vec![])); }
    let jev = Jev::new(args)?;
    let rows = parallel(&snapshot.files, args.concurrency, |file| {
        let mut questions = serde_json::Map::new();
        for d in &policy.dimensions {
            questions.insert(d.id.clone(), json!({"type":"noul","instructions":instructions(policy, &d.statement),"criteria":{"true":"A specific reachable failure is supported by this code.","false":"No demonstrated failure, insufficient evidence, or merely a style preference."}}));
        }
        let reply = jev.ask(&file.path, "screen", &state(file, args.mode), Value::Object(questions))?;
        policy.dimensions.iter().map(|d| Ok(Signal { path: file.path.clone(), dimension: d.id.clone(), probability: reply.answer(&d.id)?.noul()? })).collect::<Result<Vec<_>>>()
    })?;
    report.scanned_files = snapshot.files.len();
    report.signals = rows.into_iter().flatten().collect();
    let mut candidates: Vec<_> = report.signals.iter().filter(|s| s.probability >= args.screen_threshold).collect();
    candidates.sort_by(|a, b| b.probability.total_cmp(&a.probability).then(a.path.cmp(&b.path)).then(a.dimension.cmp(&b.dimension)));
    if candidates.len() > args.max_followups { report.complete = false; }
    candidates.truncate(args.max_followups);
    report.followed_signals = candidates.len();
    let inspected = parallel(&candidates, args.concurrency, |signal| {
        let file = snapshot.files.iter().find(|f| f.path == signal.path).context("missing source file")?;
        let dimension = policy.dimensions.iter().find(|d| d.id == signal.dimension).context("missing dimension")?;
        let mut criteria = BTreeMap::from([("none".to_owned(), "No supplied region establishes a concrete defect for this question.".to_owned())]);
        for r in &file.regions {
            criteria.insert(r.id.clone(), format!("Region {} of this file: old lines {}+{}, new lines {}+{}. Select only if this region establishes the failure.", r.id, r.old_start, r.old_lines, r.new_start, r.new_lines));
        }
        let location = jev.ask(&file.path, &format!("locate:{}", dimension.id), &state(file, args.mode), json!({"evidence":{"type":"choice","instructions":instructions(policy, &format!("Select the single strongest supplied evidence region for this question: {} Select none when the failure is speculative.", dimension.statement)),"criteria":criteria}}))?;
        let (selected, location_confidence) = location.answer("evidence")?.choice()?;
        if selected == "none" { return Ok(None); }
        let region = file.regions.iter().find(|r| r.id == selected).context("unknown evidence region")?;
        let focused = json!({"mode":args.mode,"file":file.path,"question":dimension.statement,"selected_evidence":region,"source_at_reviewed_commit":file.source});
        let reply = jev.ask(&file.path, &format!("judge:{}:{}", dimension.id, region.id), &focused, json!({
            "supported":{"type":"noul","instructions":instructions(policy,"Does the selected evidence establish a specific reachable failure for the supplied question? Trace a concrete input against the visible contract. Reject hypothetical assumptions, unrelated existing defects, and problems requiring unseen code to substantiate. A deliberate change is a defect when it contradicts the visible contract.")},
            "mechanism":{"type":"choice","instructions":instructions(policy,"Which supplied mechanism explains the failure supported by the selected evidence and question? Select none when no mechanism is established."),"criteria":policy.mechanisms},
            "severity":{"type":"score","instructions":instructions(policy,"What functional impact is supported by the selected evidence for the supplied question? Use the no-harm level when a defect is not established. Do not assume the most dangerous possible deployment."),"criteria":policy.severity_levels}
        }))?;
        let evidence_probability = reply.answer("supported")?.noul()?;
        let (mechanism, mechanism_confidence) = reply.answer("mechanism")?.choice()?;
        let (severity, severity_confidence) = reply.answer("severity")?.score()?;
        if mechanism == "none" { return Ok(None); }
        let confidence = location_confidence.min(mechanism_confidence).min(severity_confidence);
        Ok(Some(Finding {
            path: file.path.clone(), dimension: dimension.id.clone(), mechanism: mechanism.into(),
            mechanism_description: policy.mechanisms.get(mechanism).context("unknown mechanism")?.clone(),
            risk_probability: signal.probability, evidence_probability, confidence, severity,
            actionable: evidence_probability >= args.evidence_threshold && confidence >= args.min_confidence && severity >= 1.0,
            evidence: region.clone(),
        }))
    })?;
    report.findings = inspected.into_iter().flatten().collect();
    report.findings.sort_by(|a,b| b.severity.total_cmp(&a.severity).then(a.path.cmp(&b.path)).then(a.dimension.cmp(&b.dimension)));
    let traces = jev.traces.into_inner().map_err(|_| anyhow::anyhow!("trace lock poisoned"))?;
    report.api_calls = traces.len();
    report.http_attempts = jev.http.attempts.load(Ordering::Relaxed);
    for trace in &traces {
        report.input_tokens += trace.pointer("/response/usage/input_tokens").and_then(Value::as_u64).unwrap_or(0);
        report.output_tokens += trace.pointer("/response/usage/output_tokens").and_then(Value::as_u64).unwrap_or(0);
    }
    Ok((report, traces))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parallel_preserves_order() { let values = parallel(&[3,1,2], 2, |x| Ok(x * 2)).unwrap(); assert_eq!(values, vec![6,2,4]); }
    #[test] fn parallel_propagates_failure() { assert!(parallel(&[1,2], 2, |_| -> Result<u8> { anyhow::bail!("failed") }).is_err()); }
    #[test] fn default_policy_is_valid_json() { let p: Policy = serde_json::from_str(include_str!("../prompts/review.json")).unwrap(); assert_eq!(p.dimensions.len(), 5); assert!(p.mechanisms.contains_key("none")); }
}
