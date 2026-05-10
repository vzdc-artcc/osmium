use anyhow::Result;
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::FromRow;

use crate::{
    helpers::{assume_utc, assume_utc_opt},
    mapping::normalize_certification_option,
    state::AppState,
    target,
};

const DOMAIN: &str = "org";

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceVisitorApplication {
    id: String,
    user_id: String,
    home_facility: String,
    why_visit: String,
    status: String,
    reason_for_denial: Option<String>,
    submitted_at: NaiveDateTime,
    decided_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceLoa {
    id: String,
    user_id: String,
    start: NaiveDateTime,
    end: NaiveDateTime,
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
    expires: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct SourceSuaBlock {
    id: String,
    user_id: String,
    start_at: NaiveDateTime,
    end_at: NaiveDateTime,
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
        select
            id,
            "userId" as user_id,
            "homeFacility" as home_facility,
            "whyVisit" as why_visit,
            status::text as status,
            "reasonForDenial" as reason_for_denial,
            "submittedAt" as submitted_at,
            "decidedAt" as decided_at
        from public."VisitorApplication"
        order by "submittedAt" asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let source_business_key = format!("user:{user_id}");
        let target_id = target::find_mapping(&state.target, "visitor_application", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(|| row.id.clone());
        let existed = exists(&state.target, "org.visitor_applications", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into org.visitor_applications (
                    id, user_id, home_facility, why_visit, status, reason_for_denial, submitted_at, decided_at
                )
                values ($1, $2, $3, $4, $5, $6, $7, $8)
                on conflict (id) do update set
                    user_id = excluded.user_id,
                    home_facility = excluded.home_facility,
                    why_visit = excluded.why_visit,
                    status = excluded.status,
                    reason_for_denial = excluded.reason_for_denial,
                    submitted_at = excluded.submitted_at,
                    decided_at = excluded.decided_at
                "#,
            )
            .bind(&target_id)
            .bind(&user_id)
            .bind(&row.home_facility)
            .bind(&row.why_visit)
            .bind(&row.status)
            .bind(&row.reason_for_denial)
            .bind(assume_utc(row.submitted_at))
            .bind(assume_utc_opt(row.decided_at))
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "visitor_application",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    checkpoint(state, "visitor_applications").await
}

async fn migrate_loas(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceLoa>(
        r#"
        select
            id,
            "userId" as user_id,
            start,
            "end" as end,
            reason,
            status::text as status
        from public."LOA"
        order by start asc
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let source_business_key = format!("{user_id}:{}:{}:{}", row.start, row.end, row.reason);
        let target_id = target::find_mapping(&state.target, "loa", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(|| row.id.clone());
        let existed = exists(&state.target, "org.loas", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into org.loas (id, user_id, start, "end", reason, status)
                values ($1, $2, $3, $4, $5, $6)
                on conflict (id) do update set
                    user_id = excluded.user_id,
                    start = excluded.start,
                    "end" = excluded."end",
                    reason = excluded.reason,
                    status = excluded.status
                "#,
            )
            .bind(&target_id)
            .bind(&user_id)
            .bind(assume_utc(row.start))
            .bind(assume_utc(row.end))
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
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    checkpoint(state, "loas").await
}

async fn migrate_certifications(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceUserCertification>(
        r#"
        select
            id,
            "userId" as user_id,
            "certificationTypeId" as certification_type_id,
            "certificationOption"::text as certification_option
        from public."Certification"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let certification_type_id = mapped_id(
            &state.target,
            "certification_type",
            &row.certification_type_id,
        )
        .await?;
        let certification_option = normalize_certification_option(&row.certification_option)?;
        let source_business_key = format!("{user_id}:{certification_type_id}");
        let target_id = target::find_mapping(&state.target, "certification", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(|| row.id.clone());
        let existed = exists(&state.target, "org.user_certifications", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into org.user_certifications (
                    id, user_id, certification_type_id, certification_option
                )
                values ($1, $2, $3, $4)
                on conflict (id) do update set
                    user_id = excluded.user_id,
                    certification_type_id = excluded.certification_type_id,
                    certification_option = excluded.certification_option
                "#,
            )
            .bind(&target_id)
            .bind(&user_id)
            .bind(&certification_type_id)
            .bind(certification_option)
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "certification",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    checkpoint(state, "certifications").await
}

async fn migrate_solo_certifications(state: &mut AppState) -> Result<()> {
    let rows = sqlx::query_as::<_, SourceSoloCertification>(
        r#"
        select
            id,
            "userId" as user_id,
            "certificationTypeId" as certification_type_id,
            position,
            expires
        from public."SoloCertification"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in rows {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let certification_type_id = mapped_id(
            &state.target,
            "certification_type",
            &row.certification_type_id,
        )
        .await?;
        let source_business_key = format!(
            "{user_id}:{certification_type_id}:{}:{}",
            row.position, row.expires
        );
        let target_id = target::find_mapping(&state.target, "solo_certification", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(|| row.id.clone());
        let existed = exists(&state.target, "org.user_solo_certifications", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into org.user_solo_certifications (
                    id, user_id, certification_type_id, position, expires
                )
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    user_id = excluded.user_id,
                    certification_type_id = excluded.certification_type_id,
                    position = excluded.position,
                    expires = excluded.expires
                "#,
            )
            .bind(&target_id)
            .bind(&user_id)
            .bind(&certification_type_id)
            .bind(&row.position)
            .bind(assume_utc(row.expires))
            .execute(&state.target)
            .await?;

            target::upsert_mapping(
                &state.target,
                &state.config.run_id,
                DOMAIN,
                "solo_certification",
                &row.id,
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    checkpoint(state, "solo_certifications").await
}

async fn migrate_sua_blocks(state: &mut AppState) -> Result<()> {
    let blocks = sqlx::query_as::<_, SourceSuaBlock>(
        r#"
        select
            id,
            "userId" as user_id,
            start as start_at,
            "end" as end_at,
            afiliation,
            details,
            "missionNumber" as mission_number
        from public."SuaBlock"
        "#,
    )
    .fetch_all(&state.source)
    .await?;
    let airspace = sqlx::query_as::<_, SourceSuaAirspace>(
        r#"
        select
            id,
            "suaBlockId" as sua_block_id,
            identifier,
            "bottomAltitude" as bottom_altitude,
            "topAltitude" as top_altitude
        from public."SuaBlockAirspace"
        "#,
    )
    .fetch_all(&state.source)
    .await?;

    for row in blocks {
        state.report.domain_mut(DOMAIN).planned += 1;
        let user_id = mapped_id(&state.target, "user", &row.user_id).await?;
        let source_business_key = if row.mission_number.trim().is_empty() {
            format!("{user_id}:{}:{}", row.start_at, row.end_at)
        } else {
            format!("mission:{}", row.mission_number)
        };
        let target_id = target::find_mapping(&state.target, "sua_block", &row.id)
            .await?
            .map(|row| row.target_id)
            .unwrap_or_else(|| row.id.clone());
        let existed = exists(&state.target, "org.sua_blocks", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into org.sua_blocks (
                    id, user_id, start_at, end_at, afiliation, details, mission_number
                )
                values ($1, $2, $3, $4, $5, $6, $7)
                on conflict (id) do update set
                    user_id = excluded.user_id,
                    start_at = excluded.start_at,
                    end_at = excluded.end_at,
                    afiliation = excluded.afiliation,
                    details = excluded.details,
                    mission_number = excluded.mission_number
                "#,
            )
            .bind(&target_id)
            .bind(&user_id)
            .bind(assume_utc(row.start_at))
            .bind(assume_utc(row.end_at))
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
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }

        bump_counts(state, existed);
    }

    for row in airspace {
        let sua_block_id = mapped_id(&state.target, "sua_block", &row.sua_block_id).await?;
        let source_business_key = format!(
            "{sua_block_id}:{}:{}:{}",
            row.identifier, row.bottom_altitude, row.top_altitude
        );
        let target_id = row.id.clone();
        let existed = exists(&state.target, "org.sua_block_airspace", &target_id).await?;

        if !state.config.dry_run {
            sqlx::query(
                r#"
                insert into org.sua_block_airspace (
                    id, sua_block_id, identifier, bottom_altitude, top_altitude
                )
                values ($1, $2, $3, $4, $5)
                on conflict (id) do update set
                    sua_block_id = excluded.sua_block_id,
                    identifier = excluded.identifier,
                    bottom_altitude = excluded.bottom_altitude,
                    top_altitude = excluded.top_altitude
                "#,
            )
            .bind(&target_id)
            .bind(&sua_block_id)
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
                &source_business_key,
                &target_id,
                &source_business_key,
                if existed { "updated" } else { "created" },
                &row,
            )
            .await?;
        }
    }

    checkpoint(state, "sua_blocks").await
}

async fn mapped_id(pool: &sqlx::PgPool, entity_type: &str, source_id: &str) -> Result<String> {
    Ok(target::find_mapping(pool, entity_type, source_id)
        .await?
        .map(|row| row.target_id)
        .unwrap_or_else(|| source_id.to_string()))
}

async fn exists(pool: &sqlx::PgPool, table: &str, id: &str) -> Result<bool> {
    let query = format!("select exists(select 1 from {table} where id = $1)");
    Ok(sqlx::query_scalar::<_, bool>(&query)
        .bind(id)
        .fetch_one(pool)
        .await?)
}

fn bump_counts(state: &mut AppState, existed: bool) {
    let domain = state.report.domain_mut(DOMAIN);
    if existed {
        domain.updated += 1;
    } else {
        domain.created += 1;
    }
}

async fn checkpoint(state: &AppState, entity_type: &str) -> Result<()> {
    if !state.config.dry_run {
        target::checkpoint(&state.target, &state.config.run_id, DOMAIN, entity_type).await?;
    }
    Ok(())
}
