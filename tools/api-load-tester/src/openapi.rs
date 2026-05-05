use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Result, anyhow};
use reqwest::Method;
use serde_json::Value;

use crate::{
    auth::PersonaSession,
    config::Config,
    http::RequestPlan,
    personas::Persona,
    report::{DiscoveredRouteRecord, DiscoveryReport},
};

#[derive(Debug, Clone)]
pub struct DiscoveryContext {
    pub seeded_event_id: String,
    pub persona_cids: HashMap<Persona, i64>,
}

impl Default for DiscoveryContext {
    fn default() -> Self {
        let mut persona_cids = HashMap::new();
        persona_cids.insert(Persona::Staff, 10000010);
        persona_cids.insert(Persona::Student, 10000011);
        persona_cids.insert(Persona::Trainer, 10000012);
        Self {
            seeded_event_id: "seed-event-1".to_string(),
            persona_cids,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredRoute {
    pub key: String,
    pub method: Method,
    pub path: String,
    pub tag: String,
    pub route_class: RouteClass,
    pub include: bool,
    pub persona: Option<Persona>,
    pub skip_reason: Option<String>,
    pub template: Option<RouteTemplate>,
}

#[derive(Debug, Clone, Copy)]
pub enum RouteClass {
    PublicRead,
    AuthenticatedRead,
    SelfServiceMutation,
    AdminMutation,
    MachineAuthOnly,
    UnsafeOrExternal,
    Unsupported,
}

impl RouteClass {
    pub fn as_str(self) -> &'static str {
        match self {
            RouteClass::PublicRead => "public_read",
            RouteClass::AuthenticatedRead => "authenticated_read",
            RouteClass::SelfServiceMutation => "self_service_mutation",
            RouteClass::AdminMutation => "admin_mutation",
            RouteClass::MachineAuthOnly => "machine_auth_only",
            RouteClass::UnsafeOrExternal => "unsafe_or_external",
            RouteClass::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone)]
pub enum RouteTemplate {
    Health,
    Ready,
    ListEvents,
    GetSeedEvent,
    ListSeedEventPositions,
    GetSeedEventOpsPlan,
    ListSeedEventTmis,
    GetSeedEventPresetPositions,
    ListUsers,
    GetStudentUser,
    ListPublications,
    Me,
    ListMyLoas,
    ListMyStaffingRequests,
    ListStudentSoloCertifications,
    CreateEvent,
    UpdateSeedEvent,
    PublishSeedEventPositions,
    LockSeedEventPositions,
    UnlockSeedEventPositions,
    UpdateSeedPresetPositions,
    UpdateSeedOpsPlan,
    CreateSeedTmi,
    PatchMe,
    CreateTeamSpeakUid,
    CreateLoa,
    CreateStaffingRequest,
    AdminListLoas,
    AdminListStaffingRequests,
}

impl RouteTemplate {
    pub fn build(&self, config: &Config, ctx: &DiscoveryContext) -> RequestPlan {
        let now = chrono::Utc::now();
        let event_start = now + chrono::Duration::days(7);
        let event_end = now + chrono::Duration::days(7) + chrono::Duration::hours(3);
        let loa_start = now + chrono::Duration::days(9);
        let loa_end = now + chrono::Duration::days(18);
        let nonce = now.timestamp_millis();
        match self {
            RouteTemplate::Health => plan(Method::GET, "/health"),
            RouteTemplate::Ready => plan(Method::GET, "/ready"),
            RouteTemplate::ListEvents => plan(Method::GET, "/api/v1/events"),
            RouteTemplate::GetSeedEvent => plan(
                Method::GET,
                &format!("/api/v1/events/{}", ctx.seeded_event_id),
            ),
            RouteTemplate::ListSeedEventPositions => plan(
                Method::GET,
                &format!("/api/v1/events/{}/positions", ctx.seeded_event_id),
            ),
            RouteTemplate::GetSeedEventOpsPlan => plan(
                Method::GET,
                &format!("/api/v1/events/{}/ops-plan", ctx.seeded_event_id),
            ),
            RouteTemplate::ListSeedEventTmis => plan(
                Method::GET,
                &format!("/api/v1/events/{}/tmis", ctx.seeded_event_id),
            ),
            RouteTemplate::GetSeedEventPresetPositions => plan(
                Method::GET,
                &format!("/api/v1/events/{}/preset-positions", ctx.seeded_event_id),
            ),
            RouteTemplate::ListUsers => plan(Method::GET, "/api/v1/user"),
            RouteTemplate::GetStudentUser => plan(
                Method::GET,
                &format!(
                    "/api/v1/user/{}",
                    ctx.persona_cids
                        .get(&Persona::Student)
                        .copied()
                        .unwrap_or(10000011)
                ),
            ),
            RouteTemplate::ListPublications => plan(Method::GET, "/api/v1/publications"),
            RouteTemplate::Me => plan(Method::GET, "/api/v1/me"),
            RouteTemplate::ListMyLoas => plan(Method::GET, "/api/v1/loa/me"),
            RouteTemplate::ListMyStaffingRequests => {
                plan(Method::GET, "/api/v1/staffing-requests/me")
            }
            RouteTemplate::ListStudentSoloCertifications => plan(
                Method::GET,
                &format!(
                    "/api/v1/users/{}/solo-certifications",
                    ctx.persona_cids
                        .get(&Persona::Student)
                        .copied()
                        .unwrap_or(10000011)
                ),
            ),
            RouteTemplate::CreateEvent => RequestPlan {
                method: Method::POST,
                path: "/api/v1/events".to_string(),
                body: Some(serde_json::json!({
                    "title": format!("Load Test Event {nonce}"),
                    "event_type": "HOME",
                    "host": "API Load Tester",
                    "description": format!("automated create event test {nonce}"),
                    "starts_at": event_start.to_rfc3339(),
                    "ends_at": event_end.to_rfc3339()
                })),
                query: Vec::new(),
            },
            RouteTemplate::UpdateSeedEvent => RequestPlan {
                method: Method::PATCH,
                path: format!("/api/v1/events/{}", ctx.seeded_event_id),
                body: Some(serde_json::json!({
                    "title": format!("Seeded Dev Event {nonce}"),
                    "description": format!("updated by api-load-tester {nonce}"),
                    "published": true
                })),
                query: Vec::new(),
            },
            RouteTemplate::PublishSeedEventPositions => plan(
                Method::POST,
                &format!("/api/v1/events/{}/positions/publish", ctx.seeded_event_id),
            ),
            RouteTemplate::LockSeedEventPositions => plan(
                Method::POST,
                &format!("/api/v1/events/{}/positions/lock", ctx.seeded_event_id),
            ),
            RouteTemplate::UnlockSeedEventPositions => plan(
                Method::POST,
                &format!("/api/v1/events/{}/positions/unlock", ctx.seeded_event_id),
            ),
            RouteTemplate::UpdateSeedPresetPositions => RequestPlan {
                method: Method::PUT,
                path: format!("/api/v1/events/{}/preset-positions", ctx.seeded_event_id),
                body: Some(serde_json::json!({
                    "preset_positions": ["DCA_GND", "DCA_TWR", "PCT_APP"]
                })),
                query: Vec::new(),
            },
            RouteTemplate::UpdateSeedOpsPlan => RequestPlan {
                method: Method::PATCH,
                path: format!("/api/v1/events/{}/ops-plan", ctx.seeded_event_id),
                body: Some(serde_json::json!({
                    "featured_fields": ["airports", "routes"],
                    "preset_positions": ["DCA_GND", "IAD_APP"],
                    "featured_field_configs": {"airports": ["KDCA", "KIAD"]},
                    "tmis": format!("MIT generated by api-load-tester {nonce}"),
                    "ops_free_text": format!("ops note {nonce}"),
                    "ops_plan_published": true,
                    "enable_buffer_times": true
                })),
                query: Vec::new(),
            },
            RouteTemplate::CreateSeedTmi => RequestPlan {
                method: Method::POST,
                path: format!("/api/v1/events/{}/tmis", ctx.seeded_event_id),
                body: Some(serde_json::json!({
                    "tmi_type": "MIT",
                    "start_time": (now + chrono::Duration::days(6)).to_rfc3339(),
                    "notes": format!("load test tmi {nonce}")
                })),
                query: Vec::new(),
            },
            RouteTemplate::PatchMe => RequestPlan {
                method: Method::PATCH,
                path: "/api/v1/me".to_string(),
                body: Some(serde_json::json!({
                    "preferred_name": format!("Load Tester {nonce}"),
                    "timezone": "America/Chicago",
                    "bio": format!("api-load-tester bio {nonce}"),
                    "receive_event_notifications": true
                })),
                query: Vec::new(),
            },
            RouteTemplate::CreateTeamSpeakUid => RequestPlan {
                method: Method::POST,
                path: "/api/v1/me/teamspeak-uids".to_string(),
                body: Some(serde_json::json!({
                    "uid": format!("LOADTEST-TS-{nonce}")
                })),
                query: Vec::new(),
            },
            RouteTemplate::CreateLoa => RequestPlan {
                method: Method::POST,
                path: "/api/v1/loa/me".to_string(),
                body: Some(serde_json::json!({
                    "start": loa_start.to_rfc3339(),
                    "end": loa_end.to_rfc3339(),
                    "reason": format!("Automated load test LOA {nonce}")
                })),
                query: Vec::new(),
            },
            RouteTemplate::CreateStaffingRequest => RequestPlan {
                method: Method::POST,
                path: "/api/v1/staffing-requests/me".to_string(),
                body: Some(serde_json::json!({
                    "name": format!("Load Test Staffing {nonce}"),
                    "description": format!("Need more coverage {nonce}")
                })),
                query: Vec::new(),
            },
            RouteTemplate::AdminListLoas => plan(Method::GET, "/api/v1/admin/loa"),
            RouteTemplate::AdminListStaffingRequests => {
                plan(Method::GET, "/api/v1/admin/staffing-requests")
            }
        }
        .with_timeout_guard(config)
    }
}

trait RequestPlanExt {
    fn with_timeout_guard(self, _config: &Config) -> Self;
}

impl RequestPlanExt for RequestPlan {
    fn with_timeout_guard(self, _config: &Config) -> Self {
        self
    }
}

fn plan(method: Method, path: &str) -> RequestPlan {
    RequestPlan {
        method,
        path: path.to_string(),
        body: None,
        query: Vec::new(),
    }
}

pub async fn discover_routes(
    config: &Config,
    sessions: &HashMap<Persona, PersonaSession>,
    ctx: &DiscoveryContext,
) -> Result<(Vec<DiscoveredRoute>, DiscoveryReport)> {
    let client = reqwest::Client::new();
    let url = format!("{}/docs/api/v1/openapi.json", config.base_url);
    let raw = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let spec: Value = serde_json::from_str(&raw)?;
    let paths = spec
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("OpenAPI spec missing paths"))?;

    let mut routes = Vec::new();
    let mut seen = HashSet::new();

    for (path, item) in paths {
        let Some(methods) = item.as_object() else {
            continue;
        };
        for (method_name, operation) in methods {
            let method = match method_name.to_ascii_uppercase().as_str() {
                "GET" => Method::GET,
                "POST" => Method::POST,
                "PATCH" => Method::PATCH,
                "PUT" => Method::PUT,
                "DELETE" => Method::DELETE,
                _ => continue,
            };
            let key = format!("{} {}", method.as_str(), path);
            if !seen.insert(key.clone()) {
                continue;
            }
            let tag = operation
                .get("tags")
                .and_then(Value::as_array)
                .and_then(|tags| tags.first())
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();

            let (route_class, template, persona, skip_reason) =
                classify_route(&method, path, &tag, config, sessions, ctx);
            let included = skip_reason.is_none() && template.is_some();
            routes.push(DiscoveredRoute {
                key,
                method,
                path: path.to_string(),
                tag,
                route_class,
                include: included,
                persona,
                skip_reason,
                template,
            });
        }
    }

    routes.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.method.as_str().cmp(b.method.as_str()))
    });

    let report_routes = routes
        .iter()
        .map(|route| DiscoveredRouteRecord {
            key: route.key.clone(),
            method: route.method.as_str().to_string(),
            path: route.path.clone(),
            tag: route.tag.clone(),
            route_class: route.route_class.as_str().to_string(),
            included: route.include,
            persona: route.persona,
            skip_reason: route.skip_reason.clone(),
        })
        .collect::<Vec<_>>();

    let report = DiscoveryReport {
        total_routes: routes.len(),
        included_routes: routes.iter().filter(|route| route.include).count(),
        skipped_routes: routes.iter().filter(|route| !route.include).count(),
        routes: report_routes,
    };

    Ok((routes, report))
}

