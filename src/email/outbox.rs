use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{email::audience::ResolvedRecipient, errors::ApiError};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingOutboxJob {
    pub id: uuid::Uuid,
    pub template_id: String,
    pub category: String,
    pub is_transactional: bool,
    pub request_source: String,
    pub subject_override: Option<String>,
    pub reply_to_address: Option<String>,
    pub payload: Value,
    pub status: String,
    pub attempt_count: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingRecipient {
    pub id: uuid::Uuid,
    pub user_id: Option<String>,
    pub email: String,
    pub display_name: Option<String>,
    pub source: String,
    pub suppression_reason: Option<String>,
    pub delivery_status: String,
}

pub struct QueueEmailInput {
    pub template_id: String,
    pub category: String,
    pub is_transactional: bool,
    pub requested_by_actor_id: Option<String>,
    pub requested_by_user_id: Option<String>,
    pub requested_by_service_account_id: Option<String>,
    pub request_source: String,
    pub subject_override: Option<String>,
    pub reply_to_address: Option<String>,
    pub payload: Value,
    pub recipient_mode: String,
    pub audience_filter: Option<Value>,
    pub recipients: Vec<ResolvedRecipient>,
    pub suppressed_recipients: Vec<ResolvedRecipient>,
}

pub async fn queue_email<'e>(
    tx: &mut Transaction<'e, Postgres>,
    input: QueueEmailInput,
) -> Result<(uuid::Uuid, DateTime<Utc>), ApiError> {
    let id = uuid::Uuid::new_v4();
    let queued_at = Utc::now();

    sqlx::query(
        r#"
        insert into email.outbox (
            id,
            template_id,
            category,
            is_transactional,
            requested_by_actor_id,
            requested_by_user_id,
            requested_by_service_account_id,
            request_source,
            subject_override,
            reply_to_address,
            payload,
            recipient_mode,
            audience_filter,
            status,
            queued_at,
            next_attempt_at
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'pending', $14, $14)
        "#,
    )
    .bind(id)
    .bind(&input.template_id)
    .bind(&input.category)
    .bind(input.is_transactional)
    .bind(&input.requested_by_actor_id)
    .bind(&input.requested_by_user_id)
    .bind(&input.requested_by_service_account_id)
    .bind(&input.request_source)
    .bind(&input.subject_override)
    .bind(&input.reply_to_address)
    .bind(&input.payload)
    .bind(&input.recipient_mode)
    .bind(&input.audience_filter)
    .bind(queued_at)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    for recipient in input.recipients {
        insert_recipient(tx, id, recipient, "pending", None).await?;
    }

    for recipient in input.suppressed_recipients {
        insert_recipient(
            tx,
            id,
            recipient,
            "suppressed",
            Some("category_unsubscribed".to_string()),
        )
        .await?;
    }

    Ok((id, queued_at))
}

