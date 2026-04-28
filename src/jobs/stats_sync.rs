use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, Transaction};
use uuid::Uuid;

use crate::{errors::ApiError, state::AppState};

const DEFAULT_SYNC_INTERVAL_SECS: u64 = 5;
const TARGET_ARTCC_ID: &str = "ZDC";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsEnvironment {
    Live,
    Sweatbox1,
    Sweatbox2,
}

impl StatsEnvironment {
    pub const ALL: [Self; 3] = [Self::Live, Self::Sweatbox1, Self::Sweatbox2];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Sweatbox1 => "sweatbox1",
            Self::Sweatbox2 => "sweatbox2",
        }
    }

    pub fn endpoint_url(self) -> String {
        match self {
            Self::Live => std::env::var("VNAS_CONTROLLER_FEED_URL_LIVE")
                .unwrap_or_else(|_| "https://live.env.vnas.vatsim.net/data-feed/controllers.json".to_string()),
            Self::Sweatbox1 => std::env::var("VNAS_CONTROLLER_FEED_URL_SWEATBOX1")
                .unwrap_or_else(|_| "https://sweatbox1.env.vnas.vatsim.net/data-feed/controllers.json".to_string()),
            Self::Sweatbox2 => std::env::var("VNAS_CONTROLLER_FEED_URL_SWEATBOX2")
                .unwrap_or_else(|_| "https://sweatbox2.env.vnas.vatsim.net/data-feed/controllers.json".to_string()),
        }
    }
}

pub fn parse_environment(value: Option<&str>) -> Result<StatsEnvironment, ApiError> {
    match value.unwrap_or("live").trim().to_ascii_lowercase().as_str() {
        "live" => Ok(StatsEnvironment::Live),
        "sweatbox1" => Ok(StatsEnvironment::Sweatbox1),
        "sweatbox2" => Ok(StatsEnvironment::Sweatbox2),
        _ => Err(ApiError::BadRequest),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerSessionEvent {
    pub environment: String,
    pub artcc_id: String,
    pub cid: i64,
    pub user_id: Option<String>,
    pub session_id: String,
    pub occurred_at: DateTime<Utc>,
    pub real_name: Option<String>,
    pub role: Option<String>,
    pub user_rating: Option<String>,
    pub requested_rating: Option<String>,
    pub primary_facility_id: Option<String>,
    pub primary_position_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerPositionEvent {
    pub environment: String,
    pub artcc_id: String,
    pub cid: i64,
    pub user_id: Option<String>,
    pub session_id: String,
    pub activation_id: String,
    pub occurred_at: DateTime<Utc>,
    pub real_name: Option<String>,
    pub role: Option<String>,
    pub user_rating: Option<String>,
    pub requested_rating: Option<String>,
    pub position_id: String,
    pub facility_id: Option<String>,
    pub facility_name: String,
    pub position_name: String,
    pub position_type: String,
    pub radio_name: Option<String>,
    pub default_callsign: Option<String>,
    pub frequency: Option<i64>,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "data", rename_all = "snake_case")]
pub enum ControllerLifecycleEvent {
    ControllerLoggedOn(ControllerSessionEvent),
    ControllerLoggedOff(ControllerSessionEvent),
    PositionActivated(ControllerPositionEvent),
    PositionDeactivated(ControllerPositionEvent),
}

impl ControllerLifecycleEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::ControllerLoggedOn(_) => "controller_logged_on",
            Self::ControllerLoggedOff(_) => "controller_logged_off",
            Self::PositionActivated(_) => "position_activated",
            Self::PositionDeactivated(_) => "position_deactivated",
        }
    }

    pub fn environment(&self) -> &str {
        match self {
            Self::ControllerLoggedOn(event) | Self::ControllerLoggedOff(event) => &event.environment,
            Self::PositionActivated(event) | Self::PositionDeactivated(event) => &event.environment,
        }
    }

    pub fn cid(&self) -> i64 {
        match self {
            Self::ControllerLoggedOn(event) | Self::ControllerLoggedOff(event) => event.cid,
            Self::PositionActivated(event) | Self::PositionDeactivated(event) => event.cid,
        }
    }

    pub fn user_id(&self) -> Option<&str> {
        match self {
            Self::ControllerLoggedOn(event) | Self::ControllerLoggedOff(event) => {
                event.user_id.as_deref()
            }
            Self::PositionActivated(event) | Self::PositionDeactivated(event) => {
                event.user_id.as_deref()
            }
        }
    }

    pub fn session_id(&self) -> &str {
        match self {
            Self::ControllerLoggedOn(event) | Self::ControllerLoggedOff(event) => &event.session_id,
            Self::PositionActivated(event) | Self::PositionDeactivated(event) => &event.session_id,
        }
    }

    pub fn activation_id(&self) -> Option<&str> {
        match self {
            Self::PositionActivated(event) | Self::PositionDeactivated(event) => {
                Some(&event.activation_id)
            }
            _ => None,
        }
    }

    pub fn occurred_at(&self) -> DateTime<Utc> {
        match self {
            Self::ControllerLoggedOn(event) | Self::ControllerLoggedOff(event) => event.occurred_at,
            Self::PositionActivated(event) | Self::PositionDeactivated(event) => event.occurred_at,
        }
    }
}

