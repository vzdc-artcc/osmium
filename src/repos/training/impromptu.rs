use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    errors::ApiError,
    models::{ImpromptuClaimItem, ImpromptuOfferItem},
};

#[derive(Debug, sqlx::FromRow)]
struct OfferRow {
    id: String,
    created_by_user_id: String,
    created_by_name: Option<String>,
    session_types: Vec<String>,
    available_at: Option<DateTime<Utc>>,
    notes: Option<String>,
    status: String,
    accepted_user_id: Option<String>,
    claim_count: i64,
    created_at: DateTime<Utc>,
}

impl From<OfferRow> for ImpromptuOfferItem {
    fn from(row: OfferRow) -> Self {
        ImpromptuOfferItem {
            id: row.id,
            created_by_user_id: row.created_by_user_id,
            created_by_name: row.created_by_name.unwrap_or_default(),
            session_types: row.session_types,
            available_at: row.available_at,
            notes: row.notes,
            status: row.status,
            accepted_user_id: row.accepted_user_id,
            claim_count: row.claim_count,
            created_at: row.created_at,
        }
    }
}

const OFFER_SELECT: &str = r#"
    select o.id, o.created_by_user_id, u.display_name as created_by_name, o.session_types,
           o.available_at, o.notes, o.status, o.accepted_user_id,
           (select count(*) from training.impromptu_claims c where c.offer_id = o.id)::bigint as claim_count,
           o.created_at
    from training.impromptu_offers o
    left join identity.users u on u.id = o.created_by_user_id
"#;

pub async fn create_offer(
    pool: &PgPool,
    created_by_user_id: &str,
    session_types: &[String],
    available_at: Option<DateTime<Utc>>,
    notes: Option<&str>,
) -> Result<String, ApiError> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        insert into training.impromptu_offers
            (id, created_by_user_id, session_types, available_at, notes, status)
        values ($1, $2, $3, $4, $5, 'open')
        "#,
    )
    .bind(&id)
    .bind(created_by_user_id)
    .bind(session_types)
    .bind(available_at)
    .bind(notes)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(id)
}

