use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use anyhow::{Result, anyhow, bail};
use futures::future::join3;
use reqwest::Method;
use serde_json::{Value, json};
use tokio::time::{Instant, sleep};

use crate::{
    auth::PersonaSession,
    config::Config,
    http::{RequestPlan, execute_request},
    personas::Persona,
    report::{ScenarioReport, ScenarioResult, ScenarioStepResult},
};

#[derive(Debug, Clone, Default)]
pub struct ScenarioContext {
    pub event_id: Option<String>,
    pub event_position_id: Option<String>,
    pub tmi_id: Option<String>,
    pub staffing_request_id: Option<String>,
    pub loa_id: Option<String>,
    pub teamspeak_identity_id: Option<String>,
    pub student_user_id: Option<String>,
}

pub async fn run_selected_scenarios(
    config: &Config,
    sessions: &HashMap<Persona, PersonaSession>,
) -> Result<(ScenarioReport, ScenarioContext)> {
    let mut report = ScenarioReport::default();
    let mut ctx = ScenarioContext::default();

    for name in &config.scenarios {
        let result = match name.as_str() {
            "event-lifecycle" => run_event_lifecycle(config, sessions, &mut ctx).await?,
            "event-signup" => run_event_signup(config, sessions, &mut ctx).await?,
            "self-service-profile" => run_self_service_profile(config, sessions, &mut ctx).await?,
            "workflow-mix" => run_workflow_mix(config, sessions, &mut ctx).await?,
            "mixed-normal-traffic" => run_mixed_normal_traffic(config, sessions, &mut ctx).await?,
            other => ScenarioResult {
                name: other.to_string(),
                success: false,
                total_latency_ms: 0,
                steps: Vec::new(),
                errors: vec![format!("unknown scenario '{other}'")],
            },
        };
        report.scenarios.push(result);
    }

    Ok((report, ctx))
}

