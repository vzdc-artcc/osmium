use std::{collections::BTreeMap, time::Duration};

use anyhow::{Context, Result};
use reqwest::{Client, Method, StatusCode};
use serde_json::Value;
use tokio::time::Instant;

#[derive(Debug, Clone)]
pub struct RequestPlan {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
    pub query: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct HttpOutcome {
    pub status: Option<StatusCode>,
    pub latency_ms: u128,
    pub body_json: Option<Value>,
    pub body_text: String,
    pub timed_out: bool,
}

pub async fn execute_request(
    client: &Client,
    base_url: &str,
    timeout_ms: u64,
    plan: &RequestPlan,
    auth_header: Option<&str>,
) -> Result<HttpOutcome> {
    let url = format!("{base_url}{}", plan.path);
    let mut request = client.request(plan.method.clone(), &url);
    if let Some(token) = auth_header {
        request = request.header("Authorization", token);
    }
    if !plan.query.is_empty() {
        request = request.query(&plan.query);
    }
    if let Some(body) = &plan.body {
        request = request.json(body);
    }
    request = request.timeout(Duration::from_millis(timeout_ms));

    let started = Instant::now();
    let response = request.send().await;
    let latency_ms = started.elapsed().as_millis();

    match response {
        Ok(response) => {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            let body_json = serde_json::from_str::<Value>(&body_text).ok();
            Ok(HttpOutcome {
                status: Some(status),
                latency_ms,
                body_json,
                body_text,
                timed_out: false,
            })
        }
        Err(error) if error.is_timeout() => Ok(HttpOutcome {
            status: None,
            latency_ms,
            body_json: None,
            body_text: error.to_string(),
            timed_out: true,
        }),
        Err(error) => {
            Err(error).with_context(|| format!("request failed for {} {}", plan.method, plan.path))
        }
    }
}

pub fn update_statuses(statuses: &mut BTreeMap<u16, usize>, status: Option<StatusCode>) {
    if let Some(status) = status {
        *statuses.entry(status.as_u16()).or_insert(0) += 1;
    }
}