pub async fn set_offer_discord_message(
    pool: &PgPool,
    offer_id: &str,
    channel_id: &str,
    message_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "update training.impromptu_offers set discord_channel_id = $2, discord_message_id = $3, updated_at = now() where id = $1",
    )
    .bind(offer_id)
    .bind(channel_id)
    .bind(message_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn delete_offer(pool: &PgPool, offer_id: &str) -> Result<(), ApiError> {
    sqlx::query("delete from training.impromptu_offers where id = $1")
        .bind(offer_id)
        .execute(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(())
}

pub async fn list_offers_for_creator(
    pool: &PgPool,
    created_by_user_id: &str,
) -> Result<Vec<ImpromptuOfferItem>, ApiError> {
    let rows = sqlx::query_as::<_, OfferRow>(&format!(
        "{OFFER_SELECT} where o.created_by_user_id = $1 order by o.created_at desc limit 100"
    ))
    .bind(created_by_user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get_offer(
    pool: &PgPool,
    offer_id: &str,
) -> Result<Option<ImpromptuOfferItem>, ApiError> {
    let row = sqlx::query_as::<_, OfferRow>(&format!("{OFFER_SELECT} where o.id = $1"))
        .bind(offer_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(row.map(Into::into))
}

/// The stored Discord channel/message for an offer's posted embed, if any.
pub async fn get_offer_discord_message(
    pool: &PgPool,
    offer_id: &str,
) -> Result<Option<(String, String)>, ApiError> {
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "select discord_channel_id, discord_message_id from training.impromptu_offers where id = $1",
    )
    .bind(offer_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(row.and_then(|(channel, message)| Some((channel?, message?))))
}

#[derive(Debug, sqlx::FromRow)]
struct ClaimRow {
    id: String,
    user_id: String,
    cid: i64,
    name: Option<String>,
    rating: Option<String>,
    controller_status: Option<String>,
    status: String,
    session_count: i64,
    last_session_at: Option<DateTime<Utc>>,
    claimed_at: DateTime<Utc>,
}

pub async fn get_offer_claims(
    pool: &PgPool,
    offer_id: &str,
) -> Result<Vec<ImpromptuClaimItem>, ApiError> {
    let rows = sqlx::query_as::<_, ClaimRow>(
        r#"
        select c.id, c.user_id, u.cid, u.display_name as name,
               m.rating, m.controller_status, c.status,
               (select count(*) from training.training_sessions s where s.student_id = c.user_id)::bigint as session_count,
               (select max(s.start) from training.training_sessions s where s.student_id = c.user_id) as last_session_at,
               c.claimed_at
        from training.impromptu_claims c
        join identity.users u on u.id = c.user_id
        left join org.memberships m on m.user_id = c.user_id
        where c.offer_id = $1
        order by c.claimed_at asc
        "#,
    )
    .bind(offer_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(rows
        .into_iter()
        .map(|row| ImpromptuClaimItem {
            id: row.id,
            user_id: row.user_id,
            cid: row.cid,
            name: row.name.unwrap_or_default(),
            rating: row.rating,
            controller_status: row.controller_status,
            status: row.status,
            session_count: row.session_count,
            last_session_at: row.last_session_at,
            claimed_at: row.claimed_at,
        })
        .collect())
}

/// Record a claim for the user linked to `discord_id`. Returns `Ok(None)` when
/// the Discord id isn't linked to a user; `Err(Conflict)` when the offer isn't
/// open. Idempotent for repeat claims (unique offer_id+user_id).
pub async fn record_claim_by_discord_id(
    pool: &PgPool,
    offer_id: &str,
    discord_id: &str,
) -> Result<Option<()>, ApiError> {
    let user_id: Option<String> = sqlx::query_scalar(
        "select local_id from integration.external_sync_mappings where system_code = 'discord' and entity_type = 'user_identity' and external_id = $1",
    )
    .bind(discord_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    let Some(user_id) = user_id else {
        return Ok(None);
    };

    // Lock the offer row so a concurrent accept can't slip in between the
    // open-check and the insert; the accept's UPDATE takes the same row lock.
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    let status: Option<String> =
        sqlx::query_scalar("select status from training.impromptu_offers where id = $1 for update")
            .bind(offer_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;
    match status.as_deref() {
        Some("open") => {}
        Some(_) => return Err(ApiError::Conflict),
        None => return Err(ApiError::NotFound),
    }

    sqlx::query(
        r#"
        insert into training.impromptu_claims (id, offer_id, user_id, status)
        values ($1, $2, $3, 'pending')
        on conflict (offer_id, user_id) do nothing
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(offer_id)
    .bind(&user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(Some(()))
}

/// Discord id + display name for a claimant, resolved via the discord link.
#[derive(Debug, sqlx::FromRow)]
pub struct ClaimantContact {
    pub user_id: String,
    pub name: Option<String>,
    pub discord_id: Option<String>,
}

/// Accept `user_id` for the offer: mark the offer accepted, that claim accepted,
/// all others rejected. Returns (accepted_contact, rejected_contacts) for DMs.
pub async fn accept_offer(
    pool: &PgPool,
    offer_id: &str,
    user_id: &str,
) -> Result<(Option<ClaimantContact>, Vec<ClaimantContact>), ApiError> {
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    // Only a user who actually claimed can be accepted; otherwise this would
    // close the offer and reject every real claimant while accepting nobody.
    let has_claim: Option<i32> = sqlx::query_scalar(
        "select 1 from training.impromptu_claims where offer_id = $1 and user_id = $2",
    )
    .bind(offer_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    if has_claim.is_none() {
        return Err(ApiError::BadRequest);
    }

    let updated = sqlx::query(
        "update training.impromptu_offers set status = 'accepted', accepted_user_id = $2, updated_at = now() where id = $1 and status = 'open'",
    )
    .bind(offer_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::Conflict);
    }

    sqlx::query(
        "update training.impromptu_claims set status = case when user_id = $2 then 'accepted' else 'rejected' end where offer_id = $1",
    )
    .bind(offer_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let contacts = sqlx::query_as::<_, ClaimantContact>(
        r#"
        select c.user_id, u.display_name as name, dl.external_id as discord_id
        from training.impromptu_claims c
        join identity.users u on u.id = c.user_id
        left join integration.external_sync_mappings dl
            on dl.system_code = 'discord' and dl.entity_type = 'user_identity' and dl.local_id = c.user_id
        where c.offer_id = $1
        "#,
    )
    .bind(offer_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    let mut accepted = None;
    let mut rejected = Vec::new();
    for contact in contacts {
        if contact.user_id == user_id {
            accepted = Some(contact);
        } else {
            rejected.push(contact);
        }
    }
    Ok((accepted, rejected))
}

pub async fn cancel_offer(pool: &PgPool, offer_id: &str) -> Result<(), ApiError> {
    sqlx::query(
        "update training.impromptu_offers set status = 'cancelled', updated_at = now() where id = $1 and status = 'open'",
    )
    .bind(offer_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}
