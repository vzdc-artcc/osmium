use chrono::{DateTime, Datelike, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{errors::ApiError, state::AppState};

const DEFAULT_VNAS_FEED_URL: &str = "https://live.env.vnas.vatsim.net/data-feed/controllers.json";
const DEFAULT_SYNC_GUARD_SECS: i64 = 10;
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 15;

#[derive(Debug)]
pub struct StatsSyncResult {
    pub ok: bool,
    pub processed: usize,
    pub online: usize,
    pub closed_positions: usize,
    pub opened_positions: usize,
}

#[derive(sqlx::FromRow)]
struct UserLite {
    id: String,
    cid: i64,
}

#[derive(sqlx::FromRow)]
struct ActivePosition {
    id: String,
    position: String,
    facility: Option<i32>,
    start: DateTime<Utc>,
}

#[derive(Deserialize)]
struct VnasRoot {
    controllers: Vec<VnasController>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VnasController {
    login_time: String,
    is_active: bool,
    vatsim_data: VnasData,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct VnasData {
    cid: String,
    callsign: String,
    facility_type: String,
    primary_frequency: i64,
}

pub fn start_stats_sync_worker(state: AppState) {
    if !stats_sync_enabled() {
        if let Ok(mut health) = state.job_health.write() {
            health.stats_sync.enabled = false;
            health.stats_sync.last_error = Some("stats sync disabled by config".to_string());
        }
        tracing::info!("stats sync worker disabled");
        return;
    }

    if let Ok(mut health) = state.job_health.write() {
        health.stats_sync.enabled = true;
        health.stats_sync.last_error = None;
    }

    let interval_secs = std::env::var("STATS_SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
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
                health.stats_sync.last_started_at = Some(started_at);
                health.stats_sync.last_error = None;
            }

            match run_stats_sync_once(&state).await {
                Ok(result) => {
                    if let Ok(mut health) = state.job_health.write() {
                        health.stats_sync.last_finished_at = Some(Utc::now());
                        health.stats_sync.last_result_ok = Some(result.ok);
                        health.stats_sync.processed = Some(result.processed);
                        health.stats_sync.online = Some(result.online);
                        if result.ok {
                            health.stats_sync.last_success_at = Some(Utc::now());
                        }
                    }

                    tracing::info!(
                        ok = result.ok,
                        processed = result.processed,
                        online = result.online,
                        closed_positions = result.closed_positions,
                        opened_positions = result.opened_positions,
                        "stats sync completed"
                    );
                }
                Err(ApiError::ServiceUnavailable) => {
                    if let Ok(mut health) = state.job_health.write() {
                        health.stats_sync.last_finished_at = Some(Utc::now());
                        health.stats_sync.last_result_ok = Some(false);
                        health.stats_sync.last_error =
                            Some("database unavailable for stats sync".to_string());
                    }
                    tracing::debug!("stats sync skipped (database unavailable)");
                }
                Err(err) => {
                    if let Ok(mut health) = state.job_health.write() {
                        health.stats_sync.last_finished_at = Some(Utc::now());
                        health.stats_sync.last_result_ok = Some(false);
                        health.stats_sync.last_error = Some(format!("{:?}", err));
                    }
                    tracing::warn!(?err, "stats sync failed");
                }
            }
        }
    });
}

pub async fn run_stats_sync_once(state: &AppState) -> Result<StatsSyncResult, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let now = Utc::now();

    let guard_secs = std::env::var("STATS_SYNC_GUARD_SECS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_SYNC_GUARD_SECS)
        .max(1);

    let last_sync = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "select stats from sync_times where id = 'default'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .flatten();

    if let Some(last) = last_sync
        && last > now - chrono::Duration::seconds(guard_secs)
    {
        return Ok(StatsSyncResult {
            ok: false,
            processed: 0,
            online: 0,
            closed_positions: 0,
            opened_positions: 0,
        });
    }

    let controllers = fetch_vnas_controller_data().await?;

    let prefixes = sqlx::query_scalar::<_, Vec<String>>(
        "select prefixes from statistics_prefixes where id = 'default'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?
    .unwrap_or_default();

    let users = sqlx::query_as::<_, UserLite>("select id, cid from users")
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

    let mut closed_positions = 0usize;
    let mut opened_positions = 0usize;
    let mut online = 0usize;

    for user in &users {
        let user_cid = user.cid.to_string();
        let maybe_controller = controllers
            .iter()
            .find(|controller| controller.vatsim_data.cid == user_cid)
            .cloned();

        let active_position = sqlx::query_as::<_, ActivePosition>(
            r#"
            select cp.id, cp.position, cp.facility, cp.start
            from controller_positions cp
            join controller_logs cl on cl.id = cp.log_id
            where cl.user_id = $1 and cp.active = true
            limit 1
            "#,
        )
        .bind(&user.id)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)?;

        let is_online = if let Some(controller) = &maybe_controller {
            controller.is_active
                && controller.vatsim_data.primary_frequency != 199_998_000
                && (prefixes.is_empty()
                    || prefixes.iter().any(|prefix| {
                        controller
                            .vatsim_data
                            .callsign
                            .starts_with(&format!("{}{}", prefix, "_"))
                    }))
        } else {
            false
        };

        if !is_online {
            if let Some(active) = active_position {
                close_position_and_add_hours(pool, &user.id, &active, now).await?;
                closed_positions += 1;
            }
            continue;
        }

        online += 1;
        let controller = maybe_controller.expect("controller exists when online");

        let callsign = controller.vatsim_data.callsign.clone();
        let facility = map_facility_type(&controller.vatsim_data.facility_type);
        let login_time = DateTime::parse_from_rfc3339(&controller.login_time)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(now);

        if let Some(active) = active_position {
            if active.position != callsign {
                close_position_and_add_hours(pool, &user.id, &active, now).await?;
                create_active_position(pool, &user.id, &callsign, facility, login_time).await?;
                closed_positions += 1;
                opened_positions += 1;
            }
        } else {
            create_active_position(pool, &user.id, &callsign, facility, login_time).await?;
            opened_positions += 1;
        }

        sqlx::query(
            r#"
            delete from loas
            where user_id = $1
              and start <= $2
              and "end" >= $2
              and status = 'APPROVED'
            "#,
        )
        .bind(&user.id)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    }

    sqlx::query(
        "insert into sync_times (id, stats, updated_at) values ('default', $1, now()) on conflict (id) do update set stats = excluded.stats, updated_at = now()",
    )
    .bind(now)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(StatsSyncResult {
        ok: true,
        processed: users.len(),
        online,
        closed_positions,
        opened_positions,
    })
}