#[derive(Debug)]
struct EnvironmentSyncResult {
    environment: StatsEnvironment,
    ok: bool,
    processed: usize,
    online: usize,
    source_updated_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VnasRoot {
    updated_at: String,
    controllers: Vec<VnasController>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VnasController {
    artcc_id: String,
    primary_facility_id: Option<String>,
    primary_position_id: Option<String>,
    role: Option<String>,
    positions: Vec<VnasPosition>,
    is_active: bool,
    is_observer: bool,
    login_time: String,
    vatsim_data: VnasData,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VnasPosition {
    facility_id: Option<String>,
    facility_name: String,
    position_id: String,
    position_name: String,
    position_type: String,
    radio_name: Option<String>,
    default_callsign: Option<String>,
    frequency: Option<i64>,
    is_primary: bool,
    is_active: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VnasData {
    cid: String,
    real_name: Option<String>,
    user_rating: Option<String>,
    requested_rating: Option<String>,
}

#[derive(Debug, FromRow)]
struct OpenSessionRow {
    id: String,
    cid: i64,
    user_id: Option<String>,
    artcc_id: String,
    real_name: Option<String>,
    role: Option<String>,
    user_rating: Option<String>,
    requested_rating: Option<String>,
    primary_facility_id: Option<String>,
    primary_position_id: Option<String>,
    login_at: DateTime<Utc>,
    source_login_time_raw: String,
}

#[derive(Debug, FromRow)]
struct OpenActivationRow {
    id: String,
    environment: String,
    cid: i64,
    position_id: String,
    facility_id: Option<String>,
    facility_name: String,
    position_name: String,
    position_type: String,
    radio_name: Option<String>,
    default_callsign: Option<String>,
    frequency: Option<i64>,
    is_primary: bool,
    started_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    id: String,
    environment: String,
    artcc_id: String,
    cid: i64,
    user_id: Option<String>,
    real_name: Option<String>,
    role: Option<String>,
    user_rating: Option<String>,
    requested_rating: Option<String>,
    primary_facility_id: Option<String>,
    primary_position_id: Option<String>,
}

#[derive(Clone)]
struct ControllerSnapshot {
    artcc_id: String,
    cid: i64,
    user_id: Option<String>,
    real_name: Option<String>,
    role: Option<String>,
    user_rating: Option<String>,
    requested_rating: Option<String>,
    login_at: DateTime<Utc>,
    login_time_raw: String,
    primary_facility_id: Option<String>,
    primary_position_id: Option<String>,
    is_active: bool,
    positions: Vec<ActivePositionSnapshot>,
}

#[derive(Clone)]
struct ActivePositionSnapshot {
    position_id: String,
    facility_id: Option<String>,
    facility_name: String,
    position_name: String,
    position_type: String,
    radio_name: Option<String>,
    default_callsign: Option<String>,
    frequency: Option<i64>,
    is_primary: bool,
}

pub fn start_stats_sync_worker(state: AppState) {
    if !stats_sync_enabled() {
        if let Ok(mut health) = state.job_health.write() {
            health.stats_sync.enabled = false;
        }
        tracing::info!("stats sync worker disabled");
        return;
    }

    if let Ok(mut health) = state.job_health.write() {
        health.stats_sync.enabled = true;
    }

    let interval_secs = std::env::var("STATS_SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SYNC_INTERVAL_SECS)
        .max(5);

    tracing::info!(interval_secs, "starting stats sync worker");

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            let started_at = Utc::now();

            if let Ok(mut health) = state.job_health.write() {
                for environment in StatsEnvironment::ALL {
                    let env_health = health.stats_sync.environment_mut(environment.as_str());
                    env_health.last_started_at = Some(started_at);
                    env_health.last_error = None;
                }
            }

            let (live, sweatbox1, sweatbox2) = tokio::join!(
                sync_environment(state.clone(), StatsEnvironment::Live),
                sync_environment(state.clone(), StatsEnvironment::Sweatbox1),
                sync_environment(state.clone(), StatsEnvironment::Sweatbox2),
            );

            for result in [live, sweatbox1, sweatbox2] {
                match result {
                    Ok(result) => {
                        if let Ok(mut health) = state.job_health.write() {
                            let env_health =
                                health.stats_sync.environment_mut(result.environment.as_str());
                            env_health.last_finished_at = Some(Utc::now());
                            env_health.last_result_ok = Some(result.ok);
                            env_health.processed = Some(result.processed);
                            env_health.online = Some(result.online);
                            env_health.source_updated_at = result.source_updated_at;
                            if result.ok {
                                env_health.last_success_at = Some(Utc::now());
                            }
                        }

                        tracing::info!(
                            environment = result.environment.as_str(),
                            processed = result.processed,
                            online = result.online,
                            "stats sync completed"
                        );
                    }
                    Err(ApiError::ServiceUnavailable) => {
                        if let Ok(mut health) = state.job_health.write() {
                            for environment in StatsEnvironment::ALL {
                                let env_health =
                                    health.stats_sync.environment_mut(environment.as_str());
                                if env_health.last_started_at == Some(started_at) {
                                    env_health.last_finished_at = Some(Utc::now());
                                    env_health.last_result_ok = Some(false);
                                    env_health.last_error =
                                        Some("database unavailable for stats sync".to_string());
                                }
                            }
                        }
                    }
                    Err(err) => {
                        tracing::warn!(?err, "stats sync failed");
                    }
                }
            }
        }
    });
}

async fn sync_environment(
    state: AppState,
    environment: StatsEnvironment,
) -> Result<EnvironmentSyncResult, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let poll_time = Utc::now();
    let endpoint_url = environment.endpoint_url();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| ApiError::Internal)?;

