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
    created_memberships: usize,
    changed_ratings: usize,
    detail_failures: usize,
    desired_missing_local_users: usize,
    warning: Option<String>,
}

enum RosterSyncError {
    DatabaseUnavailable,
    Api(ApiError),
}

#[derive(Debug)]
enum UserDetailFetchError {
    Http(ApiError),
    Decode(reqwest::Error),
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
    cid: i64,
    fname: String,
    lname: String,
    email: Option<String>,
    facility: String,
    rating: i32,
    rating_short: Option<String>,
}

#[derive(Debug, FromRow)]
struct LocalRosterUser {
    user_id: String,
    cid: i64,
    has_membership: bool,
    rating: Option<String>,
    controller_status: Option<String>,
    membership_status: Option<String>,
    home_facility: Option<String>,
    visitor_home_facility: Option<String>,
}

#[derive(Debug, Clone)]
struct DesiredMembership {
    status: MembershipStatus,
    roster_user: VatusaRosterUser,
}

#[derive(Debug, Clone)]
struct MatchedUserUpdate {
    user_id: String,
    cid: i64,
    first_name: String,
    last_name: String,
    full_name: String,
    display_name: String,
    email: Option<String>,
    rating: String,
    controller_status: String,
    join_date: Option<DateTime<Utc>>,
    home_facility: Option<String>,
    visitor_home_facility: Option<String>,
}

#[derive(Debug, Clone)]
struct OffRosterUpdate {
    user_id: String,
    cid: i64,
    controller_status: &'static str,
    membership_status: &'static str,
    is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MembershipChangeSummary {
    created_membership: bool,
    rating_changed: bool,
    controller_status_changed: bool,
    membership_status_changed: bool,
    home_facility_changed: bool,
    visitor_home_facility_changed: bool,
}

impl MembershipChangeSummary {
    fn has_meaningful_change(&self) -> bool {
        self.created_membership
            || self.rating_changed
            || self.controller_status_changed
            || self.membership_status_changed
            || self.home_facility_changed
            || self.visitor_home_facility_changed
    }
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
                facility_id = config.facility_id.as_str(),
                "starting roster sync worker"
            );

            tokio::spawn(async move {
                let mut ticker =
                    tokio::time::interval(std::time::Duration::from_secs(config.interval_secs));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    ticker.tick().await;
                    let started_at = Utc::now();

                    tracing::info!(
                        facility_id = config.facility_id.as_str(),
                        interval_secs = config.interval_secs,
                        started_at = %started_at,
                        "roster sync started"
                    );

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
                                facility_id = config.facility_id.as_str(),
                                processed = result.processed,
                                matched = result.matched,
                                updated = result.updated,
                                demoted = result.demoted,
                                skipped = result.skipped,
                                created_memberships = result.created_memberships,
                                changed_ratings = result.changed_ratings,
                                detail_failures = result.detail_failures,
                                desired_missing_local_users = result.desired_missing_local_users,
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

                            tracing::warn!(
                                facility_id = config.facility_id.as_str(),
                                ?err,
                                "roster sync failed"
                            );
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

    let home_roster = fetch_roster(&client, config, "home").await.map_err(|err| {
        tracing::warn!(
            facility_id = config.facility_id.as_str(),
            stage = "fetch_home_roster",
            ?err,
            "roster sync stage failed"
        );
        RosterSyncError::Api(err)
    })?;
    let visiting_roster = fetch_roster(&client, config, "visit")
        .await
        .map_err(|err| {
            tracing::warn!(
                facility_id = config.facility_id.as_str(),
                stage = "fetch_visit_roster",
                ?err,
                "roster sync stage failed"
            );
            RosterSyncError::Api(err)
        })?;
    let desired_memberships = build_desired_memberships(home_roster, visiting_roster);

    let local_users = fetch_local_sync_candidates(pool).await.map_err(|err| {
        tracing::warn!(
            facility_id = config.facility_id.as_str(),
            stage = "fetch_local_sync_candidates",
            ?err,
            "roster sync stage failed"
        );
        RosterSyncError::Api(err)
    })?;
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
            facility_id = config.facility_id.as_str(),
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
            if local_user.has_membership {
                off_roster_updates.push(build_off_roster_update(local_user));
            }
            continue;
        };

        match fetch_user_detail(&client, config, local_user.cid).await {
            Ok(detail) => matched_updates.push(build_matched_update(local_user, desired, detail)),
            Err(UserDetailFetchError::Http(err)) => {
                failed_detail_cids.push(local_user.cid);
                tracing::warn!(
                    facility_id = config.facility_id.as_str(),
                    stage = "fetch_user_detail",
                    cid = local_user.cid,
                    ?err,
                    "failed to fetch vatusa user detail"
                );
            }
            Err(UserDetailFetchError::Decode(error)) => {
                failed_detail_cids.push(local_user.cid);
                tracing::warn!(
                    facility_id = config.facility_id.as_str(),
                    stage = "decode_user_detail",
                    cid = local_user.cid,
                    ?error,
                    "failed to decode vatusa user detail"
                );
            }
        }
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|_| RosterSyncError::Api(ApiError::Internal))?;

