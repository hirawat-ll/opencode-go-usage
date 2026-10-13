//! Client for the OpenCode Go usage endpoint.
//!
//! `GET https://opencode.ai/zen/go/v1/usage` with the Go API key as a bearer
//! token answers with three rolling windows:
//!
//! ```json
//! { "usage": {
//!     "rolling": { "status": "ok", "percent": 47, "resetsAt": "2026-10-12T18:41:00Z" },
//!     "weekly":  { "status": "ok", "percent": 31, "resetsAt": "..." },
//!     "monthly": { "status": "ok", "percent": 6,  "resetsAt": "..." } } }
//! ```
//!
//! Everything is parsed leniently: a field the endpoint renames later still
//! leaves a usable panel rather than a hard failure.

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";
const USER_AGENT: &str = concat!(
    "OpenCodeGoUsage/",
    env!("CARGO_PKG_VERSION"),
    " (macOS menu bar)"
);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    #[serde(default = "ok_status")]
    pub status: String,
    #[serde(default)]
    pub percent: f64,
    #[serde(default)]
    pub resets_at: String,
}

fn ok_status() -> String {
    "ok".to_string()
}

fn missing_window() -> UsageWindow {
    UsageWindow {
        status: "unknown".to_string(),
        percent: 0.0,
        resets_at: String::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default = "missing_window")]
    pub rolling: UsageWindow,
    #[serde(default = "missing_window")]
    pub weekly: UsageWindow,
    #[serde(default = "missing_window")]
    pub monthly: UsageWindow,
    /// Only present once the endpoint exposes the wallet balance
    /// (see anomalyco/opencode#44189). Absent today, so nothing renders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance: Option<Balance>,
}

/// Credit balance, as proposed in anomalyco/opencode#44189 and #10448.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub usd: f64,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default, rename = "asOf")]
    pub as_of: Option<String>,
}

fn default_currency() -> String {
    "USD".to_string()
}

impl Usage {
    pub fn peak(&self) -> f64 {
        self.rolling
            .percent
            .max(self.weekly.percent)
            .max(self.monthly.percent)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageError {
    /// "unconfigured" | "unauthorized" | "no_subscription" | "network" | "http" | "parse"
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

impl UsageError {
    pub fn unconfigured() -> Self {
        Self {
            kind: "unconfigured".into(),
            message: "No API key yet.".into(),
            status: None,
        }
    }

    fn new(kind: &str, message: impl Into<String>, status: Option<u16>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            status,
        }
    }
}

/// A key that the user can act on, phrased for the panel.
pub fn explain(error: &UsageError) -> String {
    match error.kind.as_str() {
        "unauthorized" => "OpenCode rejected this key. Copy it again from the Console.".into(),
        "no_subscription" => "This workspace is not on a Go plan.".into(),
        "network" => error.message.clone(),
        "http" => format!("opencode.ai answered {}.", error.status.unwrap_or(0)),
        "parse" => "The usage endpoint answered with something we could not read.".into(),
        _ => error.message.clone(),
    }
}

pub async fn fetch(api_key: &str) -> Result<Usage, UsageError> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(UsageError::unconfigured());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| UsageError::new("network", format!("Could not start the request: {e}"), None))?;

    let response = client
        .get(USAGE_URL)
        .bearer_auth(key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| UsageError::new("network", friendly_network_error(&e), None))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let code = status.as_u16();
        let kind = match code {
            401 => "unauthorized",
            403 => {
                if body.contains("Entitlement") {
                    "no_subscription"
                } else {
                    "unauthorized"
                }
            }
            429 => "http",
            _ => "http",
        };
        let message = server_message(&body).unwrap_or_else(|| format!("HTTP {code}"));
        return Err(UsageError::new(kind, message, Some(code)));
    }

    let body = response
        .text()
        .await
        .map_err(|e| UsageError::new("network", friendly_network_error(&e), None))?;
    parse(&body)
}