fn classify_route(
    method: &Method,
    path: &str,
    tag: &str,
    config: &Config,
    sessions: &HashMap<Persona, PersonaSession>,
    ctx: &DiscoveryContext,
) -> (
    RouteClass,
    Option<RouteTemplate>,
    Option<Persona>,
    Option<String>,
) {
    let upper_method = method.as_str().to_ascii_uppercase();
    if !config.include_tags.is_empty()
        && !config
            .include_tags
            .iter()
            .any(|included| included.eq_ignore_ascii_case(tag))
    {
        return (
            RouteClass::Unsupported,
            None,
            None,
            Some("tag excluded by include filter".to_string()),
        );
    }
    if config
        .exclude_tags
        .iter()
        .any(|excluded| excluded.eq_ignore_ascii_case(tag))
    {
        return (
            RouteClass::Unsupported,
            None,
            None,
            Some("tag excluded by exclude filter".to_string()),
        );
    }
    if !config.include_methods.is_empty()
        && !config
            .include_methods
            .iter()
            .any(|included| included.eq_ignore_ascii_case(&upper_method))
    {
        return (
            RouteClass::Unsupported,
            None,
            None,
            Some("method excluded by include filter".to_string()),
        );
    }
    if let Some(regex) = &config.exclude_path_regex {
        if regex.is_match(path) {
            return (
                RouteClass::Unsupported,
                None,
                None,
                Some("path excluded by regex".to_string()),
            );
        }
    }

    let has_student = sessions.contains_key(&Persona::Student);
    let has_staff = sessions.contains_key(&Persona::Staff);

    match (upper_method.as_str(), path) {
        ("GET", "/health") => include(RouteClass::PublicRead, RouteTemplate::Health, None),
        ("GET", "/ready") => include(RouteClass::PublicRead, RouteTemplate::Ready, None),
        ("GET", "/api/v1/events") => {
            include(RouteClass::PublicRead, RouteTemplate::ListEvents, None)
        }
        ("GET", "/api/v1/events/{event_id}") => {
            include(RouteClass::PublicRead, RouteTemplate::GetSeedEvent, None)
        }
        ("GET", "/api/v1/events/{event_id}/positions") => include(
            RouteClass::PublicRead,
            RouteTemplate::ListSeedEventPositions,
            None,
        ),
        ("GET", "/api/v1/events/{event_id}/ops-plan") => include(
            RouteClass::PublicRead,
            RouteTemplate::GetSeedEventOpsPlan,
            None,
        ),
        ("GET", "/api/v1/events/{event_id}/tmis") => include(
            RouteClass::PublicRead,
            RouteTemplate::ListSeedEventTmis,
            None,
        ),
        ("GET", "/api/v1/events/{event_id}/preset-positions") => include(
            RouteClass::PublicRead,
            RouteTemplate::GetSeedEventPresetPositions,
            None,
        ),
        ("GET", "/api/v1/user") => include_with_session(
            RouteClass::AuthenticatedRead,
            RouteTemplate::ListUsers,
            Persona::Staff,
            sessions,
        ),
        ("GET", "/api/v1/user/{cid}") => include_with_session(
            RouteClass::AuthenticatedRead,
            RouteTemplate::GetStudentUser,
            Persona::Staff,
            sessions,
        ),
        ("GET", "/api/v1/publications") => include(
            RouteClass::PublicRead,
            RouteTemplate::ListPublications,
            None,
        ),
        ("GET", "/api/v1/me") => include_with_session(
            RouteClass::AuthenticatedRead,
            RouteTemplate::Me,
            Persona::Student,
            sessions,
        ),
        ("GET", "/api/v1/auth/service-account/me") => skip(
            RouteClass::MachineAuthOnly,
            "requires machine bearer token authentication, not session auth",
        ),
        ("GET", "/api/v1/loa/me") => include_with_session(
            RouteClass::AuthenticatedRead,
            RouteTemplate::ListMyLoas,
            Persona::Student,
            sessions,
        ),
        ("GET", "/api/v1/staffing-requests/me") => include_with_session(
            RouteClass::AuthenticatedRead,
            RouteTemplate::ListMyStaffingRequests,
            Persona::Student,
            sessions,
        ),
        ("GET", "/api/v1/users/{cid}/solo-certifications")
            if ctx.persona_cids.contains_key(&Persona::Student) =>
        {
            include_with_session(
                RouteClass::AuthenticatedRead,
                RouteTemplate::ListStudentSoloCertifications,
                Persona::Student,
                sessions,
            )
        }
        ("GET", "/api/v1/admin/loa") => include_with_session(
            RouteClass::AuthenticatedRead,
            RouteTemplate::AdminListLoas,
            Persona::Staff,
            sessions,
        ),
        ("GET", "/api/v1/admin/staffing-requests") => include_with_session(
            RouteClass::AuthenticatedRead,
            RouteTemplate::AdminListStaffingRequests,
            Persona::Staff,
            sessions,
        ),
        ("POST", "/api/v1/events") if allow_mutations(config) && has_staff => include(
            RouteClass::AdminMutation,
            RouteTemplate::CreateEvent,
            Some(Persona::Staff),
        ),
        ("PATCH", "/api/v1/events/{event_id}") if allow_mutations(config) && has_staff => include(
            RouteClass::AdminMutation,
            RouteTemplate::UpdateSeedEvent,
            Some(Persona::Staff),
        ),
        ("POST", "/api/v1/events/{event_id}/positions") => skip(
            RouteClass::SelfServiceMutation,
            "one-position-per-user unique constraint prevents repeatable sweep/load (covered by event-signup scenario)",
        ),
        ("POST", "/api/v1/events/{event_id}/positions/publish")
            if allow_mutations(config) && has_staff =>
        {
            include(
                RouteClass::AdminMutation,
                RouteTemplate::PublishSeedEventPositions,
                Some(Persona::Staff),
            )
        }
        ("POST", "/api/v1/events/{event_id}/positions/lock")
            if allow_mutations(config) && has_staff =>
        {
            include(
                RouteClass::AdminMutation,
                RouteTemplate::LockSeedEventPositions,
                Some(Persona::Staff),
            )
        }
        ("POST", "/api/v1/events/{event_id}/positions/unlock")
            if allow_mutations(config) && has_staff =>
        {
            include(
                RouteClass::AdminMutation,
                RouteTemplate::UnlockSeedEventPositions,
                Some(Persona::Staff),
            )
        }
        ("PUT", "/api/v1/events/{event_id}/preset-positions")
            if allow_mutations(config) && has_staff =>
        {
            include(
                RouteClass::AdminMutation,
                RouteTemplate::UpdateSeedPresetPositions,
                Some(Persona::Staff),
            )
        }
        ("PATCH", "/api/v1/events/{event_id}/ops-plan") if allow_mutations(config) && has_staff => {
            include(
                RouteClass::AdminMutation,
                RouteTemplate::UpdateSeedOpsPlan,
                Some(Persona::Staff),
            )
        }
        ("POST", "/api/v1/events/{event_id}/tmis") if allow_mutations(config) && has_staff => {
            include(
                RouteClass::AdminMutation,
                RouteTemplate::CreateSeedTmi,
                Some(Persona::Staff),
            )
        }
        ("PATCH", "/api/v1/me") if allow_mutations(config) && has_student => include(
            RouteClass::SelfServiceMutation,
            RouteTemplate::PatchMe,
            Some(Persona::Student),
        ),
        ("POST", "/api/v1/me/teamspeak-uids") if allow_mutations(config) && has_student => include(
            RouteClass::SelfServiceMutation,
            RouteTemplate::CreateTeamSpeakUid,
            Some(Persona::Student),
        ),
        ("POST", "/api/v1/loa/me") if allow_mutations(config) && has_student => include(
            RouteClass::SelfServiceMutation,
            RouteTemplate::CreateLoa,
            Some(Persona::Student),
        ),
        ("POST", "/api/v1/staffing-requests/me") if allow_mutations(config) && has_student => {
            include(
                RouteClass::SelfServiceMutation,
                RouteTemplate::CreateStaffingRequest,
                Some(Persona::Student),
            )
        }
        ("GET", "/api/v1/auth/vatsim/login")
        | ("GET", "/api/v1/auth/vatsim/callback")
        | ("POST", "/api/v1/emails/send")
        | ("POST", "/api/v1/events/{event_id}/publish/discord") => skip(
            RouteClass::UnsafeOrExternal,
            "external or browser-driven route skipped",
        ),
        _ => skip(
            RouteClass::Unsupported,
            "unsupported route template or unresolved parameters",
        ),
    }
}