    let mut created_memberships = 0;
    let mut changed_ratings = 0;

    for update in &matched_updates {
        let local_user = local_users
            .iter()
            .find(|user| user.user_id == update.user_id)
            .expect("matched update must have local user context");
        let change_summary = summarize_membership_change(local_user, update);

        apply_matched_update(&mut tx, update, &config.facility_id)
            .await
            .map_err(|err| {
                tracing::warn!(
                    facility_id = config.facility_id.as_str(),
                    stage = "persist_memberships",
                    cid = update.cid,
                    user_id = update.user_id.as_str(),
                    ?err,
                    "roster sync stage failed"
                );
                RosterSyncError::Api(err)
            })?;

        if change_summary.created_membership {
            created_memberships += 1;
        }

        if change_summary.rating_changed {
            changed_ratings += 1;
        }

        if change_summary.has_meaningful_change() {
            tracing::info!(
                cid = update.cid,
                user_id = update.user_id.as_str(),
                change = "membership_upserted",
                created_membership = change_summary.created_membership,
                old_rating = local_user.rating.as_deref(),
                new_rating = update.rating.as_str(),
                old_controller_status = local_user.controller_status.as_deref(),
                new_controller_status = update.controller_status.as_str(),
                old_membership_status = local_user.membership_status.as_deref(),
                new_membership_status = "ACTIVE",
                old_home_facility = local_user.home_facility.as_deref(),
                new_home_facility = update.home_facility.as_deref(),
                old_visitor_home_facility = local_user.visitor_home_facility.as_deref(),
                new_visitor_home_facility = update.visitor_home_facility.as_deref(),
                "roster sync applied membership changes"
            );
        }
    }

