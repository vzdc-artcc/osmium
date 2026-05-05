use anyhow::Result;
use serde::Serialize;
use sqlx::{PgPool, Row};

use crate::mapping::checksum;

#[derive(Debug, Clone)]
pub struct EntityMapRow {
    pub target_id: String,
}

pub async fn ensure_schema(pool: &PgPool) -> Result<()> {
    sqlx::query(
        r#"
        create schema if not exists migrator;

        create table if not exists migrator.migration_runs (
            run_id text primary key,
            status text not null,
            dry_run boolean not null default false,
            started_at timestamptz not null default now(),
            finished_at timestamptz
        );

        create table if not exists migrator.migration_entity_map (
            run_id text not null references migrator.migration_runs(run_id) on delete cascade,
            domain text not null,
            entity_type text not null,
            source_id text not null,
            source_business_key text not null,
            target_id text not null,
            target_business_key text not null,
            status text not null,
            checksum text not null,
            created_at timestamptz not null default now(),
            updated_at timestamptz not null default now(),
            unique (entity_type, source_id)
        );

        create index if not exists idx_migration_entity_map_target
            on migrator.migration_entity_map(entity_type, target_id);

        create table if not exists migrator.migration_warnings (
            id text primary key default gen_random_uuid()::text,
            run_id text not null references migrator.migration_runs(run_id) on delete cascade,
            domain text not null,
            entity_type text not null,
            source_id text not null,
            message text not null,
            created_at timestamptz not null default now()
        );

        create table if not exists migrator.migration_checkpoints (
            run_id text not null references migrator.migration_runs(run_id) on delete cascade,
            domain text not null,
            entity_type text not null,
            updated_at timestamptz not null default now(),
            primary key (run_id, domain, entity_type)
        );
        "#,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn start_run(pool: &PgPool, run_id: &str, dry_run: bool) -> Result<()> {
    sqlx::query(
        r#"
        insert into migrator.migration_runs (run_id, status, dry_run)
        values ($1, 'running', $2)
        on conflict (run_id)
        do update set status = 'running', dry_run = excluded.dry_run, finished_at = null
        "#,
    )
    .bind(run_id)
    .bind(dry_run)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finish_run(pool: &PgPool, run_id: &str, status: &str) -> Result<()> {
    sqlx::query(
        r#"update migrator.migration_runs set status = $2, finished_at = now() where run_id = $1"#,
    )
    .bind(run_id)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn reset_run(pool: &PgPool, run_id: &str) -> Result<()> {
    sqlx::query("delete from migrator.migration_runs where run_id = $1")
        .bind(run_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find_mapping(
    pool: &PgPool,
    entity_type: &str,
    source_id: &str,
) -> Result<Option<EntityMapRow>> {
    let row = sqlx::query(
        r#"select target_id from migrator.migration_entity_map where entity_type = $1 and source_id = $2"#,
    )
    .bind(entity_type)
    .bind(source_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| EntityMapRow {
        target_id: row.get("target_id"),
    }))
}

pub async fn upsert_mapping(
    pool: &PgPool,
    run_id: &str,
    domain: &str,
    entity_type: &str,
    source_id: &str,
    source_business_key: &str,
    target_id: &str,
    target_business_key: &str,
    status: &str,
    payload: &impl Serialize,
) -> Result<()> {
    let payload_json = serde_json::to_vec(payload)?;
    sqlx::query(
        r#"
        insert into migrator.migration_entity_map
            (run_id, domain, entity_type, source_id, source_business_key, target_id, target_business_key, status, checksum)
        values
            ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        on conflict (entity_type, source_id)
        do update set
            run_id = excluded.run_id,
            domain = excluded.domain,
            source_business_key = excluded.source_business_key,
            target_id = excluded.target_id,
            target_business_key = excluded.target_business_key,
            status = excluded.status,
            checksum = excluded.checksum,
            updated_at = now()
        "#,
    )
    .bind(run_id)
    .bind(domain)
    .bind(entity_type)
    .bind(source_id)
    .bind(source_business_key)
    .bind(target_id)
    .bind(target_business_key)
    .bind(status)
    .bind(checksum(payload_json))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_warning(
    pool: &PgPool,
    run_id: &str,
    domain: &str,
    entity_type: &str,
    source_id: &str,
    message: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        insert into migrator.migration_warnings (run_id, domain, entity_type, source_id, message)
        values ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(run_id)
    .bind(domain)
    .bind(entity_type)
    .bind(source_id)
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn checkpoint(
    pool: &PgPool,
    run_id: &str,
    domain: &str,
    entity_type: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        insert into migrator.migration_checkpoints (run_id, domain, entity_type)
        values ($1, $2, $3)
        on conflict (run_id, domain, entity_type)
        do update set updated_at = now()
        "#,
    )
    .bind(run_id)
    .bind(domain)
    .bind(entity_type)
    .execute(pool)
    .await?;
    Ok(())
}
