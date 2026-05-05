use std::{env, path::PathBuf};

use anyhow::{Result, bail};
use regex::Regex;
use serde::Serialize;

use crate::personas::Persona;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    Run,
    Sweep,
    Load,
    Scenario,
    Discover,
}

impl CommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CommandKind::Run => "run",
            CommandKind::Sweep => "sweep",
            CommandKind::Load => "load",
            CommandKind::Scenario => "scenario",
            CommandKind::Discover => "discover",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthMode {
    DevLogin,
    Bearer,
    Hybrid,
}

impl AuthMode {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dev-login" => Ok(AuthMode::DevLogin),
            "bearer" => Ok(AuthMode::Bearer),
            "hybrid" => Ok(AuthMode::Hybrid),
            other => bail!("unsupported auth mode '{other}'"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AuthMode::DevLogin => "dev-login",
            AuthMode::Bearer => "bearer",
            AuthMode::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub command: CommandKind,
    pub base_url: String,
    pub report_dir: PathBuf,
    pub seed_dev_data: bool,
    pub auth_mode: AuthMode,
    pub include_tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub include_methods: Vec<String>,
    pub exclude_path_regex: Option<Regex>,
    pub scenarios: Vec<String>,
    pub personas: Vec<Persona>,
    pub burst_requests: usize,
    pub burst_concurrency: usize,
    pub route_timeout_ms: u64,
    pub fail_fast: bool,
    pub json_out: Option<PathBuf>,
    pub unsafe_mutations: bool,
    pub sweep_iterations: usize,
    pub latency_threshold_ms: u128,
}

#[derive(Debug, Clone, Default)]
pub struct RawConfig {
    pub command: Option<CommandKind>,
    pub base_url: Option<String>,
    pub report_dir: Option<PathBuf>,
    pub seed_dev_data: Option<bool>,
    pub auth_mode: Option<AuthMode>,
    pub include_tags: Option<Vec<String>>,
    pub exclude_tags: Option<Vec<String>>,
    pub include_methods: Option<Vec<String>>,
    pub exclude_path_regex: Option<String>,
    pub scenarios: Option<Vec<String>>,
    pub personas: Option<Vec<Persona>>,
    pub burst_requests: Option<usize>,
    pub burst_concurrency: Option<usize>,
    pub route_timeout_ms: Option<u64>,
    pub fail_fast: Option<bool>,
    pub json_out: Option<PathBuf>,
    pub unsafe_mutations: Option<bool>,
}

impl Config {
    pub fn from_raw(raw: RawConfig) -> Result<Self> {
        let base_url = raw
            .base_url
            .or_else(|| env::var("API_LOAD_BASE_URL").ok())
            .unwrap_or_else(|| "http://127.0.0.1:3000".to_string())
            .trim_end_matches('/')
            .to_string();
        let report_dir = raw
            .report_dir
            .or_else(|| env::var("API_LOAD_REPORT_DIR").ok().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("tools/api-load-tester/reports"));
        let auth_mode = raw
            .auth_mode
            .or_else(|| {
                env::var("API_LOAD_AUTH_MODE")
                    .ok()
                    .and_then(|value| AuthMode::parse(&value).ok())
            })
            .unwrap_or(AuthMode::Hybrid);
        let burst_requests = raw
            .burst_requests
            .or_else(|| {
                env::var("API_LOAD_BURST_REQUESTS")
                    .ok()
                    .and_then(|value| value.parse().ok())
            })
            .unwrap_or(100);
        let burst_concurrency = raw
            .burst_concurrency
            .or_else(|| {
                env::var("API_LOAD_BURST_CONCURRENCY")
                    .ok()
                    .and_then(|value| value.parse().ok())
            })
            .unwrap_or(10);
        let route_timeout_ms = raw
            .route_timeout_ms
            .or_else(|| {
                env::var("API_LOAD_ROUTE_TIMEOUT_MS")
                    .ok()
                    .and_then(|value| value.parse().ok())
            })
            .unwrap_or(10_000);
        let include_tags = raw.include_tags.unwrap_or_default();
        let exclude_tags = raw.exclude_tags.unwrap_or_default();
        let include_methods = raw.include_methods.unwrap_or_default();
        let exclude_path_regex = match raw.exclude_path_regex {
            Some(pattern) => Some(Regex::new(&pattern)?),
            None => None,
        };
        let personas = raw
            .personas
            .unwrap_or_else(|| vec![Persona::Staff, Persona::Student, Persona::Trainer]);
        let command = raw.command.unwrap_or(CommandKind::Run);
        let is_local_dev = base_url.contains("127.0.0.1") || base_url.contains("localhost");
        let seed_dev_data = raw.seed_dev_data.unwrap_or(is_local_dev);
        let scenarios = raw.scenarios.unwrap_or_else(|| {
            vec![
                "event-lifecycle".to_string(),
                "event-signup".to_string(),
                "self-service-profile".to_string(),
                "workflow-mix".to_string(),
                "mixed-normal-traffic".to_string(),
            ]
        });

        Ok(Self {
            command,
            base_url,
            report_dir,
            seed_dev_data,
            auth_mode,
            include_tags,
            exclude_tags,
            include_methods,
            exclude_path_regex,
            scenarios,
            personas,
            burst_requests,
            burst_concurrency,
            route_timeout_ms,
            fail_fast: raw.fail_fast.unwrap_or(false),
            json_out: raw.json_out,
            unsafe_mutations: raw.unsafe_mutations.unwrap_or(false),
            sweep_iterations: 3,
            latency_threshold_ms: 2_000,
        })
    }

    pub fn is_local_dev_target(&self) -> bool {
        self.base_url.contains("127.0.0.1") || self.base_url.contains("localhost")
    }
}

pub fn parse_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
