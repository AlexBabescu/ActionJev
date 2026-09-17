use anyhow::{bail, ensure, Context, Result};
use reqwest::{blocking::Client, redirect::Policy, Method, Url};
use serde_json::Value;
use std::{io::Read, sync::atomic::{AtomicUsize, Ordering}, thread, time::{Duration, SystemTime}};

pub struct Http { client: Client, pub attempts: AtomicUsize, allow_http: bool }
pub fn checked_url(value: &str, allow_http: bool) -> Result<Url> {
    let url = Url::parse(value).context("invalid service URL")?;
    ensure!(url.scheme() == "https" || (allow_http && url.scheme() == "http"), "service URLs require HTTPS; use --allow-insecure-http explicitly for a trusted local endpoint");
    ensure!(url.host_str().is_some() && url.username().is_empty() && url.password().is_none() && url.query().is_none() && url.fragment().is_none(), "service URL must not contain credentials, a query or fragment");
    Ok(url)
}
impl Http {
    pub fn new(timeout: u64, allow_http: bool) -> Result<Self> {
        Ok(Self { client: Client::builder().timeout(Duration::from_secs(timeout)).connect_timeout(Duration::from_secs(10)).redirect(Policy::none()).user_agent(concat!("ActionJev/", env!("CARGO_PKG_VERSION"))).build()?, attempts: AtomicUsize::new(0), allow_http })
    }
    pub fn json(&self, method: Method, url: &str, token: &str, scheme: &str, body: Option<&Value>, retry: bool) -> Result<Value> {
        self.send(method, checked_url(url, self.allow_http)?, token, scheme, body, retry)
    }
    /// Only this method adds known pagination parameters to a validated base URL.
    pub fn page(&self, url: &str, token: &str, scheme: &str, page: usize, gitea: bool) -> Result<Value> {
        let mut url = checked_url(url, self.allow_http)?;
        url.query_pairs_mut().append_pair("page", &page.to_string()).append_pair(if gitea { "limit" } else { "per_page" }, "50");
        self.send(Method::GET, url, token, scheme, None, true)
    }
    /// POST comment creation is deliberately not retried: a timeout may have committed it.
    fn send(&self, method: Method, url: Url, token: &str, scheme: &str, body: Option<&Value>, retry: bool) -> Result<Value> {
        let count = if retry { 3 } else { 1 };
        for attempt in 0..count {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            let mut request = self.client.request(method.clone(), url.clone()).header("Accept", "application/json");
            if !token.is_empty() {
                let mut auth = reqwest::header::HeaderValue::from_str(&format!("{scheme} {token}")).context("invalid token header")?;
                auth.set_sensitive(true); request = request.header(reqwest::header::AUTHORIZATION, auth);
            }
            if let Some(value) = body { request = request.json(value); }
            match request.send() {
                Err(_) if attempt + 1 < count => { thread::sleep(Duration::from_millis(250 << attempt)); continue; }
                Err(_) => bail!("HTTP transport failed (response body and credentials withheld)"),
                Ok(response) => {
                    let status = response.status();
                    if (status.as_u16() == 408 || status.as_u16() == 429 || status.is_server_error()) && attempt + 1 < count {
                        let wait = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(retry_delay).unwrap_or_else(|| Duration::from_millis(250 << attempt));
                        ensure!(wait <= Duration::from_secs(60), "Retry-After exceeds the retry time budget");
                        thread::sleep(wait); continue;
                    }
                    ensure!(status.is_success(), "HTTP {} from configured service (response body withheld)", status.as_u16());
                    let mut bytes = Vec::new();
                    response.take(8_000_001).read_to_end(&mut bytes).context("cannot read HTTP response")?;
                    ensure!(bytes.len() <= 8_000_000, "HTTP response exceeds 8 MB limit");
                    return serde_json::from_slice(&bytes).context("service returned invalid JSON");
                }
            }
        }
        bail!("HTTP retry budget exhausted")
    }
}
fn retry_delay(value: &str) -> Option<Duration> {
    value.parse::<u64>().ok().map(Duration::from_secs).or_else(|| httpdate::parse_http_date(value).ok().map(|t| t.duration_since(SystemTime::now()).unwrap_or_default()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn urls_are_restricted() {
        assert!(checked_url("https://api.typesafe.ai/v1/systemone", false).is_ok());
        for url in ["http://localhost", "file:///etc/passwd", "https://x/?key=secret", "https://u:p@x", "https://x/#fragment"] { assert!(checked_url(url, false).is_err()); }
        assert!(checked_url("http://127.0.0.1:8000/api/v1", true).is_ok());
    }
    #[test] fn retries_parse_seconds() { assert_eq!(retry_delay("2"), Some(Duration::from_secs(2))); assert!(retry_delay("bad").is_none()); }
}
