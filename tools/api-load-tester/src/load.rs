use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use futures::{StreamExt, stream};

use crate::{
    auth::PersonaSession,
    config::Config,
    http::{execute_request, update_statuses},
    openapi::{DiscoveredRoute, DiscoveryContext, group_routes_for_load},
    personas::Persona,
    report::{LoadGroupResult, LoadReport, LoadRouteBreakdown, metrics_from_latencies},
};

pub async fn run_load(
    config: &Config,
    routes: &[DiscoveredRoute],
    sessions: &std::collections::HashMap<Persona, PersonaSession>,
    ctx: &DiscoveryContext,
    spike_mode: bool,
) -> Result<LoadReport> {
    let mut report = LoadReport::default();
    let groups = group_routes_for_load(routes);
    let total_requests = if spike_mode {
        50
    } else {
        config.burst_requests
    };
    let concurrency = if spike_mode {
        25
    } else {
        config.burst_concurrency
    };

    for (name, group_routes) in groups {
        if group_routes.is_empty() {
            continue;
        }
        println!(
            "[load] group={} requests={} concurrency={}",
            name, total_requests, concurrency
        );
        let sessions = Arc::new(sessions.clone());
        let group_routes = Arc::new(group_routes);
        let ctx = Arc::new(ctx.clone());
        let base_url = config.base_url.clone();
        let timeout = config.route_timeout_ms;

        let results = stream::iter(0..total_requests)
            .map(|index| {
                let sessions = Arc::clone(&sessions);
                let group_routes = Arc::clone(&group_routes);
                let ctx = Arc::clone(&ctx);
                let base_url = base_url.clone();
                async move {
                    let route = &group_routes[index % group_routes.len()];
                    let template = route
                        .template
                        .as_ref()
                        .expect("included route missing template");
                    let session = route.persona.and_then(|persona| sessions.get(&persona));
                    let client = session
                        .map(|session| session.client.clone())
                        .unwrap_or_else(reqwest::Client::new);
                    let auth_header = session.and_then(|session| session.auth_header.clone());
                    let plan = template.build(config, &ctx);
                    let result =
                        execute_request(&client, &base_url, timeout, &plan, auth_header.as_deref())
                            .await;
                    (route.clone(), result)
                }
            })
            .buffer_unordered(concurrency)
            .collect::<Vec<_>>()
            .await;

        let mut group_latencies = Vec::new();
        let mut group_statuses = BTreeMap::new();
        let mut group_errors = Vec::new();
        let mut group_successes = 0usize;
        let mut group_failures = 0usize;
        let mut group_timeouts = 0usize;
        let mut per_route = BTreeMap::<
            String,
            (
                DiscoveredRoute,
                Vec<u128>,
                BTreeMap<u16, usize>,
                usize,
                usize,
                usize,
            ),
        >::new();

        for (route, outcome) in results {
            let entry = per_route.entry(route.key.clone()).or_insert_with(|| {
                (
                    route.clone(),
                    Vec::new(),
                    BTreeMap::new(),
                    0usize,
                    0usize,
                    0usize,
                )
            });
            match outcome {
                Ok(outcome) => {
                    entry.1.push(outcome.latency_ms);
                    group_latencies.push(outcome.latency_ms);
                    update_statuses(&mut entry.2, outcome.status);
                    update_statuses(&mut group_statuses, outcome.status);
                    if outcome.timed_out {
                        entry.5 += 1;
                        entry.4 += 1;
                        group_timeouts += 1;
                        group_failures += 1;
                        group_errors.push(format!("timeout for {}", route.key));
                    } else if matches!(outcome.status, Some(status) if status.is_success()) {
                        entry.3 += 1;
                        group_successes += 1;
                    } else {
                        entry.4 += 1;
                        group_failures += 1;
                        group_errors.push(format!(
                            "{} status {}",
                            route.key,
                            outcome
                                .status
                                .map(|status| status.as_u16().to_string())
                                .unwrap_or_else(|| "none".to_string())
                        ));
                    }
                }
                Err(error) => {
                    entry.4 += 1;
                    group_failures += 1;
                    group_errors.push(format!("{} error {error}", route.key));
                }
            }
        }

        let route_breakdowns = per_route
            .into_values()
            .map(
                |(route, latencies, statuses, successes, failures, timeouts)| LoadRouteBreakdown {
                    key: route.key,
                    method: route.method.as_str().to_string(),
                    path: route.path,
                    persona: route.persona,
                    metrics: metrics_from_latencies(
                        &latencies, successes, failures, timeouts, &statuses,
                    ),
                },
            )
            .collect::<Vec<_>>();

        report.groups.push(LoadGroupResult {
            name,
            total_requests,
            concurrency,
            metrics: metrics_from_latencies(
                &group_latencies,
                group_successes,
                group_failures,
                group_timeouts,
                &group_statuses,
            ),
            routes: route_breakdowns,
            errors: group_errors,
        });
    }

    Ok(report)
}