pub fn parse(body: &str) -> Result<Usage, UsageError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| UsageError::new("parse", format!("Not JSON: {e}"), None))?;

    // The documented shape is { usage: { ... } }; accept the bare object too.
    let mut inner = value.get("usage").cloned().unwrap_or_else(|| value.clone());
    // The balance is handled separately below, and leniently; strip it so a
    // malformed one cannot fail the windows.
    if let Some(object) = inner.as_object_mut() {
        object.remove("balance");
    }

    // Missing fields inside a window are tolerated, but a response with no
    // windows at all is a contract change, not zero usage.
    let has_window = ["rolling", "weekly", "monthly"]
        .iter()
        .any(|key| inner.get(key).is_some());
    if !has_window {
        return Err(UsageError::new(
            "parse",
            "No usage windows in the response.",
            None,
        ));
    }

    let mut usage: Usage = serde_json::from_value(inner)
        .map_err(|e| UsageError::new("parse", format!("Unexpected shape: {e}"), None))?;

    // The balance is proposed to live next to `usage`; accept it in either
    // place, and ignore it when it is absent or unreadable.
    let balance = value
        .get("balance")
        .or_else(|| value.get("usage").and_then(|usage| usage.get("balance")))
        .and_then(|balance| serde_json::from_value::<Balance>(balance.clone()).ok());
    usage.balance = balance;

    Ok(usage)
}

fn server_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let message = value
        .get("error")
        .and_then(|e| e.get("message"))
        .or_else(|| value.get("message"))
        .and_then(|m| m.as_str())?;
    Some(message.chars().take(120).collect())
}

fn friendly_network_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "The request timed out.".into()
    } else if error.is_connect() {
        "No connection to opencode.ai.".into()
    } else {
        format!("Request failed: {error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_shape() {
        let body = r#"{"usage":{
            "rolling":{"status":"ok","percent":47.5,"resetsAt":"2026-10-12T18:41:00Z"},
            "weekly":{"status":"ok","percent":31,"resetsAt":"2026-10-15T00:00:00Z"},
            "monthly":{"status":"rate-limited","percent":100,"resetsAt":"2026-11-01T00:00:00Z"}}}"#;
        let usage = parse(body).expect("documented shape should parse");
        assert_eq!(usage.rolling.percent, 47.5);
        assert_eq!(usage.peak(), 100.0);
        assert_eq!(usage.monthly.status, "rate-limited");
        assert_eq!(usage.rolling.resets_at, "2026-10-12T18:41:00Z");
    }

    #[test]
    fn tolerates_a_bare_object_and_missing_fields() {
        let usage = parse(r#"{"rolling":{"percent":12}}"#).expect("bare object should parse");
        assert_eq!(usage.rolling.percent, 12.0);
        assert_eq!(usage.rolling.status, "ok");
        assert_eq!(usage.weekly.percent, 0.0);
        assert!(usage.weekly.resets_at.is_empty());
        assert!(usage.balance.is_none());
    }

    #[test]
    fn reads_a_balance_when_the_endpoint_offers_one() {
        // The shape proposed in anomalyco/opencode#44189, next to `usage`.
        let body = r#"{
            "usage": {
                "rolling": { "status": "ok", "percent": 12, "resetsAt": "2026-10-12T18:41:00Z" },
                "weekly":  { "status": "ok", "percent": 34, "resetsAt": "2026-10-15T00:00:00Z" },
                "monthly": { "status": "ok", "percent": 56, "resetsAt": "2026-11-01T00:00:00Z" }
            },
            "balance": { "usd": 4.10, "currency": "USD", "asOf": "2026-10-13T10:00:00Z" }
        }"#;
        let usage = parse(body).expect("should parse");
        let balance = usage.balance.expect("balance present");
        assert_eq!(balance.usd, 4.10);
        assert_eq!(balance.currency, "USD");
        assert_eq!(balance.as_of.as_deref(), Some("2026-10-13T10:00:00Z"));
    }

    #[test]
    fn a_balance_inside_usage_or_without_currency_still_works() {
        let nested = parse(
            r#"{"usage":{"rolling":{"percent":1},"balance":{"usd":0.5}}}"#,
        )
        .expect("should parse");
        let balance = nested.balance.expect("balance present");
        assert_eq!(balance.usd, 0.5);
        assert_eq!(balance.currency, "USD", "currency defaults");

        // A balance that cannot be read must not lose the windows.
        let broken = parse(r#"{"rolling":{"percent":7},"balance":{"note":"coming soon"}}"#)
            .expect("should still parse");
        assert_eq!(broken.rolling.percent, 7.0);
        assert!(broken.balance.is_none());
    }

    #[test]
    fn rejects_a_changed_contract() {
        assert!(parse(r#"{"usage":{"somethingElse":{}}}"#).is_err());
        assert!(parse("not json at all").is_err());
    }

    #[test]
    fn empty_key_is_unconfigured_and_not_a_network_call() {
        let error = tauri::async_runtime::block_on(fetch("   ")).expect_err("should not fetch");
        assert_eq!(error.kind, "unconfigured");
    }
}
