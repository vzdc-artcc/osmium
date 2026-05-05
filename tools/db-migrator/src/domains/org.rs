use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

use crate::{helpers::new_id, mapping::normalize_certification_option, state::AppState, target};

const DOMAIN: &str = "org";

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceVisitorApplication {
    id: String,
    user_id: String,
    home_facility: String,
    why_visit: String,
    status: String,
    reason_for_denial: Option<String>,
    submitted_at: DateTime<Utc>,
    decided_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLoa {
    id: String,
    user_id: String,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    reason: String,
    status: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceUserCertification {
    id: String,
    user_id: String,
    certification_type_id: String,
    certification_option: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceSoloCertification {
    id: String,
    user_id: String,
    certification_type_id: String,
    position: String,
    expires: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceSuaBlock {
    id: String,
    user_id: String,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    afiliation: String,
    details: String,
    mission_number: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceSuaAirspace {
    id: String,
    sua_block_id: String,
    identifier: String,
    bottom_altitude: String,
    top_altitude: String,
}

pub async fn migrate(state: &mut AppState) -> Result<()> {
    migrate_visitor_applications(state).await?;
    migrate_loas(state).await?;
    migrate_certifications(state).await?;
    migrate_solo_certifications(state).await?;
    migrate_sua_blocks(state).await?;
    Ok(())
}

async fn migrate_visitor_applications(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceVisitorApplication>(
        r#"
        select id, user_id, home_facility, why_visit, status, reason_for_denial, submitted_at, decided_at
        from org.visitor_applications
        order by submitted_at asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_target_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let business_key = format!("{user_target_id}:{}", row.submitted_at.to_rfc3339());
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "visitor_application", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "VisitorApplication" where "userId" = $1"#,
            )
            .bind(&user_target_id)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "VisitorApplication" (
                    id, "userId", "homeFacility", "whyVisit", "submittedAt", "decidedAt", "reasonForDenial", status
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8::"VisitorApplicationStatus")
                on conflict (id) do update set
                    "userId" = excluded."userId",
                    "homeFacility" = excluded."homeFacility",
                    "whyVisit" = excluded."whyVisit",
                    "submittedAt" = excluded."submittedAt",
                    "decidedAt" = excluded."decidedAt",
                    "reasonForDenial" = excluded."reasonForDenial",
                    status = excluded.status
                "#,
            )
            .bind(&target_id)
            .bind(&user_target_id)
            .bind(&row.home_facility)
            .bind(&row.why_visit)
            .bind(row.submitted_at)
            .bind(row.decided_at)
            .bind(&row.reason_for_denial)
            .bind(&row.status)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "visitor_application",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
        if existed {
            state.report.domain_mut(DOMAIN).updated += 1;
        } else {
            state.report.domain_mut(DOMAIN).created += 1;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "visitor_applications",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_loas(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceLoa>(
        r#"select id, user_id, start, "end", reason, status from org.loas order by start asc"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_target_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let business_key = format!(
            "{user_target_id}:{}:{}:{}",
            row.start.to_rfc3339(),
            row.end.to_rfc3339(),
            row.reason
        );
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "loa", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "LOA" where "userId" = $1 and start = $2 and "end" = $3 and reason = $4"#,
            )
            .bind(&user_target_id)
            .bind(row.start)
            .bind(row.end)
            .bind(&row.reason)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "LOA" (id, "userId", start, "end", reason, status)
                values ($1, $2, $3, $4, $5, $6::"LOAStatus")
                on conflict (id) do update set
                    "userId" = excluded."userId",
                    start = excluded.start,
                    "end" = excluded."end",
                    reason = excluded.reason,
                    status = excluded.status
                "#,
            )
            .bind(&target_id)
            .bind(&user_target_id)
            .bind(row.start)
            .bind(row.end)
            .bind(&row.reason)
            .bind(&row.status)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "loa",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
        if existed {
            state.report.domain_mut(DOMAIN).updated += 1;
        } else {
            state.report.domain_mut(DOMAIN).created += 1;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, "loas").await?;
    }
    Ok(())
}

async fn migrate_certifications(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceUserCertification>(
        r#"select id, user_id, certification_type_id, certification_option from org.user_certifications"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_target_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let certification_type_target_id = mapped_id(
            &state.target,
            "certification_type",
            &row.certification_type_id,
        )
        .await?;
        let option = normalize_certification_option(&row.certification_option)?.to_string();
        let business_key = format!("{user_target_id}:{certification_type_target_id}");
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "certification", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "Certification" where "userId" = $1 and "certificationTypeId" = $2"#,
            )
            .bind(&user_target_id)
            .bind(&certification_type_target_id)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "Certification" (id, "certificationOption", "certificationTypeId", "userId")
                values ($1, $2::"CertificationOption", $3, $4)
                on conflict (id) do update set
                    "certificationOption" = excluded."certificationOption",
                    "certificationTypeId" = excluded."certificationTypeId",
                    "userId" = excluded."userId"
                "#,
            )
            .bind(&target_id)
            .bind(&option)
            .bind(&certification_type_target_id)
            .bind(&user_target_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "certification",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
        if existed {
            state.report.domain_mut(DOMAIN).updated += 1;
        } else {
            state.report.domain_mut(DOMAIN).created += 1;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "certifications",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_solo_certifications(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceSoloCertification>(
        r#"select id, user_id, certification_type_id, position, expires from org.user_solo_certifications"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_target_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let certification_type_target_id = mapped_id(
            &state.target,
            "certification_type",
            &row.certification_type_id,
        )
        .await?;
        let business_key = format!(
            "{user_target_id}:{certification_type_target_id}:{}:{}",
            row.position,
            row.expires.to_rfc3339()
        );
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "solo_certification", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "SoloCertification" where "userId" = $1 and "certificationTypeId" = $2 and position = $3 and expires = $4"#,
            )
            .bind(&user_target_id)
            .bind(&certification_type_target_id)
            .bind(&row.position)
            .bind(row.expires)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "SoloCertification" (id, expires, position, "userId", "certificationTypeId")
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    expires = excluded.expires,
                    position = excluded.position,
                    "userId" = excluded."userId",
                    "certificationTypeId" = excluded."certificationTypeId"
                "#,
            )
            .bind(&target_id)
            .bind(row.expires)
            .bind(&row.position)
            .bind(&user_target_id)
            .bind(&certification_type_target_id)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "solo_certification",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
        if existed {
            state.report.domain_mut(DOMAIN).updated += 1;
        } else {
            state.report.domain_mut(DOMAIN).created += 1;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(
            &state.target,
            &state.config.run_id,
            DOMAIN,
            "solo_certifications",
        )
        .await?;
    }
    Ok(())
}

async fn migrate_sua_blocks(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceSuaBlock>(
        r#"select id, user_id, start_at, end_at, afiliation, details, mission_number from org.sua_blocks"#,
    )
    .fetch_all(&state.source)
    .await?;
    let airspace_rows = sqlx::query_as::<_, SourceSuaAirspace>(
        r#"select id, sua_block_id, identifier, bottom_altitude, top_altitude from org.sua_block_airspace"#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        let user_target_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let business_key = format!("mission:{}", row.mission_number);
        let mut target_id = if let Some(mapping) =
            target::find_mapping(&state.target, "sua_block", &row.id).await?
        {
            Some(mapping.target_id)
        } else {
            sqlx::query_scalar::<_, String>(
                r#"select id from "SuaBlock" where "missionNumber" = $1"#,
            )
            .bind(&row.mission_number)
            .fetch_optional(&state.target)
            .await?
        };
        let existed = target_id.is_some();
        if target_id.is_none() {
            target_id = Some(new_id());
        }
        let target_id = target_id.expect("target id exists");
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "SuaBlock" (id, "userId", start, "end", afiliation, details, "missionNumber")
                values ($1, $2, $3, $4, $5, $6, $7)
                on conflict (id) do update set
                    "userId" = excluded."userId",
                    start = excluded.start,
                    "end" = excluded."end",
                    afiliation = excluded.afiliation,
                    details = excluded.details,
                    "missionNumber" = excluded."missionNumber"
                "#,
            )
            .bind(&target_id)
            .bind(&user_target_id)
            .bind(row.start_at)
            .bind(row.end_at)
            .bind(&row.afiliation)
            .bind(&row.details)
            .bind(&row.mission_number)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "sua_block",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
    }

    for row in airspace_rows {
        let sua_target_id = mapped_id(&state.target, "sua_block", &row.sua_block_id).await?;
        let business_key = format!("{sua_target_id}:{}", row.identifier);
        let target_id = target::find_mapping(&state.target, "sua_airspace", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(new_id);
        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into "SuaBlockAirspace" (id, "suaBlockId", identifier, "bottomAltitude", "topAltitude")
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    "suaBlockId" = excluded."suaBlockId",
                    identifier = excluded.identifier,
                    "bottomAltitude" = excluded."bottomAltitude",
                    "topAltitude" = excluded."topAltitude"
                "#,
            )
            .bind(&target_id)
            .bind(&sua_target_id)
            .bind(&row.identifier)
            .bind(&row.bottom_altitude)
            .bind(&row.top_altitude)
            .execute(&state.target)
            .await?;
            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "sua_airspace",
                &row.id,
                &business_key,
                &target_id,
                &business_key,
                "updated",
                &row,
            )
            .await?;
        }
    }

    if !state.config.dry_run {
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, "sua_blocks").await?;
    }
    Ok(())
}

async fn mapped_id(pool: &sqlx::PgPool, entity_type: &str, source_id: &str) -> Result<String> {
    Ok(target::find_mapping(pool, entity_type, source_id)
        .await?
        .with_context(|| format!("missing mapping for {entity_type}/{source_id}"))?
        .target_id)
}
