use std::collections::HashMap;

use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::{assume_utc, record_warning},
    state::AppState,
    target,
};

const DOMAIN: &str = "stats";
const ENVIRONMENT: &str = "live";

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceControllerLog {
    id: String,
    user_id: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceControllerLogMonth {
    id: String,
    log_id: String,
    month: i32,
    year: i32,
    delivery_hours: f64,
    ground_hours: f64,
    tower_hours: f64,
    approach_hours: f64,
    center_hours: f64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceControllerPosition {
    id: String,
    log_id: String,
    position: String,
    facility: Option<i32>,
    start: NaiveDateTime,
    end: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize)]
struct ControllerLogPayload {
    user_id: String,
    cid: i64,
}

pub async fn migrate(state: &mut AppState) -> Result<()> {
    let logs = sqlx::query_as::<_, SourceControllerLog>(
        r#"
        select id, "userId" as user_id
        from public."ControllerLog"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let months = sqlx::query_as::<_, SourceControllerLogMonth>(
        r#"
        select
            id,
            "logId" as log_id,
            month,
            year,
            "deliveryHours" as delivery_hours,
            "groundHours" as ground_hours,
            "towerHours" as tower_hours,
            "approachHours" as approach_hours,
            "centerHours" as center_hours
        from public."ControllerLogMonth"
        order by year asc, month asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    let mut log_to_cid = HashMap::<String, i64>::new();
    let mut log_to_user = HashMap::<String, String>::new();
    for log in logs {
        let resolved_user_id = match resolve_target_user_id(state, &log.user_id).await? {
            Some(user_id) => user_id,
            None => {
                record_warning(
                    state,
                    DOMAIN,
                    "controller_log",
                    &log.id,
                    format!(
                        "skipping controller log because user {} did not migrate",
                        log.user_id
                    ),
                )
                .await?;
                continue;
            }
        };

        let cid = sqlx::query_scalar::<_, i64>(r#"select cid from identity.users where id = $1"#)
            .bind(&resolved_user_id)
            .fetch_optional(&state.target)
            .await?;

        let Some(cid) = cid else {
            record_warning(
                state,
                DOMAIN,
                "controller_log",
                &log.id,
                format!(
                    "skipping controller log because migrated user {} is missing target cid",
                    resolved_user_id
                ),
            )
            .await?;
            continue;
        };

        if !state.config.dry_run {
            let target_id = format!("{ENVIRONMENT}:{cid}");
            let source_business_key = format!("user:{resolved_user_id}");
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "controller_log",
                &log.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                "updated",
                &ControllerLogPayload {
                    user_id: resolved_user_id.clone(),
                    cid,
                },
            )
            .await?;
        }

        log_to_user.insert(log.id.clone(), resolved_user_id);
        log_to_cid.insert(log.id, cid);
    }

    for row in months {
        let Some(&cid) = log_to_cid.get(&row.log_id) else {
            record_warning(
                state,
                DOMAIN,
                "controller_log_month",
                &row.id,
                format!(
                    "skipping controller log month because controller log {} did not resolve",
                    row.log_id
                ),
            )
            .await?;
            continue;
        };

        state.report.domain_mut(DOMAIN).planned += 1;
        if !validate_month_row(state, &row).await? {
            state.report.domain_mut(DOMAIN).skipped += 1;
            continue;
        }

        let delivery_seconds = hours_to_seconds(row.delivery_hours)?;
        let ground_seconds = hours_to_seconds(row.ground_hours)?;
        let tower_seconds = hours_to_seconds(row.tower_hours)?;
        let tracon_seconds = hours_to_seconds(row.approach_hours)?;
        let center_seconds = hours_to_seconds(row.center_hours)?;
        let online_seconds =
            delivery_seconds + ground_seconds + tower_seconds + tracon_seconds + center_seconds;

        let existed = sqlx::query_scalar::<_, bool>(
            r#"
            select exists(
                select 1
                from stats.controller_monthly_rollups
                where environment = $1 and cid = $2 and year = $3 and month = $4
            )
            "#,
        )
        .bind(ENVIRONMENT)
        .bind(cid)
        .bind(row.year)
        .bind(row.month)
        .fetch_one(&state.target)
        .await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into stats.controller_monthly_rollups (
                    environment, cid, year, month,
                    online_seconds, delivery_seconds, ground_seconds,
                    tower_seconds, tracon_seconds, center_seconds
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                on conflict (environment, cid, year, month) do update set
                    online_seconds = excluded.online_seconds,
                    delivery_seconds = excluded.delivery_seconds,
                    ground_seconds = excluded.ground_seconds,
                    tower_seconds = excluded.tower_seconds,
                    tracon_seconds = excluded.tracon_seconds,
                    center_seconds = excluded.center_seconds
                "#,
            )
            .bind(ENVIRONMENT)
            .bind(cid)
            .bind(row.year)
            .bind(row.month)
            .bind(online_seconds)
            .bind(delivery_seconds)
            .bind(ground_seconds)
            .bind(tower_seconds)
            .bind(tracon_seconds)
            .bind(center_seconds)
            .execute(&state.target)
            .await?;

            let target_id = format!("{ENVIRONMENT}:{cid}:{}:{}", row.year, row.month);
            let source_business_key = target_id.clone();
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "controller_log_month",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        let report = state.report.domain_mut(DOMAIN);
        if existed {
            report.updated += 1;
        } else {
            report.created += 1;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "controller_monthly_rollups",
        )
        .await?;
    }

    migrate_positions(state, &log_to_cid, &log_to_user).await?;

    Ok(())
}

/// Backfills legacy `ControllerPosition` rows as closed sessions plus one
/// activation each, which is what `GET /stats/controller/{cid}/positions`
/// lists. The rollups above carry the hours; without this, migrated periods
/// show hours but no controlling sessions.
async fn migrate_positions(
    state: &mut AppState,
    log_to_cid: &HashMap<String, i64>,
    log_to_user: &HashMap<String, String>,
) -> Result<()> {
    let positions = sqlx::query_as::<_, SourceControllerPosition>(
        r#"
        select id, "logId" as log_id, position, facility, start, "end"
        from public."ControllerPosition"
        order by start asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    // Never overlap what osmium's own sync recorded: from the first login it
    // saw, the target is authoritative.
    let live_since = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        r#"
        select min(login_at) from stats.controller_sessions
        where environment = $1 and source_login_time_raw not like 'legacy:%'
        "#,
    )
    .bind(ENVIRONMENT)
    .fetch_one(&state.target)
    .await?;

    for row in positions {
        let (Some(&cid), Some(user_id)) =
            (log_to_cid.get(&row.log_id), log_to_user.get(&row.log_id))
        else {
            record_warning(
                state,
                DOMAIN,
                "controller_position",
                &row.id,
                format!(
                    "skipping controller position because controller log {} did not resolve",
                    row.log_id
                ),
            )
            .await?;
            continue;
        };

        state.report.domain_mut(DOMAIN).planned += 1;
        let started_at = assume_utc(row.start);
        let ended_at = row.end.map(assume_utc);
        let Some(ended_at) = ended_at else {
            skip_position(state, &row.id, "still open in the legacy data").await?;
            continue;
        };
        let Some(position_type) = legacy_position_type(row.facility) else {
            let reason = format!("facility {:?} is not a counted position", row.facility);
            skip_position(state, &row.id, &reason).await?;
            continue;
        };
        if live_since.is_some_and(|since| ended_at > since) {
            let reason = "overlaps activations recorded by osmium's own sync";
            skip_position(state, &row.id, reason).await?;
            continue;
        }

        if target::find_mapping(&state.target, "controller_position", &row.id)
            .await?
            .is_some()
        {
            state.report.domain_mut(DOMAIN).skipped += 1;
            continue;
        }

        if !state.config.dry_run {
            let seconds = (ended_at - started_at).num_seconds().max(0);
            let source_id = format!("legacy:{}", row.id);
            // Both writes key on the `legacy:<id>` marker rather than the
            // mapping, so a run that died between them can be re-run safely.
            let existing = sqlx::query_scalar::<_, String>(
                r#"
                select id from stats.controller_sessions
                where environment = $1 and source_login_time_raw = $2
                "#,
            )
            .bind(ENVIRONMENT)
            .bind(&source_id)
            .fetch_optional(&state.target)
            .await?;
            let session_id = match existing {
                Some(id) => id,
                None => {
                    let inserted = sqlx::query_scalar::<_, String>(
                        r#"
                        insert into stats.controller_sessions (
                            environment, artcc_id, cid, user_id, login_at, logout_at,
                            online_seconds, source_login_time_raw
                        )
                        values ($1, 'ZDC', $2, $3, $4, $5, $6, $7)
                        on conflict (environment, cid, login_at) do nothing
                        returning id
                        "#,
                    )
                    .bind(ENVIRONMENT)
                    .bind(cid)
                    .bind(user_id)
                    .bind(started_at)
                    .bind(ended_at)
                    .bind(seconds)
                    .bind(&source_id)
                    .fetch_optional(&state.target)
                    .await?;
                    let Some(id) = inserted else {
                        let reason = "another session already starts at the same time";
                        skip_position(state, &row.id, reason).await?;
                        continue;
                    };
                    id
                }
            };

            sqlx::query(
                r#"
                insert into stats.controller_activations (
                    session_id, environment, cid, position_id, facility_name, position_name,
                    position_type, default_callsign, is_primary, started_at, ended_at,
                    active_seconds
                )
                select $1, $2, $3, $4, $5, $6, $7, $6, true, $8, $9, $10
                where not exists (
                    select 1 from stats.controller_activations
                    where environment = $2 and position_id = $4
                )
                "#,
            )
            .bind(&session_id)
            .bind(ENVIRONMENT)
            .bind(cid)
            .bind(&source_id)
            .bind(legacy_facility_name(&row.position))
            .bind(&row.position)
            .bind(position_type)
            .bind(started_at)
            .bind(ended_at)
            .bind(seconds)
            .execute(&state.target)
            .await?;

            let target_id = format!("{ENVIRONMENT}:{cid}:{}", row.id);
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "controller_position",
                &row.id,
                &target_id,
                &target_id,
                &target_id,
                "created",
                &row,
            )
            .await?;
        }
        state.report.domain_mut(DOMAIN).created += 1;
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "controller_activations",
        )
        .await?;
    }
    Ok(())
}

async fn skip_position(state: &mut AppState, source_id: &str, reason: &str) -> Result<()> {
    record_warning(
        state,
        DOMAIN,
        "controller_position",
        source_id,
        format!("skipping controller position: {reason}"),
    )
    .await?;
    state.report.domain_mut(DOMAIN).skipped += 1;
    Ok(())
}

/// Legacy stored the VATSIM datafeed facility code (2 DEL, 3 GND, 4 TWR, 5 APP,
/// 6 CTR; 0 OBS and 1 FSS were never counted). Maps to the `position_type`
/// values the stats queries bucket on.
fn legacy_position_type(facility: Option<i32>) -> Option<&'static str> {
    match facility? {
        2 => Some("Delivery"),
        3 => Some("Ground"),
        4 => Some("Tower"),
        5 => Some("Tracon"),
        6 => Some("Artcc"),
        _ => None,
    }
}