    for update in &off_roster_updates {
        apply_off_roster_update(&mut tx, update)
            .await
            .map_err(|err| {
                tracing::warn!(
                    facility_id = config.facility_id.as_str(),
                    stage = "persist_memberships",
                    cid = update.cid,
                    user_id = update.user_id.as_str(),
                    ?err,
                    "roster sync stage failed"
                );
                RosterSyncError::Api(err)
            })?;

        tracing::info!(
            cid = update.cid,
            user_id = update.user_id.as_str(),
            change = "membership_demoted",
            new_controller_status = update.controller_status,
            new_membership_status = update.membership_status,
            is_active = update.is_active,
            "roster sync demoted off-roster membership"
        );
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
    .map_err(|_| {
        tracing::warn!(
            facility_id = config.facility_id.as_str(),
            stage = "update_sync_times",
            "roster sync stage failed"
        );
        RosterSyncError::Api(ApiError::Internal)
    })?;

    tx.commit().await.map_err(|_| {
        tracing::warn!(
            facility_id = config.facility_id.as_str(),
            stage = "persist_memberships",
            "roster sync commit failed"
        );
        RosterSyncError::Api(ApiError::Internal)
    })?;

    Ok(RosterSyncRunResult {
        processed,
        matched,
        updated: matched_updates.len(),
        demoted: off_roster_updates.len(),
        skipped: failed_detail_cids.len(),
        created_memberships,
        changed_ratings,
        detail_failures: failed_detail_cids.len(),
        desired_missing_local_users: unknown_cids.len(),
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
) -> Result<VatusaUserDetail, UserDetailFetchError> {
    let url = format!("{}/user/{}", config.api_base_url, cid);

    send_vatusa_request(client, &url, &config.api_key)
        .await
        .map_err(UserDetailFetchError::Http)?
        .json::<ApiEnvelope<VatusaUserDetail>>()
        .await
        .map(|body| body.data)
        .map_err(|error| {
            tracing::warn!(cid, ?error, "failed to parse vatusa user response");
            UserDetailFetchError::Decode(error)
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

async fn fetch_local_sync_candidates(
    pool: &sqlx::PgPool,
) -> Result<Vec<LocalRosterUser>, ApiError> {
    sqlx::query_as::<_, LocalRosterUser>(
        r#"
        select
            u.id as user_id,
            u.cid,
            (m.user_id is not null) as has_membership,
            m.rating,
            m.controller_status,
            m.membership_status,
            m.home_facility,
            m.visitor_home_facility
        from identity.users u
        left join org.memberships m on m.user_id = u.id
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
    if detail.cid != local_user.cid {
        tracing::warn!(
            expected_cid = local_user.cid,
            actual_cid = detail.cid,
            "vatusa user detail cid did not match local user cid"
        );
    }
    let first_name = detail.fname.trim().to_string();
    let last_name = detail.lname.trim().to_string();
    let full_name = build_full_name(&first_name, &last_name);
    let join_date = desired
        .roster_user
        .facility_join
        .as_deref()
        .and_then(|value| parse_timestamp(value, local_user.cid));

    let (home_facility, visitor_home_facility) = match desired.status {
        MembershipStatus::Home => (Some(desired.roster_user.facility.clone()), None),
        MembershipStatus::Visitor => (None, Some(detail.facility.trim().to_string())),
    };

    MatchedUserUpdate {
        user_id: local_user.user_id.clone(),
        cid: local_user.cid,
        first_name,
        last_name,
        full_name: full_name.clone(),
        display_name: full_name,
        email: detail
            .email
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        rating: resolve_rating_code(
            detail.rating_short.as_deref(),
            detail.rating,
            local_user.cid,
        ),
        controller_status: desired.status.as_db_value().to_string(),
        join_date,
        home_facility,
        visitor_home_facility,
    }
}

fn build_off_roster_update(local_user: &LocalRosterUser) -> OffRosterUpdate {
    OffRosterUpdate {
        user_id: local_user.user_id.clone(),
        cid: local_user.cid,
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
        set email = coalesce($2, email),
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
        insert into org.memberships (
            user_id,
            artcc,
            division,
            rating,
            controller_status,
            membership_status,
            is_active,
            join_date,
            home_facility,
            visitor_home_facility,
            updated_at
        )
        values ($1, $2, 'USA', $3, $4, 'ACTIVE', true, $5, $6, $7, now())
        on conflict (user_id) do update
        set artcc = excluded.artcc,
            division = excluded.division,
            rating = excluded.rating,
            controller_status = excluded.controller_status,
            membership_status = 'ACTIVE',
            is_active = true,
            join_date = coalesce(excluded.join_date, org.memberships.join_date),
            home_facility = excluded.home_facility,
            visitor_home_facility = excluded.visitor_home_facility,
            updated_at = now()
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

fn summarize_membership_change(
    local_user: &LocalRosterUser,
    update: &MatchedUserUpdate,
) -> MembershipChangeSummary {
    MembershipChangeSummary {
        created_membership: !local_user.has_membership,
        rating_changed: local_user.rating.as_deref() != Some(update.rating.as_str()),
        controller_status_changed: local_user.controller_status.as_deref()
            != Some(update.controller_status.as_str()),
        membership_status_changed: local_user.membership_status.as_deref() != Some("ACTIVE"),
        home_facility_changed: local_user.home_facility != update.home_facility,
        visitor_home_facility_changed: local_user.visitor_home_facility
            != update.visitor_home_facility,
    }
}

fn parse_timestamp(value: &str, cid: i64) -> Option<DateTime<Utc>> {
    match chrono::DateTime::parse_from_rfc3339(value) {
        Ok(parsed) => Some(parsed.with_timezone(&Utc)),
        Err(error) => {
            tracing::warn!(
                cid,
                raw_value = value,
                ?error,
                "failed to parse vatusa facility_join timestamp"
            );
            None
        }
    }
}

fn build_full_name(first_name: &str, last_name: &str) -> String {
    format!("{} {}", first_name.trim(), last_name.trim())
        .trim()
        .to_string()
}

fn resolve_rating_code(rating_short: Option<&str>, rating: i32, cid: i64) -> String {
    if let Some(code) = normalize_rating_code(rating_short) {
        return code;
    }

    numeric_rating_to_short_code(rating, cid).to_string()
}

fn normalize_rating_code(value: Option<&str>) -> Option<String> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }

    let upper = raw.to_ascii_uppercase();
    let normalized = match upper.as_str() {
        "OBS" | "OBSERVER" => "OBS",
        "S1" | "STUDENT1" => "S1",
        "S2" | "STUDENT2" => "S2",
        "S3" | "STUDENT3" | "SENIORSTUDENT" => "S3",
        "C1" | "CONTROLLER1" => "C1",
        "C2" | "CONTROLLER2" => "C2",
        "C3" | "CONTROLLER3" => "C3",
        "I1" | "INSTRUCTOR1" => "I1",
        "I2" | "INSTRUCTOR2" => "I2",
        "I3" | "INS" | "INSTRUCTOR" | "INSTRUCTOR3" => "I3",
        "SUP" | "SUPERVISOR" => "SUP",
        "ADM" | "ADMIN" | "ADMINISTRATOR" => "ADM",
        other => other,
    };

    Some(normalized.to_string())
}

fn numeric_rating_to_short_code(value: i32, cid: i64) -> &'static str {
    match value {
        1 => "OBS",
        2 => "S1",
        3 => "S2",
        4 => "S3",
        5 => "C1",
        6 => "C2",
        7 => "C3",
        8 => "I1",
        9 => "I2",
        10 => "I3",
        11 => "SUP",
        12 => "ADM",
        _ => {
            tracing::warn!(cid, rating = value, "unknown vatusa numeric rating");
            "SUS"
        }
    }
}

fn format_roster_sync_error(err: &ApiError) -> String {
    match err {
        ApiError::BadRequest => "bad request".to_string(),
        ApiError::OAuthLoginOriginMismatch => "oauth login origin mismatch".to_string(),
        ApiError::OAuthStateCookieMissing => "oauth state cookie missing".to_string(),
        ApiError::OAuthStateMismatch => "oauth state mismatch".to_string(),
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
            cid: 1001,
            fname: "Jane".to_string(),
            lname: "Controller".to_string(),
            email: Some("jane@example.com".to_string()),
            facility: facility.to_string(),
            rating: 4,
            rating_short: Some("S3".to_string()),
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
            has_membership: true,
            rating: Some("S2".to_string()),
            controller_status: Some("VISITOR".to_string()),
            membership_status: Some("ACTIVE".to_string()),
            home_facility: None,
            visitor_home_facility: Some("ZOB".to_string()),
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
        assert_eq!(update.email.as_deref(), Some("jane@example.com"));
        assert_eq!(update.rating, "S3");
        assert_eq!(update.controller_status, "VISITOR");
        assert_eq!(update.home_facility, None);
        assert_eq!(update.visitor_home_facility, Some("ZNY".to_string()));
        assert!(update.join_date.is_some());
    }

    #[test]
    fn visitor_detail_uses_home_facility_from_user_detail() {
        let local = LocalRosterUser {
            user_id: "user-8".to_string(),
            cid: 8008,
            has_membership: true,
            rating: Some("S2".to_string()),
            controller_status: Some("VISITOR".to_string()),
            membership_status: Some("ACTIVE".to_string()),
            home_facility: None,
            visitor_home_facility: Some("ZOB".to_string()),
        };
        let desired = DesiredMembership {
            status: MembershipStatus::Visitor,
            roster_user: roster_user(8008, "ZDC", Some("2024-01-15T10:00:00Z")),
        };

        let update = build_matched_update(&local, &desired, user_detail("ZNY"));

        assert_eq!(update.visitor_home_facility.as_deref(), Some("ZNY"));
    }

    #[test]
    fn rating_short_from_vatusa_is_preferred() {
        let local = LocalRosterUser {
            user_id: "user-6".to_string(),
            cid: 6006,
            has_membership: true,
            rating: Some("C1".to_string()),
            controller_status: Some("VISITOR".to_string()),
            membership_status: Some("ACTIVE".to_string()),
            home_facility: None,
            visitor_home_facility: Some("ZBW".to_string()),
        };
        let desired = DesiredMembership {
            status: MembershipStatus::Visitor,
            roster_user: roster_user(6006, "ZDC", Some("2024-01-15T10:00:00Z")),
        };

        let update = build_matched_update(
            &local,
            &desired,
            VatusaUserDetail {
                cid: 6006,
                fname: "Jane".to_string(),
                lname: "Controller".to_string(),
                email: Some("jane@example.com".to_string()),
                facility: "ZBW".to_string(),
                rating: 8,
                rating_short: Some("I1".to_string()),
            },
        );

        assert_eq!(update.rating, "I1");
    }

    #[test]
    fn numeric_rating_fallback_maps_instructor_levels_exactly() {
        assert_eq!(numeric_rating_to_short_code(1, 1), "OBS");
        assert_eq!(numeric_rating_to_short_code(5, 1), "C1");
        assert_eq!(numeric_rating_to_short_code(8, 1), "I1");
        assert_eq!(numeric_rating_to_short_code(9, 1), "I2");
        assert_eq!(numeric_rating_to_short_code(10, 1), "I3");
        assert_eq!(numeric_rating_to_short_code(11, 1), "SUP");
        assert_eq!(numeric_rating_to_short_code(12, 1), "ADM");
    }

    #[test]
    fn blank_rating_short_falls_back_to_numeric_rating() {
        assert_eq!(resolve_rating_code(Some(" "), 5, 1), "C1");
    }

    #[test]
    fn unknown_numeric_rating_uses_fallback() {
        assert_eq!(numeric_rating_to_short_code(0, 1), "SUS");
    }

    #[test]
    fn null_email_does_not_clear_local_email() {
        let local = LocalRosterUser {
            user_id: "user-7".to_string(),
            cid: 7007,
            has_membership: true,
            rating: Some("S2".to_string()),
            controller_status: Some("HOME".to_string()),
            membership_status: Some("ACTIVE".to_string()),
            home_facility: Some("ZDC".to_string()),
            visitor_home_facility: None,
        };
        let desired = DesiredMembership {
            status: MembershipStatus::Home,
            roster_user: roster_user(7007, "ZDC", Some("2024-01-15T10:00:00Z")),
        };

        let update = build_matched_update(
            &local,
            &desired,
            VatusaUserDetail {
                cid: 7007,
                fname: "Jane".to_string(),
                lname: "Controller".to_string(),
                email: None,
                facility: "ZDC".to_string(),
                rating: 4,
                rating_short: Some("S3".to_string()),
            },
        );

        assert_eq!(update.email, None);
    }

    #[test]
    fn off_roster_mapping_clears_only_membership_fields() {
        let local = LocalRosterUser {
            user_id: "user-2".to_string(),
            cid: 2002,
            has_membership: true,
            rating: Some("C1".to_string()),
            controller_status: Some("HOME".to_string()),
            membership_status: Some("ACTIVE".to_string()),
            home_facility: Some("ZDC".to_string()),
            visitor_home_facility: None,
        };

        let update = build_off_roster_update(&local);

        assert_eq!(update.controller_status, "NONE");
        assert_eq!(update.membership_status, "INACTIVE");
        assert!(!update.is_active);
    }

    #[test]
    fn summarize_membership_change_detects_created_membership_and_rating_change() {
        let local = LocalRosterUser {
            user_id: "user-3".to_string(),
            cid: 3003,
            has_membership: false,
            rating: None,
            controller_status: None,
            membership_status: None,
            home_facility: None,
            visitor_home_facility: None,
        };
        let update = MatchedUserUpdate {
            user_id: local.user_id.clone(),
            cid: local.cid,
            first_name: "Jane".to_string(),
            last_name: "Controller".to_string(),
            full_name: "Jane Controller".to_string(),
            display_name: "Jane Controller".to_string(),
            email: Some("jane@example.com".to_string()),
            rating: "S3".to_string(),
            controller_status: "HOME".to_string(),
            join_date: None,
            home_facility: Some("ZDC".to_string()),
            visitor_home_facility: None,
        };

        let summary = summarize_membership_change(&local, &update);

        assert!(summary.created_membership);
        assert!(summary.rating_changed);
        assert!(summary.controller_status_changed);
        assert!(summary.membership_status_changed);
        assert!(summary.home_facility_changed);
        assert!(!summary.visitor_home_facility_changed);
        assert!(summary.has_meaningful_change());
    }

    #[test]
    fn summarize_membership_change_detects_noop_update() {
        let local = LocalRosterUser {
            user_id: "user-4".to_string(),
            cid: 4004,
            has_membership: true,
            rating: Some("S3".to_string()),
            controller_status: Some("HOME".to_string()),
            membership_status: Some("ACTIVE".to_string()),
            home_facility: Some("ZDC".to_string()),
            visitor_home_facility: None,
        };
        let update = MatchedUserUpdate {
            user_id: local.user_id.clone(),
            cid: local.cid,
            first_name: "Jane".to_string(),
            last_name: "Controller".to_string(),
            full_name: "Jane Controller".to_string(),
            display_name: "Jane Controller".to_string(),
            email: Some("jane@example.com".to_string()),
            rating: "S3".to_string(),
            controller_status: "HOME".to_string(),
            join_date: None,
            home_facility: Some("ZDC".to_string()),
            visitor_home_facility: None,
        };

        let summary = summarize_membership_change(&local, &update);

        assert!(!summary.created_membership);
        assert!(!summary.rating_changed);
        assert!(!summary.controller_status_changed);
        assert!(!summary.membership_status_changed);
        assert!(!summary.home_facility_changed);
        assert!(!summary.visitor_home_facility_changed);
        assert!(!summary.has_meaningful_change());
    }

    #[test]
    fn matched_user_update_home_sets_home_facility() {
        let local = LocalRosterUser {
            user_id: "user-5".to_string(),
            cid: 5005,
            has_membership: true,
            rating: Some("S2".to_string()),
            controller_status: Some("VISITOR".to_string()),
            membership_status: Some("ACTIVE".to_string()),
            home_facility: None,
            visitor_home_facility: Some("ZNY".to_string()),
        };
        let desired = DesiredMembership {
            status: MembershipStatus::Home,
            roster_user: roster_user(5005, "ZDC", Some("2024-02-01T12:00:00Z")),
        };

        let update = build_matched_update(&local, &desired, user_detail("ZNY"));

        assert_eq!(update.controller_status, "HOME");
        assert_eq!(update.home_facility.as_deref(), Some("ZDC"));
        assert_eq!(update.visitor_home_facility, None);
    }

    #[test]
    fn malformed_join_timestamp_is_ignored() {
        assert!(parse_timestamp("not-a-timestamp", 6006).is_none());
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
