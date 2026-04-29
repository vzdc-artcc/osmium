use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{FromRow, Postgres, Transaction};

use crate::{errors::ApiError, state::AppState};

const DEFAULT_INTERVAL_SECS: u64 = 900;
const DEFAULT_API_BASE_URL: &str = "https://api.vatusa.net/v2";
const DEFAULT_FACILITY_ID: &str = "ZDC";

#[derive(Clone)]
struct RosterSyncConfig {
    api_key: String,
    facility_id: String,
    api_base_url: String,
    interval_secs: u64,
}

#[derive(Debug, Clone, Default)]
struct RosterSyncRunResult {
    processed: usize,
    matched: usize,
    updated: usize,
    demoted: usize,
    skipped: usize,
    warning: Option<String>,
}

enum RosterSyncError {
    DatabaseUnavailable,
    Api(ApiError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MembershipStatus {
    Home,
    Visitor,
}

impl MembershipStatus {
    fn as_db_value(&self) -> &'static str {
        match self {
            Self::Home => "HOME",
            Self::Visitor => "VISITOR",
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    data: T,
}

#[derive(Debug, Clone, Deserialize)]
struct VatusaRosterUser {
    cid: i64,
    facility: String,
    facility_join: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct VatusaUserDetail {
    fname: String,
    lname: String,
    email: String,
    facility: String,
    rating: i32,
}

#[derive(Debug, FromRow)]
struct LocalRosterUser {
    user_id: String,
    cid: i64,
}

#[derive(Debug, Clone)]
struct DesiredMembership {
    status: MembershipStatus,
    roster_user: VatusaRosterUser,
}

#[derive(Debug, Clone)]
struct MatchedUserUpdate {
    user_id: String,
    first_name: String,
    last_name: String,
    full_name: String,
    display_name: String,
    email: String,
    rating: String,
    controller_status: String,
    join_date: Option<DateTime<Utc>>,
    home_facility: Option<String>,
    visitor_home_facility: Option<String>,
}

#[derive(Debug, Clone)]
struct OffRosterUpdate {
    user_id: String,
    controller_status: &'static str,
    membership_status: &'static str,
    is_active: bool,
}

pub fn start_roster_sync_worker(state: AppState) {
    match roster_sync_config_from_env() {
        Ok(Some(config)) => {
            if let Ok(mut health) = state.job_health.write() {
                health.roster_sync.enabled = true;
                health.roster_sync.last_error = None;
            }

            tracing::info!(
                interval_secs = config.interval_secs,
                facility_id = config.facility_id,
                "starting roster sync worker"
            );

            tokio::spawn(async move {
                let mut ticker =
                    tokio::time::interval(std::time::Duration::from_secs(config.interval_secs));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    ticker.tick().await;
                    let started_at = Utc::now();

                    if let Ok(mut health) = state.job_health.write() {
                        health.roster_sync.last_started_at = Some(started_at);
                        health.roster_sync.last_error = None;
                    }

                    match run_roster_sync(state.clone(), &config).await {
                        Ok(result) => {
                            let last_error = result.warning.clone();
                            if let Ok(mut health) = state.job_health.write() {
                                health.roster_sync.last_finished_at = Some(Utc::now());
                                health.roster_sync.last_success_at = Some(Utc::now());
                                health.roster_sync.last_result_ok = Some(last_error.is_none());
                                health.roster_sync.last_error = last_error.clone();
                                health.roster_sync.processed = Some(result.processed);
                                health.roster_sync.matched = Some(result.matched);
                                health.roster_sync.updated = Some(result.updated);
                                health.roster_sync.demoted = Some(result.demoted);
                                health.roster_sync.skipped = Some(result.skipped);
                            }

                            tracing::info!(
                                processed = result.processed,
                                matched = result.matched,
                                updated = result.updated,
                                demoted = result.demoted,
                                skipped = result.skipped,
                                warning = result.warning.as_deref(),
                                "roster sync completed"
                            );
                        }
                        Err(RosterSyncError::DatabaseUnavailable) => {
                            if let Ok(mut health) = state.job_health.write() {
                                health.roster_sync.last_finished_at = Some(Utc::now());
                                health.roster_sync.last_result_ok = Some(false);
                                health.roster_sync.last_error =
                                    Some("database unavailable for roster sync".to_string());
                            }
                        }
                        Err(RosterSyncError::Api(err)) => {
                            let message = format_roster_sync_error(&err);
                            if let Ok(mut health) = state.job_health.write() {
                                health.roster_sync.last_finished_at = Some(Utc::now());
                                health.roster_sync.last_result_ok = Some(false);
                                health.roster_sync.last_error = Some(message.clone());
                            }

                            tracing::warn!(?err, "roster sync failed");
                        }
                    }
                }
            });
        }
        Ok(None) => {
            if let Ok(mut health) = state.job_health.write() {
                health.roster_sync.enabled = false;
                health.roster_sync.last_error = None;
            }
            tracing::info!("roster sync worker disabled");
        }
        Err(message) => {
            if let Ok(mut health) = state.job_health.write() {
                health.roster_sync.enabled = false;
                health.roster_sync.last_result_ok = Some(false);
                health.roster_sync.last_error = Some(message.clone());
            }
            tracing::warn!(message, "roster sync worker disabled");
        }
    }
}

async fn run_roster_sync(
    state: AppState,
    config: &RosterSyncConfig,
) -> Result<RosterSyncRunResult, RosterSyncError> {
    let pool = state
        .db
        .as_ref()
        .ok_or(RosterSyncError::DatabaseUnavailable)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| RosterSyncError::Api(ApiError::Internal))?;

    let home_roster = fetch_roster(&client, config, "home")
        .await
        .map_err(RosterSyncError::Api)?;
    let visiting_roster = fetch_roster(&client, config, "visit")
        .await
        .map_err(RosterSyncError::Api)?;
    let desired_memberships = build_desired_memberships(home_roster, visiting_roster);

    let local_users = fetch_local_roster_users(pool)
        .await
        .map_err(RosterSyncError::Api)?;
    let processed = local_users.len();
    let matched = local_users
        .iter()
        .filter(|user| desired_memberships.contains_key(&user.cid))
        .count();

    let desired_cids = desired_memberships.keys().copied().collect::<HashSet<_>>();
    let local_cids = local_users
        .iter()
        .map(|user| user.cid)
        .collect::<HashSet<_>>();
    let unknown_cids = desired_cids
        .difference(&local_cids)
        .copied()
        .collect::<Vec<_>>();

    if !unknown_cids.is_empty() {
        tracing::info!(
            count = unknown_cids.len(),
            cids = ?unknown_cids,
            "ignoring vatusa roster users missing from local database"
        );
    }

    let mut matched_updates = Vec::new();
    let mut off_roster_updates = Vec::new();
    let mut failed_detail_cids = Vec::new();

    for local_user in &local_users {
        let Some(desired) = desired_memberships.get(&local_user.cid) else {
            off_roster_updates.push(build_off_roster_update(local_user));
            continue;
        };

        match fetch_user_detail(&client, config, local_user.cid).await {
            Ok(detail) => matched_updates.push(build_matched_update(local_user, desired, detail)),
            Err(err) => {
                failed_detail_cids.push(local_user.cid);
                tracing::warn!(
                    cid = local_user.cid,
                    ?err,
                    "failed to fetch vatusa user detail"
                );
            }
        }
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|_| RosterSyncError::Api(ApiError::Internal))?;

    for update in &matched_updates {
        apply_matched_update(&mut tx, update, &config.facility_id)
            .await
            .map_err(RosterSyncError::Api)?;
    }

    for update in &off_roster_updates {
        apply_off_roster_update(&mut tx, update)
            .await
            .map_err(RosterSyncError::Api)?;
    }

    sqlx::query(
        r#"
        update stats.sync_times
        set roster = now(),
            updated_at = now()
        where id = 'default'
        "#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| RosterSyncError::Api(ApiError::Internal))?;

    tx.commit()
        .await
        .map_err(|_| RosterSyncError::Api(ApiError::Internal))?;

    Ok(RosterSyncRunResult {
        processed,
        matched,
        updated: matched_updates.len(),
        demoted: off_roster_updates.len(),
        skipped: failed_detail_cids.len(),
        warning: build_warning_summary(&failed_detail_cids),
    })
}

fn roster_sync_config_from_env() -> Result<Option<RosterSyncConfig>, String> {
    if !roster_sync_enabled() {
        return Ok(None);
    }

    let api_key = match std::env::var("VATUSA_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return Err("missing VATUSA_API_KEY".to_string()),
    };

    Ok(Some(RosterSyncConfig {
        api_key,
        facility_id: std::env::var("VATUSA_FACILITY_ID")
            .ok()
            .map(|value| value.trim().to_ascii_uppercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_FACILITY_ID.to_string()),
        api_base_url: std::env::var("VATUSA_API_BASE_URL")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_API_BASE_URL.to_string()),
        interval_secs: roster_sync_interval_secs(),
    }))
}

fn roster_sync_enabled() -> bool {
    std::env::var("ROSTER_SYNC_ENABLED")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(true)
}

fn roster_sync_interval_secs() -> u64 {
    std::env::var("ROSTER_SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_SECS)
        .max(60)
}

async fn fetch_roster(
    client: &reqwest::Client,
    config: &RosterSyncConfig,
    membership: &str,
) -> Result<Vec<VatusaRosterUser>, ApiError> {
    let url = format!(
        "{}/facility/{}/roster/{}",
        config.api_base_url, config.facility_id, membership
    );

    send_vatusa_request(client, &url, &config.api_key)
        .await?
        .json::<ApiEnvelope<Vec<VatusaRosterUser>>>()
        .await
        .map(|body| body.data)
        .map_err(|error| {
            tracing::warn!(membership, ?error, "failed to parse vatusa roster response");
            ApiError::ServiceUnavailable
        })
}

async fn fetch_user_detail(
    client: &reqwest::Client,
    config: &RosterSyncConfig,
    cid: i64,
) -> Result<VatusaUserDetail, ApiError> {
    let url = format!("{}/user/{}", config.api_base_url, cid);

    send_vatusa_request(client, &url, &config.api_key)
        .await?
        .json::<ApiEnvelope<VatusaUserDetail>>()
        .await
        .map(|body| body.data)
        .map_err(|error| {
            tracing::warn!(cid, ?error, "failed to parse vatusa user response");
            ApiError::ServiceUnavailable
        })
}

async fn send_vatusa_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
) -> Result<reqwest::Response, ApiError> {
    let response = client
        .get(url)
        .query(&[("apikey", api_key)])
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(%url, ?error, "vatusa request failed");
            ApiError::ServiceUnavailable
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        tracing::warn!(%url, %status, body, "vatusa request returned non-success status");
        return Err(ApiError::ServiceUnavailable);
    }

    Ok(response)
}

fn build_desired_memberships(
    home_roster: Vec<VatusaRosterUser>,
    visiting_roster: Vec<VatusaRosterUser>,
) -> HashMap<i64, DesiredMembership> {
    let mut desired = HashMap::new();

    for user in visiting_roster {
        desired.insert(
            user.cid,
            DesiredMembership {
                status: MembershipStatus::Visitor,
                roster_user: user,
            },
        );
    }

    for user in home_roster {
        if desired.contains_key(&user.cid) {
            tracing::warn!(
                cid = user.cid,
                "cid found in both home and visiting rosters"
            );
        }

        desired.insert(
            user.cid,
            DesiredMembership {
                status: MembershipStatus::Home,
                roster_user: user,
            },
        );
    }

    desired
}

async fn fetch_local_roster_users(pool: &sqlx::PgPool) -> Result<Vec<LocalRosterUser>, ApiError> {
    sqlx::query_as::<_, LocalRosterUser>(
        r#"
        select
            u.id as user_id,
            u.cid
        from identity.users u
        join org.memberships m on m.user_id = u.id
        left join identity.user_flags f on f.user_id = u.id
        where u.cid is not null
          and coalesce(f.excluded_from_roster_sync, false) = false
        order by u.cid asc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

fn build_matched_update(
    local_user: &LocalRosterUser,
    desired: &DesiredMembership,
    detail: VatusaUserDetail,
) -> MatchedUserUpdate {
    let first_name = detail.fname.trim().to_string();
    let last_name = detail.lname.trim().to_string();
    let full_name = build_full_name(&first_name, &last_name);
    let join_date = desired
        .roster_user
        .facility_join
        .as_deref()
        .and_then(parse_timestamp);

    let (home_facility, visitor_home_facility) = match desired.status {
        MembershipStatus::Home => (Some(desired.roster_user.facility.clone()), None),
        MembershipStatus::Visitor => (None, Some(detail.facility.trim().to_string())),
    };

    MatchedUserUpdate {
        user_id: local_user.user_id.clone(),
        first_name,
        last_name,
        full_name: full_name.clone(),
        display_name: full_name,
        email: detail.email.trim().to_string(),
        rating: rating_short_from_numeric(detail.rating).to_string(),
        controller_status: desired.status.as_db_value().to_string(),
        join_date,
        home_facility,
        visitor_home_facility,
    }
}

fn build_off_roster_update(local_user: &LocalRosterUser) -> OffRosterUpdate {
    OffRosterUpdate {
        user_id: local_user.user_id.clone(),
        controller_status: "NONE",
        membership_status: "INACTIVE",
        is_active: false,
    }
}

async fn apply_matched_update(
    tx: &mut Transaction<'_, Postgres>,
    update: &MatchedUserUpdate,
    facility_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update identity.users
        set email = $2,
            first_name = $3,
            last_name = $4,
            full_name = $5,
            display_name = $6,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(&update.user_id)
    .bind(&update.email)
    .bind(&update.first_name)
    .bind(&update.last_name)
    .bind(&update.full_name)
    .bind(&update.display_name)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        update org.memberships
        set artcc = $2,
            division = 'USA',
            rating = $3,
            controller_status = $4,
            membership_status = 'ACTIVE',
            is_active = true,
            join_date = coalesce($5, join_date),
            home_facility = $6,
            visitor_home_facility = $7,
            updated_at = now()
        where user_id = $1
        "#,
    )
    .bind(&update.user_id)
    .bind(facility_id)
    .bind(&update.rating)
    .bind(&update.controller_status)
    .bind(update.join_date)
    .bind(&update.home_facility)
    .bind(&update.visitor_home_facility)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

async fn apply_off_roster_update(
    tx: &mut Transaction<'_, Postgres>,
    update: &OffRosterUpdate,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update org.memberships
        set controller_status = $2,
            membership_status = $3,
            is_active = $4,
            home_facility = null,
            visitor_home_facility = null,
            updated_at = now()
        where user_id = $1
        "#,
    )
    .bind(&update.user_id)
    .bind(update.controller_status)
    .bind(update.membership_status)
    .bind(update.is_active)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

fn build_warning_summary(failed_detail_cids: &[i64]) -> Option<String> {
    if failed_detail_cids.is_empty() {
        None
    } else {
        Some(format!(
            "detail fetch failed for {} users",
            failed_detail_cids.len()
        ))
    }
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .ok()
}

fn build_full_name(first_name: &str, last_name: &str) -> String {
    format!("{} {}", first_name.trim(), last_name.trim())
        .trim()
        .to_string()
}

fn rating_short_from_numeric(value: i32) -> &'static str {
    match value {
        1 => "OBS",
        2 => "S1",
        3 => "S2",
        4 => "S3",
        5 => "C1",
        6 => "C2",
        7 => "C3",
        8..=10 => "INS",
        11 => "SUP",
        12 => "ADM",
        _ => "SUS",
    }
}

fn format_roster_sync_error(err: &ApiError) -> String {
    match err {
        ApiError::BadRequest => "bad request".to_string(),
        ApiError::Unauthorized => "unauthorized".to_string(),
        ApiError::ServiceUnavailable => "service unavailable".to_string(),
        ApiError::Internal => "internal error".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }

            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::remove_var(key);
            }

            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.as_deref() {
                unsafe {
                    std::env::set_var(self.key, previous);
                }
            } else {
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    fn env_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn roster_user(cid: i64, facility: &str, facility_join: Option<&str>) -> VatusaRosterUser {
        VatusaRosterUser {
            cid,
            facility: facility.to_string(),
            facility_join: facility_join.map(str::to_string),
        }
    }

    fn user_detail(facility: &str) -> VatusaUserDetail {
        VatusaUserDetail {
            fname: "Jane".to_string(),
            lname: "Controller".to_string(),
            email: "jane@example.com".to_string(),
            facility: facility.to_string(),
            rating: 4,
        }
    }

    #[test]
    fn home_roster_entry_maps_to_home() {
        let desired = build_desired_memberships(vec![roster_user(1, "ZDC", None)], vec![]);
        assert_eq!(
            desired.get(&1).map(|entry| entry.status.clone()),
            Some(MembershipStatus::Home)
        );
    }

    #[test]
    fn visiting_roster_entry_maps_to_visitor() {
        let desired = build_desired_memberships(vec![], vec![roster_user(1, "ZDC", None)]);
        assert_eq!(
            desired.get(&1).map(|entry| entry.status.clone()),
            Some(MembershipStatus::Visitor)
        );
    }

    #[test]
    fn home_wins_when_cid_in_both_lists() {
        let desired = build_desired_memberships(
            vec![roster_user(1, "ZDC", None)],
            vec![roster_user(1, "ABC", None)],
        );

        assert_eq!(
            desired.get(&1).map(|entry| entry.status.clone()),
            Some(MembershipStatus::Home)
        );
    }

    #[test]
    fn matched_user_update_maps_fields() {
        let local = LocalRosterUser {
            user_id: "user-1".to_string(),
            cid: 1001,
        };
        let desired = DesiredMembership {
            status: MembershipStatus::Visitor,
            roster_user: roster_user(1001, "ZDC", Some("2024-01-15T10:00:00Z")),
        };

        let update = build_matched_update(&local, &desired, user_detail("ZNY"));

        assert_eq!(update.first_name, "Jane");
        assert_eq!(update.last_name, "Controller");
        assert_eq!(update.full_name, "Jane Controller");
        assert_eq!(update.display_name, "Jane Controller");
        assert_eq!(update.email, "jane@example.com");
        assert_eq!(update.rating, "S3");
        assert_eq!(update.controller_status, "VISITOR");
        assert_eq!(update.home_facility, None);
        assert_eq!(update.visitor_home_facility, Some("ZNY".to_string()));
        assert!(update.join_date.is_some());
    }

    #[test]
    fn numeric_rating_maps_to_short_code() {
        assert_eq!(rating_short_from_numeric(1), "OBS");
        assert_eq!(rating_short_from_numeric(5), "C1");
        assert_eq!(rating_short_from_numeric(10), "INS");
        assert_eq!(rating_short_from_numeric(11), "SUP");
        assert_eq!(rating_short_from_numeric(12), "ADM");
        assert_eq!(rating_short_from_numeric(0), "SUS");
    }

    #[test]
    fn off_roster_mapping_clears_only_membership_fields() {
        let local = LocalRosterUser {
            user_id: "user-2".to_string(),
            cid: 2002,
        };

        let update = build_off_roster_update(&local);

        assert_eq!(update.controller_status, "NONE");
        assert_eq!(update.membership_status, "INACTIVE");
        assert!(!update.is_active);
    }

    #[test]
    fn worker_config_disabled_when_env_false() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _enabled = EnvVarGuard::set("ROSTER_SYNC_ENABLED", "false");
        let _apikey = EnvVarGuard::set("VATUSA_API_KEY", "secret");

        assert!(matches!(roster_sync_config_from_env(), Ok(None)));
    }

    #[test]
    fn worker_config_disabled_when_api_key_missing() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _enabled = EnvVarGuard::set("ROSTER_SYNC_ENABLED", "true");
        let _apikey = EnvVarGuard::unset("VATUSA_API_KEY");

        assert!(roster_sync_config_from_env().is_err());
    }

    #[test]
    fn worker_config_uses_default_interval() {
        let _env_lock = env_test_lock().lock().unwrap();
        let _enabled = EnvVarGuard::set("ROSTER_SYNC_ENABLED", "true");
        let _apikey = EnvVarGuard::set("VATUSA_API_KEY", "secret");
        let _interval = EnvVarGuard::unset("ROSTER_SYNC_INTERVAL_SECS");

        let config = roster_sync_config_from_env()
            .expect("config lookup should succeed")
            .expect("config should exist");
        assert_eq!(config.interval_secs, DEFAULT_INTERVAL_SECS);
    }
}
