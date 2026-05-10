use std::collections::HashMap;

use anyhow::{Result, bail};
use serde::Serialize;
use sqlx::FromRow;

use crate::{helpers::record_warning, state::AppState, target};

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

    Ok(())
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