    let feed = client
        .get(&endpoint_url)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?
        .error_for_status()
        .map_err(|_| ApiError::ServiceUnavailable)?
        .json::<VnasRoot>()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;

    let source_updated_at = parse_feed_timestamp(&feed.updated_at).unwrap_or(poll_time);
    let snapshot_count = feed.controllers.len();

    let mut controllers = feed
        .controllers
        .into_iter()
        .filter(|controller| controller.artcc_id == TARGET_ARTCC_ID && !controller.is_observer)
        .collect::<Vec<_>>();

    let online = controllers.iter().filter(|controller| controller.is_active).count();

    let existing_updated_at = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "select last_source_updated_at from stats.controller_feed_state where environment = $1",
    )
    .bind(environment.as_str())
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .flatten();

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    upsert_feed_state(
        &mut tx,
        environment,
        &endpoint_url,
        poll_time,
        source_updated_at,
        snapshot_count as i32,
        controllers.len() as i32,
        None,
    )
    .await?;

    if existing_updated_at == Some(source_updated_at) {
        tx.commit().await.map_err(|_| ApiError::Internal)?;
        return Ok(EnvironmentSyncResult {
            environment,
            ok: true,
            processed: controllers.len(),
            online,
            source_updated_at: Some(source_updated_at),
        });
    }

    let cids = controllers
        .iter()
        .filter_map(|controller| controller.vatsim_data.cid.parse::<i64>().ok())
        .collect::<Vec<_>>();
    let user_map = fetch_user_ids(&mut tx, &cids).await?;
    let open_sessions = fetch_open_sessions(&mut tx, environment).await?;
    let mut open_session_map = open_sessions
        .into_iter()
        .map(|session| (session_key(session.cid, &session.source_login_time_raw), session))
        .collect::<HashMap<_, _>>();

    let mut emitted_events = Vec::new();

    controllers.sort_by_key(|controller| controller.vatsim_data.cid.clone());

    for controller in controllers.drain(..) {
        let Ok(cid) = controller.vatsim_data.cid.parse::<i64>() else {
            continue;
        };

        let login_at = parse_feed_timestamp(&controller.login_time).unwrap_or(poll_time);
        let user_id = user_map.get(&cid).cloned().flatten();
        let snapshot = ControllerSnapshot {
            artcc_id: controller.artcc_id.clone(),
            cid,
            user_id,
            real_name: controller.vatsim_data.real_name.clone(),
            role: controller.role.clone(),
            user_rating: controller.vatsim_data.user_rating.clone(),
            requested_rating: controller.vatsim_data.requested_rating.clone(),
            login_at,
            login_time_raw: controller.login_time.clone(),
            primary_facility_id: controller.primary_facility_id.clone(),
            primary_position_id: controller.primary_position_id.clone(),
            is_active: controller.is_active,
            positions: controller
                .positions
                .iter()
                .filter(|position| controller.is_active && position.is_active)
                .map(|position| ActivePositionSnapshot {
                    position_id: position.position_id.clone(),
                    facility_id: position.facility_id.clone(),
                    facility_name: position.facility_name.clone(),
                    position_name: position.position_name.clone(),
                    position_type: position.position_type.clone(),
                    radio_name: position.radio_name.clone(),
                    default_callsign: position.default_callsign.clone(),
                    frequency: position.frequency,
                    is_primary: position.is_primary,
                })
                .collect(),
        };

        let session = if let Some(existing) =
            open_session_map.remove(&session_key(cid, &snapshot.login_time_raw))
        {
            update_open_session(&mut tx, &existing.id, &snapshot).await?;
            SessionRecord {
                id: existing.id,
                environment: environment.as_str().to_string(),
                artcc_id: snapshot.artcc_id.clone(),
                cid: snapshot.cid,
                user_id: snapshot.user_id.clone(),
                real_name: snapshot.real_name.clone(),
                role: snapshot.role.clone(),
                user_rating: snapshot.user_rating.clone(),
                requested_rating: snapshot.requested_rating.clone(),
                primary_facility_id: snapshot.primary_facility_id.clone(),
                primary_position_id: snapshot.primary_position_id.clone(),
            }
        } else {
            let session = insert_session(&mut tx, environment, &snapshot).await?;
            emitted_events.push(ControllerLifecycleEvent::ControllerLoggedOn(
                build_session_event(&session, snapshot.login_at),
            ));
            session
        };

        sync_session_activations(&mut tx, &session, &snapshot, poll_time, &mut emitted_events).await?;
    }

    for stale_session in open_session_map.into_values() {
        close_session(
            &mut tx,
            environment,
            stale_session,
            poll_time,
            &mut emitted_events,
        )
        .await?;
    }

    persist_events(&mut tx, &emitted_events).await?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;

    for event in emitted_events {
        let _ = state.controller_events.send(event);
    }

    Ok(EnvironmentSyncResult {
        environment,
        ok: true,
        processed: cids.len(),
        online,
        source_updated_at: Some(source_updated_at),
    })
}

