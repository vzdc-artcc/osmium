mod auth;
mod cli;
mod config;
mod http;
mod load;
mod openapi;
mod personas;
mod report;
mod scenarios;
mod sweep;

use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use serde_json::to_string_pretty;
use tokio::fs;
use uuid::Uuid;

use crate::{
    auth::bootstrap_personas,
    cli::Cli,
    config::CommandKind,
    load::run_load,
    openapi::{DiscoveryContext, discover_routes},
    report::{RunReport, RunTotals, Violation},
    scenarios::run_selected_scenarios,
    sweep::run_sweep,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = cli.into_config()?;
    let started_at = Utc::now();
    let run_id = Uuid::new_v4().to_string();

    println!(
        "[bootstrap] mode={} base_url={}",
        config.command.as_str(),
        config.base_url
    );
    let auth = bootstrap_personas(&config).await;
    if config.fail_fast && !auth.failures.is_empty() && auth.sessions.is_empty() {
        anyhow::bail!("auth bootstrap failed: {}", auth.failures.join("; "));
    }

    let discovery_ctx = DiscoveryContext::default();
    println!("[discover] fetching OpenAPI");
    let (routes, discovery) = discover_routes(&config, &auth.sessions, &discovery_ctx).await?;
    print_discovery_summary(&discovery);

    let mut sweep_report = report::SweepReport::default();
    let mut load_report = report::LoadReport::default();
    let mut scenario_report = report::ScenarioReport::default();
    let mut failures = auth.failures.clone();
    let mut skipped = discovery
        .routes
        .iter()
        .filter_map(|route| {
            route
                .skip_reason
                .clone()
                .map(|reason| format!("{} {}: {reason}", route.method, route.path))
        })
        .collect::<Vec<_>>();

    match config.command {
        CommandKind::Run => {
            println!("[sweep] starting baseline timing sweep");
            sweep_report = run_sweep(&config, &routes, &auth.sessions, &discovery_ctx).await?;
            println!("[load] starting burst/load phase");
            load_report = run_load(&config, &routes, &auth.sessions, &discovery_ctx, false).await?;
            println!("[load] starting spike phase");
            let spike_report =
                run_load(&config, &routes, &auth.sessions, &discovery_ctx, true).await?;
            load_report.groups.extend(spike_report.groups);
            println!("[scenario] starting realistic scenario phase");
            let (report, _) = run_selected_scenarios(&config, &auth.sessions).await?;
            scenario_report = report;
        }
        CommandKind::Sweep => {
            sweep_report = run_sweep(&config, &routes, &auth.sessions, &discovery_ctx).await?;
        }
        CommandKind::Load => {
            load_report = run_load(&config, &routes, &auth.sessions, &discovery_ctx, false).await?;
        }
        CommandKind::Scenario => {
            let (report, _) = run_selected_scenarios(&config, &auth.sessions).await?;
            scenario_report = report;
        }
        CommandKind::Discover => {}
    }

    failures.extend(
        sweep_report
            .routes
            .iter()
            .flat_map(|route| route.errors.iter().cloned()),
    );
    failures.extend(
        load_report
            .groups
            .iter()
            .flat_map(|group| group.errors.iter().cloned()),
    );
    failures.extend(
        scenario_report
            .scenarios
            .iter()
            .flat_map(|scenario| scenario.errors.iter().cloned()),
    );

    let violations = collect_violations(&config, &sweep_report, &load_report, &scenario_report);
    let totals = RunTotals {
        discovered_routes: discovery.total_routes,
        sweep_routes: sweep_report.routes.len(),
        load_groups: load_report.groups.len(),
        scenarios_run: scenario_report.scenarios.len(),
        failure_count: failures.len(),
        violation_count: violations.len(),
    };
    let finished_at = Utc::now();

    let report = RunReport {
        run_id: run_id.clone(),
        started_at,
        finished_at,
        base_url: config.base_url.clone(),
        mode: config.command.as_str().to_string(),
        auth_mode: config.auth_mode.as_str().to_string(),
        personas: auth.reports,
        discovery,
        sweep: sweep_report,
        load: load_report,
        scenarios: scenario_report,
        totals,
        violations,
        failures,
        skipped: std::mem::take(&mut skipped),
    };

    emit_console_summary(&report);
    write_report(&config.report_dir, config.json_out.as_ref(), &report).await?;

    if !report.violations.is_empty() {
        std::process::exit(2);
    }
    if report.personas.iter().all(|persona| !persona.available) && report.mode != "discover" {
        std::process::exit(3);
    }

    Ok(())
}

