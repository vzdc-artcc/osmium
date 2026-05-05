use std::collections::BTreeMap;

use anyhow::Result;

use crate::{
    auth::PersonaSession,
    config::Config,
    http::{execute_request, update_statuses},
    openapi::{DiscoveredRoute, DiscoveryContext},
    personas::Persona,
    report::{SweepReport, SweepRouteResult, metrics_from_latencies},
};

pub async fn run_sweep(
    config: &Config,
    routes: &[DiscoveredRoute],
    sessions: &std::collections::HashMap<Persona, PersonaSession>,
    ctx: &DiscoveryContext,
) -> Result<SweepReport> {
    let mut report = SweepReport::default();

    for route in routes.iter().filter(|route| route.include) {
        println!("[sweep] {} {}", route.method, route.path);
        let Some(template) = &route.template else {
            continue;
        };
        let session = route.persona.and_then(|persona| sessions.get(&persona));
        let client = session.map(|session| &session.client).unwrap_or_else(|| {
            static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
            CLIENT.get_or_init(reqwest::Client::new)
        });
        let auth_header = session.and_then(|session| session.auth_header.as_deref());
        let warmup = template.build(config, ctx);
        let _ = execute_request(
            client,
            &config.base_url,
            config.route_timeout_ms,
            &warmup,
            auth_header,
        )
        .await;

        let mut latencies = Vec::new();
        let mut statuses = BTreeMap::new();
        let mut errors = Vec::new();
        let mut successes = 0usize;
        let mut failures = 0usize;
        let mut timeouts = 0usize;

        for _ in 0..config.sweep_iterations {
            let plan = template.build(config, ctx);
            match execute_request(
                client,
                &config.base_url,
                config.route_timeout_ms,
                &plan,
                auth_header,
            )
            .await
            {
                Ok(outcome) => {
                    latencies.push(outcome.latency_ms);
                    update_statuses(&mut statuses, outcome.status);
                    if outcome.timed_out {
                        timeouts += 1;
                        failures += 1;
                        errors.push(format!("timeout for {} {}", route.method, route.path));
                    } else if matches!(outcome.status, Some(status) if status.is_success()) {
                        successes += 1;
                    } else {
                        failures += 1;
                        errors.push(format!(
                            "{} {} returned status {}",
                            route.method,
                            route.path,
                            outcome
                                .status
                                .map(|status| status.as_u16().to_string())
                                .unwrap_or_else(|| "none".to_string())
                        ));
                    }
                }
                Err(error) => {
                    failures += 1;
                    errors.push(error.to_string());
                }
            }
        }

        let metrics = metrics_from_latencies(&latencies, successes, failures, timeouts, &statuses);
        println!(
            "[sweep] done {} {} avg={:.1?}ms p95={:?} failures={}",
            route.method,
            route.path,
            metrics.avg_latency_ms,
            metrics.p95_latency_ms,
            metrics.failure_count
        );
        report.routes.push(SweepRouteResult {
            key: route.key.clone(),
            method: route.method.as_str().to_string(),
            path: route.path.clone(),
            persona: route.persona,
            metrics,
            errors,
        });
    }

    report.routes.sort_by(|a, b| {
        b.metrics
            .p95_latency_ms
            .cmp(&a.metrics.p95_latency_ms)
            .then_with(|| b.metrics.failure_count.cmp(&a.metrics.failure_count))
    });

    Ok(report)
}