async fn upsert_feed_state(
    tx: &mut Transaction<'_, Postgres>,
    environment: StatsEnvironment,
    endpoint_url: &str,
    last_polled_at: DateTime<Utc>,
    last_source_updated_at: DateTime<Utc>,
    last_snapshot_count: i32,
    last_zdc_count: i32,
    last_error: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into stats.controller_feed_state (
            environment,
            endpoint_url,
            last_polled_at,
            last_source_updated_at,
            last_success_at,
            last_error,
            last_snapshot_count,
            last_zdc_count
        )
        values ($1, $2, $3, $4, $3, $5, $6, $7)
        on conflict (environment) do update
        set endpoint_url = excluded.endpoint_url,
            last_polled_at = excluded.last_polled_at,
            last_source_updated_at = excluded.last_source_updated_at,
            last_success_at = excluded.last_success_at,
            last_error = excluded.last_error,
            last_snapshot_count = excluded.last_snapshot_count,
            last_zdc_count = excluded.last_zdc_count
        "#,
    )
    .bind(environment.as_str())
    .bind(endpoint_url)
    .bind(last_polled_at)
    .bind(last_source_updated_at)
    .bind(last_error)
    .bind(last_snapshot_count)
    .bind(last_zdc_count)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn fetch_user_ids(
    tx: &mut Transaction<'_, Postgres>,
    cids: &[i64],
) -> Result<HashMap<i64, Option<String>>, ApiError> {
    if cids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query_as::<_, (i64, String)>(
        "select cid, id from identity.users where cid = any($1)",
    )
    .bind(cids)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let mut map = cids.iter().copied().map(|cid| (cid, None)).collect::<HashMap<_, _>>();
    for (cid, user_id) in rows {
        map.insert(cid, Some(user_id));
    }

    Ok(map)
}

