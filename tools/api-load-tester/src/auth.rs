use std::{collections::HashMap, env};

use anyhow::{Result, anyhow};
use reqwest::{Client, redirect::Policy};
use serde_json::Value;

use crate::{
    config::{AuthMode, Config},
    http::{RequestPlan, execute_request},
    personas::Persona,
    report::{AuthKind, PersonaReport},
};

#[derive(Debug, Clone)]
pub struct PersonaSession {
    pub persona: Persona,
    pub auth_kind: AuthKind,
    pub client: Client,
    pub auth_header: Option<String>,
    pub cid: Option<i64>,
    pub user_id: Option<String>,
    pub display_name: Option<String>,
    pub validation_route: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Default)]
pub struct AuthBootstrap {
    pub sessions: HashMap<Persona, PersonaSession>,
    pub reports: Vec<PersonaReport>,
    pub failures: Vec<String>,
}

pub async fn bootstrap_personas(config: &Config) -> AuthBootstrap {
    let mut result = AuthBootstrap::default();

    if config.seed_dev_data && matches!(config.auth_mode, AuthMode::DevLogin | AuthMode::Hybrid) {
        match seed_dev_data(config).await {
            Ok(()) => println!("[bootstrap] seeded dev data"),
            Err(error) => {
                let message = format!("failed to seed dev data: {error:#}");
                println!("[bootstrap] {message}");
                result.failures.push(message);
            }
        }
    }

    for persona in &config.personas {
        match bootstrap_persona(config, *persona).await {
            Ok(Some(session)) => {
                result.reports.push(PersonaReport {
                    persona: *persona,
                    available: true,
                    auth_kind: Some(session.auth_kind.clone()),
                    cid: session.cid,
                    user_id: session.user_id.clone(),
                    display_name: session.display_name.clone(),
                    validation_route: Some(session.validation_route.clone()),
                    notes: session.notes.clone(),
                });
                result.sessions.insert(*persona, session);
            }
            Ok(None) => {
                result.reports.push(PersonaReport {
                    persona: *persona,
                    available: false,
                    auth_kind: None,
                    cid: persona.default_cid(),
                    user_id: None,
                    display_name: None,
                    validation_route: None,
                    notes: vec!["no usable auth configured".to_string()],
                });
            }
            Err(error) => {
                let message = format!("persona {persona} auth failed: {error:#}");
                result.failures.push(message.clone());
                result.reports.push(PersonaReport {
                    persona: *persona,
                    available: false,
                    auth_kind: None,
                    cid: persona.default_cid(),
                    user_id: None,
                    display_name: None,
                    validation_route: None,
                    notes: vec![message],
                });
            }
        }
    }

    result
}

async fn bootstrap_persona(config: &Config, persona: Persona) -> Result<Option<PersonaSession>> {
    if matches!(config.auth_mode, AuthMode::DevLogin | AuthMode::Hybrid) {
        if let Some(session) = try_dev_login(config, persona).await? {
            return Ok(Some(session));
        }
    }

    if matches!(config.auth_mode, AuthMode::Bearer | AuthMode::Hybrid) {
        if let Some(session) = try_bearer(config, persona).await? {
            return Ok(Some(session));
        }
    }

    Ok(None)
}

async fn try_dev_login(config: &Config, persona: Persona) -> Result<Option<PersonaSession>> {
    let Some(cid) = persona.default_cid() else {
        return Ok(None);
    };
    let client = Client::builder()
        .cookie_store(true)
        .redirect(Policy::limited(10))
        .build()?;

    let path = format!("/api/v1/auth/login/as/{cid}");
    let outcome = execute_request(
        &client,
        &config.base_url,
        config.route_timeout_ms,
        &RequestPlan {
            method: reqwest::Method::GET,
            path,
            body: None,
            query: Vec::new(),
        },
        None,
    )
    .await?;

    let status = outcome
        .status
        .ok_or_else(|| anyhow!("missing status from dev login"))?;
    if !status.is_success() {
        return Ok(None);
    }

    let me = execute_request(
        &client,
        &config.base_url,
        config.route_timeout_ms,
        &RequestPlan {
            method: reqwest::Method::GET,
            path: "/api/v1/me".to_string(),
            body: None,
            query: Vec::new(),
        },
        None,
    )
    .await?;
    let body = me
        .body_json
        .ok_or_else(|| anyhow!("missing /api/v1/me json for {persona}"))?;

    Ok(Some(PersonaSession {
        persona,
        auth_kind: AuthKind::CookieSession,
        client,
        auth_header: None,
        cid: body.get("cid").and_then(Value::as_i64),
        user_id: body
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        display_name: body
            .get("display_name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        validation_route: "/api/v1/me".to_string(),
        notes: vec!["authenticated with dev login-as session".to_string()],
    }))
}

async fn try_bearer(config: &Config, persona: Persona) -> Result<Option<PersonaSession>> {
    let Ok(token) = env::var(persona.env_key()) else {
        return Ok(None);
    };
    let client = Client::builder().cookie_store(true).build()?;
    let auth_header = format!("Bearer {token}");
    let me_plan = RequestPlan {
        method: reqwest::Method::GET,
        path: "/api/v1/me".to_string(),
        body: None,
        query: Vec::new(),
    };
    let service_plan = RequestPlan {
        method: reqwest::Method::GET,
        path: "/api/v1/auth/service-account/me".to_string(),
        body: None,
        query: Vec::new(),
    };

    let me = execute_request(
        &client,
        &config.base_url,
        config.route_timeout_ms,
        &me_plan,
        Some(&auth_header),
    )
    .await?;
    if matches!(me.status, Some(status) if status.is_success()) {
        let body = me.body_json.unwrap_or(Value::Null);
        return Ok(Some(PersonaSession {
            persona,
            auth_kind: AuthKind::Bearer,
            client,
            auth_header: Some(auth_header),
            cid: body.get("cid").and_then(Value::as_i64),
            user_id: body
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            display_name: body
                .get("display_name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            validation_route: "/api/v1/me".to_string(),
            notes: vec!["authenticated with bearer token".to_string()],
        }));
    }

    let service = execute_request(
        &client,
        &config.base_url,
        config.route_timeout_ms,
        &service_plan,
        Some(&auth_header),
    )
    .await?;
    if matches!(service.status, Some(status) if status.is_success()) {
        let body = service.body_json.unwrap_or(Value::Null);
        return Ok(Some(PersonaSession {
            persona,
            auth_kind: AuthKind::Bearer,
            client,
            auth_header: Some(auth_header),
            cid: None,
            user_id: body
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            display_name: body
                .get("name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            validation_route: "/api/v1/auth/service-account/me".to_string(),
            notes: vec!["authenticated with bearer token as machine identity".to_string()],
        }));
    }

    Ok(None)
}

async fn seed_dev_data(config: &Config) -> Result<()> {
    let client = Client::builder().cookie_store(true).build()?;
    let outcome = execute_request(
        &client,
        &config.base_url,
        config.route_timeout_ms,
        &RequestPlan {
            method: reqwest::Method::POST,
            path: "/api/v1/dev/seed".to_string(),
            body: None,
            query: Vec::new(),
        },
        None,
    )
    .await?;

    match outcome.status {
        Some(status) if status.is_success() => Ok(()),
        Some(status) => Err(anyhow!(
            "seed returned status {status} body={}",
            outcome.body_text
        )),
        None => Err(anyhow!("seed did not return an HTTP status")),
    }
}