fn allow_mutations(config: &Config) -> bool {
    config.is_local_dev_target() || config.unsafe_mutations
}

fn include(
    route_class: RouteClass,
    template: RouteTemplate,
    persona: Option<Persona>,
) -> (
    RouteClass,
    Option<RouteTemplate>,
    Option<Persona>,
    Option<String>,
) {
    (route_class, Some(template), persona, None)
}

fn include_with_session(
    route_class: RouteClass,
    template: RouteTemplate,
    persona: Persona,
    sessions: &HashMap<Persona, PersonaSession>,
) -> (
    RouteClass,
    Option<RouteTemplate>,
    Option<Persona>,
    Option<String>,
) {
    if sessions.contains_key(&persona) {
        include(route_class, template, Some(persona))
    } else {
        skip(
            route_class,
            &format!("missing persona session for {persona}"),
        )
    }
}

fn skip(
    route_class: RouteClass,
    reason: &str,
) -> (
    RouteClass,
    Option<RouteTemplate>,
    Option<Persona>,
    Option<String>,
) {
    (route_class, None, None, Some(reason.to_string()))
}

pub fn group_routes_for_load(routes: &[DiscoveredRoute]) -> Vec<(String, Vec<DiscoveredRoute>)> {
    let map = routes.iter().filter(|route| route.include).cloned().fold(
        BTreeMap::<String, Vec<DiscoveredRoute>>::new(),
        |mut acc, route| {
            let key = if matches!(route.path.as_str(), "/health" | "/ready")
                || matches!(
                    route.path.as_str(),
                    "/api/v1/events" | "/api/v1/publications"
                ) {
                "health-public"
            } else if route.path == "/api/v1/me"
                || route.path == "/api/v1/loa/me"
                || route.path == "/api/v1/staffing-requests/me"
                || route.path == "/api/v1/user"
            {
                "authenticated-reads"
            } else if route.path.contains("/positions") && route.path.contains("/events/") {
                "event-staffing"
            } else if route.path.contains("/ops-plan")
                || route.path.contains("/preset-positions")
                || route.path.contains("/admin/")
                || (route.path == "/api/v1/events" && route.method != Method::GET)
            {
                "admin-event-management"
            } else {
                "other"
            };
            acc.entry(key.to_string()).or_default().push(route);
            acc
        },
    );

    map.into_iter().collect()
}