async fn fetch_open_sessions(
    tx: &mut Transaction<'_, Postgres>,
    environment: StatsEnvironment,
) -> Result<Vec<OpenSessionRow>, ApiError> {
    sqlx::query_as::<_, OpenSessionRow>(
        r#"
        select
            id,
            cid,
            user_id,
            artcc_id,
            real_name,
            role,
            user_rating,
            requested_rating,
            primary_facility_id,
            primary_position_id,
            login_at,
            source_login_time_raw
        from stats.controller_sessions
        where environment = $1 and logout_at is null
        "#,
    )
    .bind(environment.as_str())
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn update_open_session(
    tx: &mut Transaction<'_, Postgres>,
    session_id: &str,
    snapshot: &ControllerSnapshot,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update stats.controller_sessions
        set user_id = $2,
            artcc_id = $3,
            real_name = $4,
            role = $5,
            user_rating = $6,
            requested_rating = $7,
            primary_facility_id = $8,
            primary_position_id = $9,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(session_id)
    .bind(&snapshot.user_id)
    .bind(&snapshot.artcc_id)
    .bind(&snapshot.real_name)
    .bind(&snapshot.role)
    .bind(&snapshot.user_rating)
    .bind(&snapshot.requested_rating)
    .bind(&snapshot.primary_facility_id)
    .bind(&snapshot.primary_position_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn insert_session(
    tx: &mut Transaction<'_, Postgres>,
    environment: StatsEnvironment,
    snapshot: &ControllerSnapshot,
) -> Result<SessionRecord, ApiError> {
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        insert into stats.controller_sessions (
            id,
            environment,
            artcc_id,
            cid,
            user_id,
            real_name,
            role,
            user_rating,
            requested_rating,
            login_at,
            primary_facility_id,
            primary_position_id,
            source_login_time_raw
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(&id)
    .bind(environment.as_str())
    .bind(&snapshot.artcc_id)
    .bind(snapshot.cid)
    .bind(&snapshot.user_id)
    .bind(&snapshot.real_name)
    .bind(&snapshot.role)
    .bind(&snapshot.user_rating)
    .bind(&snapshot.requested_rating)
    .bind(snapshot.login_at)
    .bind(&snapshot.primary_facility_id)
    .bind(&snapshot.primary_position_id)
    .bind(&snapshot.login_time_raw)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(SessionRecord {
        id,
        environment: environment.as_str().to_string(),
        artcc_id: snapshot.artcc_id.clone(),
        cid: snapshot.cid,
        user_id: snapshot.user_id.clone(),
        real_name: snapshot.real_name.clone(),
        role: snapshot.role.clone(),
        user_rating: snapshot.user_rating.clone(),
        requested_rating: snapshot.requested_rating.clone(),
        primary_facility_id: snapshot.primary_facility_id.clone(),
        primary_position_id: snapshot.primary_position_id.clone(),
    })
}

async fn sync_session_activations(
    tx: &mut Transaction<'_, Postgres>,
    session: &SessionRecord,
    snapshot: &ControllerSnapshot,
    now: DateTime<Utc>,
    emitted_events: &mut Vec<ControllerLifecycleEvent>,
) -> Result<(), ApiError> {
    let existing = sqlx::query_as::<_, OpenActivationRow>(
        r#"
        select
            id,
            environment,
            cid,
            position_id,
            facility_id,
            facility_name,
            position_name,
            position_type,
            radio_name,
            default_callsign,
            frequency,
            is_primary,
            started_at
        from stats.controller_activations
        where session_id = $1 and ended_at is null
        "#,
    )
    .bind(&session.id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let mut existing_map = existing
        .into_iter()
        .map(|activation| (activation.position_id.clone(), activation))
        .collect::<HashMap<_, _>>();
    let mut seen_positions = HashSet::new();

    for position in &snapshot.positions {
        seen_positions.insert(position.position_id.clone());

        if let Some(existing) = existing_map.remove(&position.position_id) {
            update_open_activation(tx, &existing.id, position).await?;
            continue;
        }

        let activation = insert_activation(tx, session, position, now).await?;
        emitted_events.push(ControllerLifecycleEvent::PositionActivated(
            build_position_event(session, &activation, now),
        ));
    }

    for activation in existing_map.into_values() {
        close_activation(tx, session, activation, now, emitted_events).await?;
    }

    if !snapshot.is_active && !seen_positions.is_empty() {
        tracing::debug!(cid = snapshot.cid, "inactive controller retained active positions");
    }

    Ok(())
}

async fn update_open_activation(
    tx: &mut Transaction<'_, Postgres>,
    activation_id: &str,
    position: &ActivePositionSnapshot,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update stats.controller_activations
        set facility_id = $2,
            facility_name = $3,
            position_name = $4,
            position_type = $5,
            radio_name = $6,
            default_callsign = $7,
            frequency = $8,
            is_primary = $9
        where id = $1
        "#,
    )
    .bind(activation_id)
    .bind(&position.facility_id)
    .bind(&position.facility_name)
    .bind(&position.position_name)
    .bind(&position.position_type)
    .bind(&position.radio_name)
    .bind(&position.default_callsign)
    .bind(position.frequency)
    .bind(position.is_primary)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn insert_activation(
    tx: &mut Transaction<'_, Postgres>,
    session: &SessionRecord,
    position: &ActivePositionSnapshot,
    started_at: DateTime<Utc>,
) -> Result<OpenActivationRow, ApiError> {
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        insert into stats.controller_activations (
            id,
            session_id,
            environment,
            cid,
            position_id,
            facility_id,
            facility_name,
            position_name,
            position_type,
            radio_name,
            default_callsign,
            frequency,
            is_primary,
            started_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        "#,
    )
    .bind(&id)
    .bind(&session.id)
    .bind(&session.environment)
    .bind(session.cid)
    .bind(&position.position_id)
    .bind(&position.facility_id)
    .bind(&position.facility_name)
    .bind(&position.position_name)
    .bind(&position.position_type)
    .bind(&position.radio_name)
    .bind(&position.default_callsign)
    .bind(position.frequency)
    .bind(position.is_primary)
    .bind(started_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(OpenActivationRow {
        id,
        environment: session.environment.clone(),
        cid: session.cid,
        position_id: position.position_id.clone(),
        facility_id: position.facility_id.clone(),
        facility_name: position.facility_name.clone(),
        position_name: position.position_name.clone(),
        position_type: position.position_type.clone(),
        radio_name: position.radio_name.clone(),
        default_callsign: position.default_callsign.clone(),
        frequency: position.frequency,
        is_primary: position.is_primary,
        started_at,
    })
}

async fn close_activation(
    tx: &mut Transaction<'_, Postgres>,
    session: &SessionRecord,
    activation: OpenActivationRow,
    ended_at: DateTime<Utc>,
    emitted_events: &mut Vec<ControllerLifecycleEvent>,
) -> Result<(), ApiError> {
    let active_seconds = (ended_at - activation.started_at).num_seconds().max(0);

    sqlx::query(
        r#"
        update stats.controller_activations
        set ended_at = $2,
            active_seconds = $3
        where id = $1
        "#,
    )
    .bind(&activation.id)
    .bind(ended_at)
    .bind(active_seconds)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    add_monthly_rollup(
        tx,
        &activation.environment,
        activation.cid,
        activation.started_at,
        ended_at,
        RollupKind::Position(&activation.position_type),
    )
    .await?;

    emitted_events.push(ControllerLifecycleEvent::PositionDeactivated(
        build_position_event(session, &activation, ended_at),
    ));

    Ok(())
}

async fn close_session(
    tx: &mut Transaction<'_, Postgres>,
    environment: StatsEnvironment,
    session: OpenSessionRow,
    logout_at: DateTime<Utc>,
    emitted_events: &mut Vec<ControllerLifecycleEvent>,
) -> Result<(), ApiError> {
    let open_activations = sqlx::query_as::<_, OpenActivationRow>(
        r#"
        select
            id,
            environment,
            cid,
            position_id,
            facility_id,
            facility_name,
            position_name,
            position_type,
            radio_name,
            default_callsign,
            frequency,
            is_primary,
            started_at
        from stats.controller_activations
        where session_id = $1 and ended_at is null
        "#,
    )
    .bind(&session.id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let session_record = SessionRecord {
        id: session.id.clone(),
        environment: environment.as_str().to_string(),
        artcc_id: session.artcc_id.clone(),
        cid: session.cid,
        user_id: session.user_id.clone(),
        real_name: session.real_name.clone(),
        role: session.role.clone(),
        user_rating: session.user_rating.clone(),
        requested_rating: session.requested_rating.clone(),
        primary_facility_id: session.primary_facility_id.clone(),
        primary_position_id: session.primary_position_id.clone(),
    };

    for activation in open_activations {
        close_activation(tx, &session_record, activation, logout_at, emitted_events).await?;
    }

    let online_seconds = (logout_at - session.login_at).num_seconds().max(0);
    sqlx::query(
        r#"
        update stats.controller_sessions
        set logout_at = $2,
            online_seconds = $3,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(&session.id)
    .bind(logout_at)
    .bind(online_seconds)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    add_monthly_rollup(
        tx,
        environment.as_str(),
        session.cid,
        session.login_at,
        logout_at,
        RollupKind::Online,
    )
    .await?;

    emitted_events.push(ControllerLifecycleEvent::ControllerLoggedOff(
        build_session_event(&session_record, logout_at),
    ));

    Ok(())
}

enum RollupKind<'a> {
    Online,
    Position(&'a str),
}

async fn add_monthly_rollup(
    tx: &mut Transaction<'_, Postgres>,
    environment: &str,
    cid: i64,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    kind: RollupKind<'_>,
) -> Result<(), ApiError> {
    for segment in monthly_segments(started_at, ended_at) {
        let (online_seconds, delivery_seconds, ground_seconds, tower_seconds, tracon_seconds, center_seconds) =
            match kind {
                RollupKind::Online => (segment.seconds, 0, 0, 0, 0, 0),
                RollupKind::Position(position_type) => match map_position_type(position_type) {
                    PositionBucket::Delivery => (0, segment.seconds, 0, 0, 0, 0),
                    PositionBucket::Ground => (0, 0, segment.seconds, 0, 0, 0),
                    PositionBucket::Tower => (0, 0, 0, segment.seconds, 0, 0),
                    PositionBucket::Tracon => (0, 0, 0, 0, segment.seconds, 0),
                    PositionBucket::Center => (0, 0, 0, 0, 0, segment.seconds),
                    PositionBucket::Unknown => (0, 0, 0, 0, 0, 0),
                },
            };

        if online_seconds
            + delivery_seconds
            + ground_seconds
            + tower_seconds
            + tracon_seconds
            + center_seconds
            == 0
        {
            continue;
        }

        sqlx::query(
            r#"
            insert into stats.controller_monthly_rollups (
                environment,
                cid,
                year,
                month,
                online_seconds,
                delivery_seconds,
                ground_seconds,
                tower_seconds,
                tracon_seconds,
                center_seconds
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            on conflict (environment, cid, year, month) do update
            set online_seconds = stats.controller_monthly_rollups.online_seconds + excluded.online_seconds,
                delivery_seconds = stats.controller_monthly_rollups.delivery_seconds + excluded.delivery_seconds,
                ground_seconds = stats.controller_monthly_rollups.ground_seconds + excluded.ground_seconds,
                tower_seconds = stats.controller_monthly_rollups.tower_seconds + excluded.tower_seconds,
                tracon_seconds = stats.controller_monthly_rollups.tracon_seconds + excluded.tracon_seconds,
                center_seconds = stats.controller_monthly_rollups.center_seconds + excluded.center_seconds,
                updated_at = now()
            "#,
        )
        .bind(environment)
        .bind(cid)
        .bind(segment.year)
        .bind(segment.month0)
        .bind(online_seconds)
        .bind(delivery_seconds)
        .bind(ground_seconds)
        .bind(tower_seconds)
        .bind(tracon_seconds)
        .bind(center_seconds)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    }

    Ok(())
}

async fn persist_events(
    tx: &mut Transaction<'_, Postgres>,
    events: &[ControllerLifecycleEvent],
) -> Result<(), ApiError> {
    for event in events {
        let payload = serde_json::to_value(event).map_err(|_| ApiError::Internal)?;
        sqlx::query(
            r#"
            insert into stats.controller_events (
                environment,
                event_type,
                cid,
                user_id,
                session_id,
                activation_id,
                occurred_at,
                payload
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(event.environment())
        .bind(event.event_type())
        .bind(event.cid())
        .bind(event.user_id())
        .bind(event.session_id())
        .bind(event.activation_id())
        .bind(event.occurred_at())
        .bind(payload)
        .execute(&mut **tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    }

    Ok(())
}

fn build_session_event(session: &SessionRecord, occurred_at: DateTime<Utc>) -> ControllerSessionEvent {
    ControllerSessionEvent {
        environment: session.environment.clone(),
        artcc_id: session.artcc_id.clone(),
        cid: session.cid,
        user_id: session.user_id.clone(),
        session_id: session.id.clone(),
        occurred_at,
        real_name: session.real_name.clone(),
        role: session.role.clone(),
        user_rating: session.user_rating.clone(),
        requested_rating: session.requested_rating.clone(),
        primary_facility_id: session.primary_facility_id.clone(),
        primary_position_id: session.primary_position_id.clone(),
    }
}

fn build_position_event(
    session: &SessionRecord,
    activation: &OpenActivationRow,
    occurred_at: DateTime<Utc>,
) -> ControllerPositionEvent {
    ControllerPositionEvent {
        environment: session.environment.clone(),
        artcc_id: session.artcc_id.clone(),
        cid: session.cid,
        user_id: session.user_id.clone(),
        session_id: session.id.clone(),
        activation_id: activation.id.clone(),
        occurred_at,
        real_name: session.real_name.clone(),
        role: session.role.clone(),
        user_rating: session.user_rating.clone(),
        requested_rating: session.requested_rating.clone(),
        position_id: activation.position_id.clone(),
        facility_id: activation.facility_id.clone(),
        facility_name: activation.facility_name.clone(),
        position_name: activation.position_name.clone(),
        position_type: activation.position_type.clone(),
        radio_name: activation.radio_name.clone(),
        default_callsign: activation.default_callsign.clone(),
        frequency: activation.frequency,
        is_primary: activation.is_primary,
    }
}

fn stats_sync_enabled() -> bool {
    std::env::var("STATS_SYNC_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(true)
}

fn parse_feed_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn session_key(cid: i64, login_time_raw: &str) -> String {
    format!("{cid}:{login_time_raw}")
}

struct MonthlySegment {
    year: i32,
    month0: i32,
    seconds: i64,
}

fn monthly_segments(started_at: DateTime<Utc>, ended_at: DateTime<Utc>) -> Vec<MonthlySegment> {
    if ended_at <= started_at {
        return Vec::new();
    }

    let mut segments = Vec::new();
    let mut cursor = started_at;

    while cursor < ended_at {
        let next_month = next_month_boundary(cursor);
        let segment_end = if next_month < ended_at { next_month } else { ended_at };
        let seconds = (segment_end - cursor).num_seconds().max(0);
        if seconds > 0 {
            segments.push(MonthlySegment {
                year: cursor.year(),
                month0: cursor.month0() as i32,
                seconds,
            });
        }
        cursor = segment_end;
    }

    segments
}

fn next_month_boundary(value: DateTime<Utc>) -> DateTime<Utc> {
    let year = value.year();
    let month = value.month();
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };

    let date = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("valid next month date")
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight");
    Utc.from_utc_datetime(&date)
}

#[derive(Clone, Copy)]
enum PositionBucket {
    Delivery,
    Ground,
    Tower,
    Tracon,
    Center,
    Unknown,
}

fn map_position_type(value: &str) -> PositionBucket {
    match value {
        "Delivery" | "ClearanceDelivery" => PositionBucket::Delivery,
        "Ground" => PositionBucket::Ground,
        "Tower" => PositionBucket::Tower,
        "Tracon" | "ApproachDeparture" => PositionBucket::Tracon,
        "Artcc" | "Center" => PositionBucket::Center,
        _ => PositionBucket::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{PositionBucket, map_position_type, monthly_segments};
    use chrono::{TimeZone, Utc};

    #[test]
    fn maps_position_types() {
        assert!(matches!(map_position_type("Artcc"), PositionBucket::Center));
        assert!(matches!(map_position_type("Tracon"), PositionBucket::Tracon));
        assert!(matches!(
            map_position_type("ClearanceDelivery"),
            PositionBucket::Delivery
        ));
    }

    #[test]
    fn splits_monthly_segments() {
        let start = Utc.with_ymd_and_hms(2026, 1, 31, 23, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 2, 1, 1, 0, 0).unwrap();
        let segments = monthly_segments(start, end);

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].month0, 0);
        assert_eq!(segments[0].seconds, 3600);
        assert_eq!(segments[1].month0, 1);
        assert_eq!(segments[1].seconds, 3600);
    }
}