async fn run_event_lifecycle(
    config: &Config,
    sessions: &HashMap<Persona, PersonaSession>,
    ctx: &mut ScenarioContext,
) -> Result<ScenarioResult> {
    let mut scenario = ScenarioBuilder::new("event-lifecycle");
    let staff = require_session(sessions, Persona::Staff)?;
    let now = chrono::Utc::now();
    let create_payload = json!({
        "title": format!("Scenario Event {}", now.timestamp_millis()),
        "event_type": "HOME",
        "host": "API Load Tester",
        "description": "scenario event lifecycle",
        "starts_at": (now + chrono::Duration::days(10)).to_rfc3339(),
        "ends_at": (now + chrono::Duration::days(10) + chrono::Duration::hours(3)).to_rfc3339()
    });
    let create = scenario
        .request(
            config,
            Some(staff),
            "create event",
            RequestPlan {
                method: Method::POST,
                path: "/api/v1/events".to_string(),
                body: Some(create_payload),
                query: Vec::new(),
            },
        )
        .await?;
    let event_id = require_string(&create, "id")?;
    ctx.event_id = Some(event_id.clone());

    scenario
        .request(
            config,
            Some(staff),
            "fetch created event",
            RequestPlan {
                method: Method::GET,
                path: format!("/api/v1/events/{event_id}"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "update event fields",
            RequestPlan {
                method: Method::PATCH,
                path: format!("/api/v1/events/{event_id}"),
                body: Some(json!({
                    "title": format!("Scenario Event Updated {}", now.timestamp_millis()),
                    "description": "scenario update",
                    "published": true
                })),
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "update preset positions",
            RequestPlan {
                method: Method::PUT,
                path: format!("/api/v1/events/{event_id}/preset-positions"),
                body: Some(json!({"preset_positions": ["DCA_GND", "DCA_TWR", "PCT_APP"]})),
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "fetch preset positions",
            RequestPlan {
                method: Method::GET,
                path: format!("/api/v1/events/{event_id}/preset-positions"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    let tmi = scenario
        .request(
            config,
            Some(staff),
            "create tmi entry",
            RequestPlan {
                method: Method::POST,
                path: format!("/api/v1/events/{event_id}/tmis"),
                body: Some(json!({
                    "tmi_type": "MIT",
                    "start_time": (now + chrono::Duration::days(9)).to_rfc3339(),
                    "notes": "scenario tmi"
                })),
                query: Vec::new(),
            },
        )
        .await?;
    ctx.tmi_id = require_optional_string(&tmi, "id");
    scenario
        .request(
            config,
            Some(staff),
            "fetch tmi list",
            RequestPlan {
                method: Method::GET,
                path: format!("/api/v1/events/{event_id}/tmis"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "fetch ops plan",
            RequestPlan {
                method: Method::GET,
                path: format!("/api/v1/events/{event_id}/ops-plan"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "update ops plan",
            RequestPlan {
                method: Method::PATCH,
                path: format!("/api/v1/events/{event_id}/ops-plan"),
                body: Some(json!({
                    "featured_fields": ["airports", "routes"],
                    "preset_positions": ["DCA_GND", "IAD_APP"],
                    "featured_field_configs": {"airports": ["KDCA", "KIAD"]},
                    "tmis": "scenario tmi text",
                    "ops_free_text": "scenario ops note",
                    "ops_plan_published": true,
                    "enable_buffer_times": true
                })),
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "lock positions",
            RequestPlan {
                method: Method::POST,
                path: format!("/api/v1/events/{event_id}/positions/lock"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "unlock positions",
            RequestPlan {
                method: Method::POST,
                path: format!("/api/v1/events/{event_id}/positions/unlock"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;

    Ok(scenario.finish())
}

async fn run_event_signup(
    config: &Config,
    sessions: &HashMap<Persona, PersonaSession>,
    ctx: &mut ScenarioContext,
) -> Result<ScenarioResult> {
    let mut scenario = ScenarioBuilder::new("event-signup");
    let staff = require_session(sessions, Persona::Staff)?;
    let student = require_session(sessions, Persona::Student)?;
    let event_id = ctx
        .event_id
        .clone()
        .unwrap_or_else(|| "seed-event-1".to_string());

    let create_position = scenario
        .request(
            config,
            Some(staff),
            "staff creates event position",
            RequestPlan {
                method: Method::POST,
                path: format!("/api/v1/events/{event_id}/positions"),
                body: Some(json!({
                    "callsign": "IAD_APP",
                    "requested_slot": 2
                })),
                query: Vec::new(),
            },
        )
        .await?;
    ctx.event_position_id = require_optional_string(&create_position, "id");

    scenario
        .request(
            config,
            Some(student),
            "student lists event positions",
            RequestPlan {
                method: Method::GET,
                path: format!("/api/v1/events/{event_id}/positions"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    let signup = scenario
        .request(
            config,
            Some(student),
            "student signs up for position",
            RequestPlan {
                method: Method::POST,
                path: format!("/api/v1/events/{event_id}/positions"),
                body: Some(json!({
                    "callsign": "DCA_DEL",
                    "requested_slot": 3
                })),
                query: Vec::new(),
            },
        )
        .await?;
    ctx.event_position_id =
        require_optional_string(&signup, "id").or(ctx.event_position_id.clone());
    scenario
        .request(
            config,
            Some(student),
            "student re-reads positions",
            RequestPlan {
                method: Method::GET,
                path: format!("/api/v1/events/{event_id}/positions"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "staff reads event detail",
            RequestPlan {
                method: Method::GET,
                path: format!("/api/v1/events/{event_id}"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "staff publishes positions",
            RequestPlan {
                method: Method::POST,
                path: format!("/api/v1/events/{event_id}/positions/publish"),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;

    Ok(scenario.finish())
}

async fn run_self_service_profile(
    config: &Config,
    sessions: &HashMap<Persona, PersonaSession>,
    ctx: &mut ScenarioContext,
) -> Result<ScenarioResult> {
    let mut scenario = ScenarioBuilder::new("self-service-profile");
    let student = require_session(sessions, Persona::Student)?;
    let now = chrono::Utc::now().timestamp_millis();

    let me = scenario
        .request(
            config,
            Some(student),
            "get me",
            RequestPlan {
                method: Method::GET,
                path: "/api/v1/me".to_string(),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    ctx.student_user_id = require_optional_string(&me, "id");

    scenario
        .request(
            config,
            Some(student),
            "patch profile",
            RequestPlan {
                method: Method::PATCH,
                path: "/api/v1/me".to_string(),
                body: Some(json!({
                    "preferred_name": format!("Scenario Student {now}"),
                    "timezone": "America/Chicago",
                    "bio": format!("Scenario bio {now}"),
                    "receive_event_notifications": true
                })),
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(student),
            "get me again",
            RequestPlan {
                method: Method::GET,
                path: "/api/v1/me".to_string(),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    let create_uid = scenario
        .request(
            config,
            Some(student),
            "create teamspeak uid",
            RequestPlan {
                method: Method::POST,
                path: "/api/v1/me/teamspeak-uids".to_string(),
                body: Some(json!({
                    "uid": format!("SCENARIO-TS-{now}")
                })),
                query: Vec::new(),
            },
        )
        .await?;
    ctx.teamspeak_identity_id = require_optional_string(&create_uid, "id");
    scenario
        .request(
            config,
            Some(student),
            "list teamspeak uids",
            RequestPlan {
                method: Method::GET,
                path: "/api/v1/me/teamspeak-uids".to_string(),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;

    if let Some(identity_id) = ctx.teamspeak_identity_id.clone() {
        scenario
            .request(
                config,
                Some(student),
                "delete teamspeak uid",
                RequestPlan {
                    method: Method::DELETE,
                    path: format!("/api/v1/me/teamspeak-uids/{identity_id}"),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
    }

    scenario
        .request(
            config,
            Some(student),
            "final list verification",
            RequestPlan {
                method: Method::GET,
                path: "/api/v1/me/teamspeak-uids".to_string(),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;

    Ok(scenario.finish())
}

async fn run_workflow_mix(
    config: &Config,
    sessions: &HashMap<Persona, PersonaSession>,
    ctx: &mut ScenarioContext,
) -> Result<ScenarioResult> {
    let mut scenario = ScenarioBuilder::new("workflow-mix");
    let student = require_session(sessions, Persona::Student)?;
    let staff = require_session(sessions, Persona::Staff)?;
    let now = chrono::Utc::now();

    let staffing = scenario
        .request(
            config,
            Some(student),
            "student creates staffing request",
            RequestPlan {
                method: Method::POST,
                path: "/api/v1/staffing-requests/me".to_string(),
                body: Some(json!({
                    "name": format!("Scenario Staffing {}", now.timestamp_millis()),
                    "description": "Need additional mentoring coverage"
                })),
                query: Vec::new(),
            },
        )
        .await?;
    ctx.staffing_request_id = require_optional_string(&staffing, "id");
    scenario
        .request(
            config,
            Some(student),
            "student lists own staffing requests",
            RequestPlan {
                method: Method::GET,
                path: "/api/v1/staffing-requests/me".to_string(),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "staff lists admin staffing requests",
            RequestPlan {
                method: Method::GET,
                path: "/api/v1/admin/staffing-requests".to_string(),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;

    if let Some(request_id) = ctx.staffing_request_id.clone() {
        scenario
            .request(
                config,
                Some(staff),
                "staff deletes staffing request",
                RequestPlan {
                    method: Method::DELETE,
                    path: format!("/api/v1/admin/staffing-requests/{request_id}"),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
    }

    let loa = scenario
        .request(
            config,
            Some(student),
            "student creates loa",
            RequestPlan {
                method: Method::POST,
                path: "/api/v1/loa/me".to_string(),
                body: Some(json!({
                    "start": (now + chrono::Duration::days(12)).to_rfc3339(),
                    "end": (now + chrono::Duration::days(22)).to_rfc3339(),
                    "reason": "Scenario LOA request"
                })),
                query: Vec::new(),
            },
        )
        .await?;
    ctx.loa_id = require_optional_string(&loa, "id");
    scenario
        .request(
            config,
            Some(student),
            "student lists own loas",
            RequestPlan {
                method: Method::GET,
                path: "/api/v1/loa/me".to_string(),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;
    scenario
        .request(
            config,
            Some(staff),
            "staff lists admin loas",
            RequestPlan {
                method: Method::GET,
                path: "/api/v1/admin/loa".to_string(),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;

    if let Some(loa_id) = ctx.loa_id.clone() {
        scenario
            .request(
                config,
                Some(staff),
                "staff decides loa",
                RequestPlan {
                    method: Method::PATCH,
                    path: format!("/api/v1/admin/loa/{loa_id}/decision"),
                    body: Some(json!({
                        "status": "APPROVED",
                        "reason": "scenario approval"
                    })),
                    query: Vec::new(),
                },
            )
            .await?;
    }

    scenario
        .request(
            config,
            Some(student),
            "student re-reads own loas",
            RequestPlan {
                method: Method::GET,
                path: "/api/v1/loa/me".to_string(),
                body: None,
                query: Vec::new(),
            },
        )
        .await?;

    Ok(scenario.finish())
}

async fn run_mixed_normal_traffic(
    config: &Config,
    sessions: &HashMap<Persona, PersonaSession>,
    ctx: &mut ScenarioContext,
) -> Result<ScenarioResult> {
    let event_id = ctx
        .event_id
        .clone()
        .unwrap_or_else(|| "seed-event-1".to_string());
    let student = require_session(sessions, Persona::Student)?.clone();
    let trainer = require_session(sessions, Persona::Trainer)?.clone();
    let staff = require_session(sessions, Persona::Staff)?.clone();
    let name = "mixed-normal-traffic".to_string();
    let started = Instant::now();

    let student_task = async {
        let mut builder = ScenarioBuilder::new("mixed-normal-traffic/student");
        builder
            .request(
                config,
                Some(&student),
                "student profile read",
                RequestPlan {
                    method: Method::GET,
                    path: "/api/v1/me".to_string(),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
        sleep(Duration::from_millis(75)).await;
        builder
            .request(
                config,
                Some(&student),
                "student event browse",
                RequestPlan {
                    method: Method::GET,
                    path: format!("/api/v1/events/{event_id}/positions"),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
        sleep(Duration::from_millis(75)).await;
        builder
            .request(
                config,
                Some(&student),
                "student checks own loas",
                RequestPlan {
                    method: Method::GET,
                    path: "/api/v1/loa/me".to_string(),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
        Ok::<_, anyhow::Error>(builder.steps)
    };

    let trainer_task = async {
        let mut builder = ScenarioBuilder::new("mixed-normal-traffic/trainer");
        sleep(Duration::from_millis(30)).await;
        builder
            .request(
                config,
                Some(&trainer),
                "trainer reads self profile",
                RequestPlan {
                    method: Method::GET,
                    path: "/api/v1/me".to_string(),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
        sleep(Duration::from_millis(90)).await;
        builder
            .request(
                config,
                Some(&trainer),
                "trainer reads events",
                RequestPlan {
                    method: Method::GET,
                    path: "/api/v1/events".to_string(),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
        Ok::<_, anyhow::Error>(builder.steps)
    };

    let staff_task = async {
        let mut builder = ScenarioBuilder::new("mixed-normal-traffic/staff");
        sleep(Duration::from_millis(45)).await;
        builder
            .request(
                config,
                Some(&staff),
                "staff event detail read",
                RequestPlan {
                    method: Method::GET,
                    path: format!("/api/v1/events/{event_id}"),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
        sleep(Duration::from_millis(60)).await;
        builder
            .request(
                config,
                Some(&staff),
                "staff lock positions",
                RequestPlan {
                    method: Method::POST,
                    path: format!("/api/v1/events/{event_id}/positions/lock"),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
        builder
            .request(
                config,
                Some(&staff),
                "staff unlock positions",
                RequestPlan {
                    method: Method::POST,
                    path: format!("/api/v1/events/{event_id}/positions/unlock"),
                    body: None,
                    query: Vec::new(),
                },
            )
            .await?;
        Ok::<_, anyhow::Error>(builder.steps)
    };

    let (student_steps, trainer_steps, staff_steps) =
        join3(student_task, trainer_task, staff_task).await;
    let mut steps = Vec::new();
    let mut errors = Vec::new();
    let mut success = true;
    for result in [student_steps, trainer_steps, staff_steps] {
        match result {
            Ok(mut group_steps) => steps.append(&mut group_steps),
            Err(error) => {
                success = false;
                errors.push(error.to_string());
            }
        }
    }

    Ok(ScenarioResult {
        name,
        success,
        total_latency_ms: started.elapsed().as_millis(),
        steps,
        errors,
    })
}

#[derive(Debug)]
struct ScenarioBuilder {
    name: String,
    started: Instant,
    steps: Vec<ScenarioStepResult>,
    errors: Vec<String>,
}

impl ScenarioBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            started: Instant::now(),
            steps: Vec::new(),
            errors: Vec::new(),
        }
    }

    async fn request(
        &mut self,
        config: &Config,
        session: Option<&PersonaSession>,
        name: &str,
        plan: RequestPlan,
    ) -> Result<Value> {
        println!("[scenario:{}] {}", self.name, name);
        let client = session
            .map(|session| &session.client)
            .ok_or_else(|| anyhow!("missing session"))?;
        let auth_header = session.and_then(|session| session.auth_header.as_deref());
        let outcome = execute_request(
            client,
            &config.base_url,
            config.route_timeout_ms,
            &plan,
            auth_header,
        )
        .await?;
        let success =
            !outcome.timed_out && matches!(outcome.status, Some(status) if status.is_success());
        let mut details = BTreeMap::new();
        if let Some(body_json) = &outcome.body_json {
            if let Some(id) = body_json.get("id").and_then(Value::as_str) {
                details.insert("id".to_string(), id.to_string());
            }
        }
        let error = if success {
            None
        } else {
            Some(format!(
                "status={} body={}",
                outcome
                    .status
                    .map(|status| status.as_u16().to_string())
                    .unwrap_or_else(|| "none".to_string()),
                outcome.body_text
            ))
        };
        self.steps.push(ScenarioStepResult {
            name: name.to_string(),
            persona: session.map(|session| session.persona),
            method: Some(plan.method.as_str().to_string()),
            path: Some(plan.path.clone()),
            status: outcome.status.map(|status| status.as_u16()),
            success,
            latency_ms: Some(outcome.latency_ms),
            details,
            error: error.clone(),
        });
        if let Some(error) = error {
            self.errors.push(format!("{name}: {error}"));
            bail!("{error}");
        }
        Ok(outcome.body_json.unwrap_or(Value::Null))
    }

    fn finish(self) -> ScenarioResult {
        ScenarioResult {
            name: self.name,
            success: self.errors.is_empty(),
            total_latency_ms: self.started.elapsed().as_millis(),
            steps: self.steps,
            errors: self.errors,
        }
    }
}

fn require_session(
    sessions: &HashMap<Persona, PersonaSession>,
    persona: Persona,
) -> Result<&PersonaSession> {
    sessions
        .get(&persona)
        .ok_or_else(|| anyhow!("missing required session for {persona}"))
}

fn require_string(body: &Value, key: &str) -> Result<String> {
    body.get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("response missing string field '{key}'"))
}

fn require_optional_string(body: &Value, key: &str) -> Option<String> {
    body.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}
