use crate::{config::Args, http::Http};
use anyhow::{bail, ensure, Context, Result};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Mutex, time::Instant};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    Noul { noul: f64 },
    Choice { choice: String, probabilities: BTreeMap<String, f64>, confidence: f64 },
    Score { score: f64, probabilities: BTreeMap<String, f64>, confidence: f64 },
}
impl Answer {
    pub fn noul(&self) -> Result<f64> { if let Self::Noul { noul } = self { Ok(*noul) } else { bail!("expected Noul answer") } }
    pub fn choice(&self) -> Result<(&str, f64)> { if let Self::Choice { choice, confidence, .. } = self { Ok((choice, *confidence)) } else { bail!("expected Choice answer") } }
    pub fn score(&self) -> Result<(f64, f64)> { if let Self::Score { score, confidence, .. } = self { Ok((*score, *confidence)) } else { bail!("expected Score answer") } }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Usage { #[serde(default)] pub input_tokens: u64, #[serde(default)] pub output_tokens: u64 }
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Reply { pub model: String, pub answers: BTreeMap<String, Answer>, #[serde(default)] pub usage: Usage }
impl Reply {
    pub fn answer(&self, key: &str) -> Result<&Answer> { self.answers.get(key).context("missing required Jev answer") }
}
fn probability(v: f64) -> Result<()> { ensure!(v.is_finite() && (0.0..=1.0).contains(&v), "invalid probability or confidence"); Ok(()) }
fn distribution(values: &BTreeMap<String, f64>, keys: &[String]) -> Result<()> {
    ensure!(values.len() == keys.len() && keys.iter().all(|k| values.contains_key(k)), "probability distribution does not match the question");
    for v in values.values() { probability(*v)?; }
    ensure!((values.values().sum::<f64>() - 1.0).abs() <= 0.02, "probabilities must sum to one");
    Ok(())
}
pub fn validate(reply: &Reply, questions: &Value) -> Result<()> {
    let questions = questions.as_object().context("questions must be an object")?;
    ensure!(reply.answers.len() == questions.len(), "Jev answer count does not match request");
    for (key, q) in questions {
        match (q["type"].as_str(), reply.answer(key)?) {
            (Some("noul"), Answer::Noul { noul }) => probability(*noul)?,
            (Some("choice"), Answer::Choice { choice, probabilities, confidence }) => {
                let options = q["criteria"].as_object().context("invalid choice criteria")?;
                ensure!(options.contains_key(choice), "Jev chose an option outside the supplied choices");
                probability(*confidence)?;
                distribution(probabilities, &options.keys().cloned().collect::<Vec<_>>())?;
            }
            (Some("score"), Answer::Score { score, probabilities, confidence }) => {
                let levels = q["criteria"].as_array().context("invalid score criteria")?.len();
                ensure!(levels >= 2 && score.is_finite() && *score >= 0.0 && *score <= (levels - 1) as f64, "invalid severity score");
                probability(*confidence)?;
                distribution(probabilities, &(0..levels).map(|i| i.to_string()).collect::<Vec<_>>())?;
            }
            _ => bail!("Jev answer type does not match question"),
        }
    }
    Ok(())
}
pub struct Jev<'a> { pub http: Http, args: &'a Args, key: String, pub traces: Mutex<Vec<Value>> }
impl<'a> Jev<'a> {
    pub fn new(args: &'a Args) -> Result<Self> {
        let key = std::env::var("TYPESAFE_API_KEY").context("TYPESAFE_API_KEY is required for a nonempty review")?;
        ensure!(!key.trim().is_empty(), "TYPESAFE_API_KEY is empty");
        Ok(Self { http: Http::new(args.timeout_seconds, args.allow_insecure_http)?, args, key, traces: Mutex::new(Vec::new()) })
    }
    pub fn ask(&self, path: &str, stage: &str, state: &Value, questions: Value) -> Result<Reply> {
        let started = Instant::now();
        let request = json!({"model": self.args.model, "state": state, "questions": questions});
        ensure!(serde_json::to_vec(&request)?.len() <= 500_000, "Jev request exceeds the 500 KB safety limit; lower max-file-bytes or max-regions");
        let raw = self.http.json(Method::POST, &self.args.typesafe_url, &self.key, "Bearer", Some(&request), true)?;
        let reply: Reply = serde_json::from_value(raw).context("invalid Jev response shape")?;
        validate(&reply, &questions)?;
        // Store question/answer traces, not credentials or source/state. This is an audit log, not model chain-of-thought.
        self.traces.lock().map_err(|_| anyhow::anyhow!("trace lock poisoned"))?.push(json!({"path":path,"stage":stage,"questions":questions,"response":reply,"elapsed_ms":started.elapsed().as_millis()}));
        Ok(reply)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn validates_noul() {
        let r: Reply = serde_json::from_value(json!({"model":"jev-test","answers":{"x":{"type":"noul","noul":0.9}}})).unwrap();
        assert!(validate(&r, &json!({"x":{"type":"noul"}})).is_ok());
        assert!(validate(&r, &json!({"missing":{"type":"noul"}})).is_err());
    }
    #[test] fn invalid_probabilities_fail_closed() {
        assert!(probability(f64::NAN).is_err()); assert!(probability(1.01).is_err());
        assert!(distribution(&BTreeMap::from([("a".into(), 0.1)]), &["a".into()]).is_err());
    }
    #[test] fn unknown_choice_fails() {
        let r: Reply = serde_json::from_value(json!({"model":"test","answers":{"x":{"type":"choice","choice":"evil","probabilities":{"evil":1.0},"confidence":1.0}}})).unwrap();
        assert!(validate(&r, &json!({"x":{"type":"choice","criteria":{"good":"valid"}}})).is_err());
    }
}
