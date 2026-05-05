use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::personas::Persona;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    CookieSession,
    Bearer,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonaReport {
    pub persona: Persona,
    pub available: bool,
    pub auth_kind: Option<AuthKind>,
    pub cid: Option<i64>,
    pub user_id: Option<String>,
    pub display_name: Option<String>,
    pub validation_route: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredRouteRecord {
    pub key: String,
    pub method: String,
    pub path: String,
    pub tag: String,
    pub route_class: String,
    pub included: bool,
    pub persona: Option<Persona>,
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryReport {
    pub total_routes: usize,
    pub included_routes: usize,
    pub skipped_routes: usize,
    pub routes: Vec<DiscoveredRouteRecord>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Metrics {
    pub request_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub timeout_count: usize,
    pub min_latency_ms: Option<u128>,
    pub max_latency_ms: Option<u128>,
    pub avg_latency_ms: Option<f64>,
    pub p50_latency_ms: Option<u128>,
    pub p95_latency_ms: Option<u128>,
    pub statuses: BTreeMap<u16, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SweepRouteResult {
    pub key: String,
    pub method: String,
    pub path: String,
    pub persona: Option<Persona>,
    pub metrics: Metrics,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SweepReport {
    pub routes: Vec<SweepRouteResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadRouteBreakdown {
    pub key: String,
    pub method: String,
    pub path: String,
    pub persona: Option<Persona>,
    pub metrics: Metrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadGroupResult {
    pub name: String,
    pub total_requests: usize,
    pub concurrency: usize,
    pub metrics: Metrics,
    pub routes: Vec<LoadRouteBreakdown>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LoadReport {
    pub groups: Vec<LoadGroupResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioStepResult {
    pub name: String,
    pub persona: Option<Persona>,
    pub method: Option<String>,
    pub path: Option<String>,
    pub status: Option<u16>,
    pub success: bool,
    pub latency_ms: Option<u128>,
    pub details: BTreeMap<String, String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioResult {
    pub name: String,
    pub success: bool,
    pub total_latency_ms: u128,
    pub steps: Vec<ScenarioStepResult>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScenarioReport {
    pub scenarios: Vec<ScenarioResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub area: String,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTotals {
    pub discovered_routes: usize,
    pub sweep_routes: usize,
    pub load_groups: usize,
    pub scenarios_run: usize,
    pub failure_count: usize,
    pub violation_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub run_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub base_url: String,
    pub mode: String,
    pub auth_mode: String,
    pub personas: Vec<PersonaReport>,
    pub discovery: DiscoveryReport,
    pub sweep: SweepReport,
    pub load: LoadReport,
    pub scenarios: ScenarioReport,
    pub totals: RunTotals,
    pub violations: Vec<Violation>,
    pub failures: Vec<String>,
    pub skipped: Vec<String>,
}

pub fn metrics_from_latencies(
    latencies: &[u128],
    successes: usize,
    failures: usize,
    timeouts: usize,
    statuses: &BTreeMap<u16, usize>,
) -> Metrics {
    let mut sorted = latencies.to_vec();
    sorted.sort_unstable();
    let request_count = successes + failures;
    let min_latency_ms = sorted.first().copied();
    let max_latency_ms = sorted.last().copied();
    let avg_latency_ms = if sorted.is_empty() {
        None
    } else {
        Some(sorted.iter().sum::<u128>() as f64 / sorted.len() as f64)
    };
    let p50_latency_ms = percentile(&sorted, 50.0);
    let p95_latency_ms = percentile(&sorted, 95.0);

    Metrics {
        request_count,
        success_count: successes,
        failure_count: failures,
        timeout_count: timeouts,
        min_latency_ms,
        max_latency_ms,
        avg_latency_ms,
        p50_latency_ms,
        p95_latency_ms,
        statuses: statuses.clone(),
    }
}

fn percentile(values: &[u128], percentile: f64) -> Option<u128> {
    if values.is_empty() {
        return None;
    }
    let rank = ((percentile / 100.0) * (values.len().saturating_sub(1)) as f64).round() as usize;
    values.get(rank).copied()
}