fn stats_sync_enabled() -> bool {
    std::env::var("STATS_SYNC_ENABLED")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(true)
}

async fn fetch_vnas_controller_data() -> Result<Vec<VnasController>, ApiError> {
    let url = std::env::var("VNAS_CONTROLLER_FEED_URL")
        .unwrap_or_else(|_| DEFAULT_VNAS_FEED_URL.to_string());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| ApiError::Internal)?;

    let body = client
        .get(url)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?
        .error_for_status()
        .map_err(|_| ApiError::ServiceUnavailable)?
        .json::<VnasRoot>()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;

    Ok(body.controllers)
}

async fn ensure_controller_log(pool: &sqlx::PgPool, user_id: &str) -> Result<String, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        insert into controller_logs (id, user_id)
        values ($1, $2)
        on conflict (user_id) do update set user_id = excluded.user_id
        returning id
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

async fn create_active_position(
    pool: &sqlx::PgPool,
    user_id: &str,
    position: &str,
    facility: i32,
    start: DateTime<Utc>,
) -> Result<(), ApiError> {
    let log_id = ensure_controller_log(pool, user_id).await?;

    sqlx::query(
        r#"
        insert into controller_positions (id, log_id, position, facility, start, active)
        values ($1, $2, $3, $4, $5, true)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(log_id)
    .bind(position)
    .bind(facility)
    .bind(start)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn close_position_and_add_hours(
    pool: &sqlx::PgPool,
    user_id: &str,
    position: &ActivePosition,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        "update controller_positions set active = false, \"end\" = $2 where id = $1",
    )
    .bind(&position.id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let elapsed = (now - position.start).num_seconds().max(0) as f64 / 3600.0;
    add_hours(pool, user_id, position.facility.unwrap_or(0), elapsed, now).await
}

async fn add_hours(
    pool: &sqlx::PgPool,
    user_id: &str,
    facility: i32,
    hours: f64,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if hours <= 0.0 {
        return Ok(());
    }

    let log_id = ensure_controller_log(pool, user_id).await?;
    let month = now.month0() as i32;
    let year = now.year();

    let (del, gnd, twr, app, ctr) = match facility {
        2 => (hours, 0.0, 0.0, 0.0, 0.0),
        3 => (0.0, hours, 0.0, 0.0, 0.0),
        4 => (0.0, 0.0, hours, 0.0, 0.0),
        5 => (0.0, 0.0, 0.0, hours, 0.0),
        6 => (0.0, 0.0, 0.0, 0.0, hours),
        _ => (0.0, 0.0, 0.0, 0.0, 0.0),
    };

    if del + gnd + twr + app + ctr == 0.0 {
        return Ok(());
    }

    sqlx::query(
        r#"
        insert into controller_log_months (
            id,
            log_id,
            month,
            year,
            delivery_hours,
            ground_hours,
            tower_hours,
            approach_hours,
            center_hours
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        on conflict (log_id, month, year) do update
        set delivery_hours = controller_log_months.delivery_hours + excluded.delivery_hours,
            ground_hours = controller_log_months.ground_hours + excluded.ground_hours,
            tower_hours = controller_log_months.tower_hours + excluded.tower_hours,
            approach_hours = controller_log_months.approach_hours + excluded.approach_hours,
            center_hours = controller_log_months.center_hours + excluded.center_hours
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(log_id)
    .bind(month)
    .bind(year)
    .bind(del)
    .bind(gnd)
    .bind(twr)
    .bind(app)
    .bind(ctr)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

fn map_facility_type(value: &str) -> i32 {
    match value {
        "ClearanceDelivery" => 2,
        "Ground" => 3,
        "Tower" => 4,
        "ApproachDeparture" => 5,
        "Center" => 6,
        _ => 0,
    }
}