fn collect_violations(
    config: &config::Config,
    sweep_report: &report::SweepReport,
    load_report: &report::LoadReport,
    scenario_report: &report::ScenarioReport,
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for route in &sweep_report.routes {
        if route.metrics.failure_count > 0 {
            violations.push(Violation {
                area: "sweep".to_string(),
                target: route.key.clone(),
                message: format!("{} failures during sweep", route.metrics.failure_count),
            });
        }
        if route.metrics.timeout_count > 0 {
            violations.push(Violation {
                area: "sweep".to_string(),
                target: route.key.clone(),
                message: format!("{} timeouts during sweep", route.metrics.timeout_count),
            });
        }
        if route.metrics.p95_latency_ms.unwrap_or(0) > config.latency_threshold_ms {
            violations.push(Violation {
                area: "sweep".to_string(),
                target: route.key.clone(),
                message: format!(
                    "p95 {}ms exceeded threshold {}ms",
                    route.metrics.p95_latency_ms.unwrap_or(0),
                    config.latency_threshold_ms
                ),
            });
        }
    }

    for group in &load_report.groups {
        let failure_rate = if group.metrics.request_count == 0 {
            0.0
        } else {
            group.metrics.failure_count as f64 / group.metrics.request_count as f64
        };
        if failure_rate > 0.01 {
            violations.push(Violation {
                area: "load".to_string(),
                target: group.name.clone(),
                message: format!("failure rate {:.2}% exceeded 1%", failure_rate * 100.0),
            });
        }
        if group.metrics.timeout_count > 0 {
            violations.push(Violation {
                area: "load".to_string(),
                target: group.name.clone(),
                message: format!("{} timeouts during load", group.metrics.timeout_count),
            });
        }
        if group.metrics.p95_latency_ms.unwrap_or(0) > config.latency_threshold_ms {
            violations.push(Violation {
                area: "load".to_string(),
                target: group.name.clone(),
                message: format!(
                    "p95 {}ms exceeded threshold {}ms",
                    group.metrics.p95_latency_ms.unwrap_or(0),
                    config.latency_threshold_ms
                ),
            });
        }
    }

    for scenario in &scenario_report.scenarios {
        if !scenario.success {
            violations.push(Violation {
                area: "scenario".to_string(),
                target: scenario.name.clone(),
                message: "scenario failed".to_string(),
            });
        }
    }

    violations
}

async fn write_report(
    report_dir: &PathBuf,
    explicit_path: Option<&PathBuf>,
    report: &RunReport,
) -> Result<()> {
    let path = if let Some(path) = explicit_path {
        path.clone()
    } else {
        let file_name = format!(
            "api-load-report-{}.json",
            report.finished_at.format("%Y%m%dT%H%M%SZ")
        );
        report_dir.join(file_name)
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let json = to_string_pretty(report)?;
    fs::write(&path, json).await?;
    println!("[report] wrote {}", path.display());
    Ok(())
}

fn print_discovery_summary(report: &report::DiscoveryReport) {
    println!(
        "[discover] total={} included={} skipped={}",
        report.total_routes, report.included_routes, report.skipped_routes
    );
}

fn emit_console_summary(report: &RunReport) {
    println!();
    println!("Run: {}", report.run_id);
    println!("Mode: {}", report.mode);
    println!("Base URL: {}", report.base_url);
    println!("Auth mode: {}", report.auth_mode);
    println!("Personas:");
    for persona in &report.personas {
        println!(
            "  - {} available={} auth={:?} cid={:?} user_id={:?}",
            persona.persona, persona.available, persona.auth_kind, persona.cid, persona.user_id
        );
    }
    println!(
        "Discovery: total={} included={} skipped={}",
        report.discovery.total_routes,
        report.discovery.included_routes,
        report.discovery.skipped_routes
    );
    if !report.sweep.routes.is_empty() {
        println!("Sweep leaderboard:");
        for route in report.sweep.routes.iter().take(5) {
            println!(
                "  - {} p95={:?} failures={}",
                route.key, route.metrics.p95_latency_ms, route.metrics.failure_count
            );
        }
    }
    if !report.load.groups.is_empty() {
        println!("Load leaderboard:");
        for group in &report.load.groups {
            println!(
                "  - {} p95={:?} failures={} requests={}",
                group.name,
                group.metrics.p95_latency_ms,
                group.metrics.failure_count,
                group.metrics.request_count
            );
        }
    }
    if !report.scenarios.scenarios.is_empty() {
        println!("Scenarios:");
        for scenario in &report.scenarios.scenarios {
            println!(
                "  - {} success={} total={}ms",
                scenario.name, scenario.success, scenario.total_latency_ms
            );
        }
    }
    if !report.violations.is_empty() {
        println!("Violations:");
        for violation in &report.violations {
            println!(
                "  - [{}] {}: {}",
                violation.area, violation.target, violation.message
            );
        }
    }
    if !report.failures.is_empty() {
        println!("Failures:");
        for failure in report.failures.iter().take(10) {
            println!("  - {failure}");
        }
    }
    if !report.skipped.is_empty() {
        println!("Skipped:");
        for skipped in report.skipped.iter().take(10) {
            println!("  - {skipped}");
        }
    }
}