/// The facility part of a VATSIM callsign: `DCA_N_GND` → `DCA`.
fn legacy_facility_name(callsign: &str) -> String {
    callsign
        .split('_')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(callsign)
        .to_string()
}

async fn resolve_target_user_id(
    state: &mut AppState,
    source_user_id: &str,
) -> Result<Option<String>> {
    if let Some(mapping) = target::find_mapping(&state.target, "user", source_user_id).await? {
        return Ok(Some(mapping.target_id));
    }

    let exists = sqlx::query_scalar::<_, bool>(
        r#"select exists(select 1 from identity.users where id = $1)"#,
    )
    .bind(source_user_id)
    .fetch_one(&state.target)
    .await?;
    if exists {
        Ok(Some(source_user_id.to_string()))
    } else {
        Ok(None)
    }
}

async fn validate_month_row(state: &mut AppState, row: &SourceControllerLogMonth) -> Result<bool> {
    if !(0..=11).contains(&row.month) {
        let message = format!("invalid month {} for controller log month", row.month);
        if state.config.strict {
            bail!("{} {}", row.id, message);
        }
        record_warning(state, DOMAIN, "controller_log_month", &row.id, message).await?;
        return Ok(false);
    }
    if !(2000..=2100).contains(&row.year) {
        let message = format!("invalid year {} for controller log month", row.year);
        if state.config.strict {
            bail!("{} {}", row.id, message);
        }
        record_warning(state, DOMAIN, "controller_log_month", &row.id, message).await?;
        return Ok(false);
    }
    for (name, value) in [
        ("deliveryHours", row.delivery_hours),
        ("groundHours", row.ground_hours),
        ("towerHours", row.tower_hours),
        ("approachHours", row.approach_hours),
        ("centerHours", row.center_hours),
    ] {
        if !value.is_finite() || value < 0.0 {
            let message = format!("invalid {name} value {value}");
            if state.config.strict {
                bail!("{} {}", row.id, message);
            }
            record_warning(state, DOMAIN, "controller_log_month", &row.id, message).await?;
            return Ok(false);
        }
    }
    Ok(true)
}

fn hours_to_seconds(hours: f64) -> Result<i64> {
    if !hours.is_finite() {
        bail!("stats hour value is not finite");
    }
    if hours < 0.0 {
        bail!("stats hour value is negative");
    }
    Ok((hours * 3600.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::{legacy_facility_name, legacy_position_type};

    #[test]
    fn maps_vatsim_facility_codes_to_counted_position_types() {
        assert_eq!(legacy_position_type(Some(2)), Some("Delivery"));
        assert_eq!(legacy_position_type(Some(3)), Some("Ground"));
        assert_eq!(legacy_position_type(Some(5)), Some("Tracon"));
        assert_eq!(legacy_position_type(Some(6)), Some("Artcc"));
        assert_eq!(legacy_position_type(Some(0)), None);
        assert_eq!(legacy_position_type(Some(1)), None);
        assert_eq!(legacy_position_type(None), None);
    }

    #[test]
    fn takes_the_facility_from_the_callsign() {
        assert_eq!(legacy_facility_name("DCA_N_GND"), "DCA");
        assert_eq!(legacy_facility_name("DC_CTR"), "DC");
        assert_eq!(legacy_facility_name("NOUNDERSCORE"), "NOUNDERSCORE");
    }
}