async fn insert_recipient<'e>(
    tx: &mut Transaction<'e, Postgres>,
    outbox_id: uuid::Uuid,
    recipient: ResolvedRecipient,
    delivery_status: &str,
    suppression_reason: Option<String>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into email.outbox_recipients (
            id,
            outbox_id,
            user_id,
            email,
            display_name,
            source,
            suppression_reason,
            delivery_status
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(outbox_id)
    .bind(recipient.user_id)
    .bind(recipient.email)
    .bind(recipient.display_name)
    .bind(recipient.source)
    .bind(suppression_reason)
    .bind(delivery_status)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn claim_pending_jobs(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<PendingOutboxJob>, ApiError> {
    sqlx::query_as::<_, PendingOutboxJob>(
        r#"
        with claimed as (
            select id
            from email.outbox
            where status = 'pending'
              and next_attempt_at <= now()
            order by queued_at asc
            limit $1
            for update skip locked
        )
        update email.outbox o
        set status = 'processing'
        from claimed
        where o.id = claimed.id
        returning
            o.id,
            o.template_id,
            o.category,
            o.is_transactional,
            o.request_source,
            o.subject_override,
            o.reply_to_address,
            o.payload,
            o.status,
            o.attempt_count
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_pending_recipients(
    pool: &PgPool,
    outbox_id: uuid::Uuid,
) -> Result<Vec<PendingRecipient>, ApiError> {
    sqlx::query_as::<_, PendingRecipient>(
        r#"
        select
            id,
            user_id,
            email::text as email,
            display_name,
            source,
            suppression_reason,
            delivery_status
        from email.outbox_recipients
        where outbox_id = $1
          and delivery_status in ('pending', 'processing')
        order by created_at asc
        "#,
    )
    .bind(outbox_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn mark_recipient_processing(
    pool: &PgPool,
    recipient_id: uuid::Uuid,
) -> Result<(), ApiError> {
    sqlx::query("update email.outbox_recipients set delivery_status = 'processing' where id = $1")
        .bind(recipient_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn mark_recipient_sent(
    pool: &PgPool,
    recipient_id: uuid::Uuid,
    provider_message_id: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update email.outbox_recipients
        set delivery_status = 'sent',
            provider_message_id = $2,
            sent_at = now(),
            last_error = null
        where id = $1
        "#,
    )
    .bind(recipient_id)
    .bind(provider_message_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn mark_recipient_failed(
    pool: &PgPool,
    recipient_id: uuid::Uuid,
    error: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        update email.outbox_recipients
        set delivery_status = 'failed',
            failed_at = now(),
            last_error = $2
        where id = $1
        "#,
    )
    .bind(recipient_id)
    .bind(error)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn finalize_outbox_status(
    pool: &PgPool,
    outbox_id: uuid::Uuid,
    provider_message_id: Option<&str>,
) -> Result<(), ApiError> {
    let counts = sqlx::query_as::<_, (i64, i64)>(
        r#"
        select
            count(*) filter (where delivery_status = 'sent') as sent_count,
            count(*) filter (where delivery_status = 'failed') as failed_count
        from email.outbox_recipients
        where outbox_id = $1
        "#,
    )
    .bind(outbox_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    if counts.1 == 0 {
        sqlx::query(
            r#"
            update email.outbox
            set status = case when $2 > 0 then 'sent' else 'suppressed' end,
                sent_at = case when $2 > 0 then now() else null end,
                provider = 'aws_ses_v2',
                provider_message_id = coalesce($3, provider_message_id),
                last_error = null
            where id = $1
            "#,
        )
        .bind(outbox_id)
        .bind(counts.0)
        .bind(provider_message_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
        return Ok(());
    }

    retry_outbox(pool, outbox_id, "one or more recipients failed").await
}

pub async fn retry_outbox(
    pool: &PgPool,
    outbox_id: uuid::Uuid,
    error: &str,
) -> Result<(), ApiError> {
    let attempt_count =
        sqlx::query_scalar::<_, i32>("select attempt_count from email.outbox where id = $1")
            .bind(outbox_id)
            .fetch_one(pool)
            .await
            .map_err(|_| ApiError::Internal)?;

    let next_attempt = next_backoff_time(attempt_count + 1);
    let final_failure = attempt_count + 1 >= 8;

    sqlx::query(
        r#"
        update email.outbox
        set status = $2,
            attempt_count = attempt_count + 1,
            next_attempt_at = $3,
            failed_at = case when $2 = 'failed' then now() else failed_at end,
            last_error = $4
        where id = $1
        "#,
    )
    .bind(outbox_id)
    .bind(if final_failure { "failed" } else { "pending" })
    .bind(next_attempt)
    .bind(error)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    if !final_failure {
        sqlx::query(
            "update email.outbox_recipients set delivery_status = 'pending' where outbox_id = $1 and delivery_status = 'processing'",
        )
        .bind(outbox_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    }

    Ok(())
}

fn next_backoff_time(next_attempt_number: i32) -> DateTime<Utc> {
    let minutes = 2_i64.pow(next_attempt_number.clamp(1, 8) as u32).min(360);
    Utc::now() + Duration::minutes(minutes)
}

pub async fn pending_count(pool: &PgPool) -> Result<i64, ApiError> {
    sqlx::query_scalar::<_, i64>(
        "select count(*)::bigint from email.outbox where status = 'pending'",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}
