use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use chrono::{DateTime, Utc};

use crate::{
    errors::ApiError,
    models::{
        CertificationItem, CertificationTypeItem, RosterCertOption, RosterCertificationItem,
        RosterSolo,
    },
};

/// Bulk cert/solo/LOA summary for every rostered controller — one row per cid
/// with their granted (non-NONE) certifications, active-solo type ids, and
/// approved-LOA flag. Avoids the roster table making an N+1 per-controller call.
pub async fn list_roster_certifications(
    pool: &PgPool,
) -> Result<Vec<RosterCertificationItem>, ApiError> {
    let cert_rows = sqlx::query_as::<_, (i64, String, String)>(
        r#"
        select u.cid, uc.certification_type_id, uc.certification_option
        from org.user_certifications uc
        join identity.users u on u.id = uc.user_id
        where uc.certification_option <> 'NONE'
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let solo_rows = sqlx::query_as::<_, (i64, String, String, DateTime<Utc>)>(
        r#"
        select u.cid, s.certification_type_id, s.position, s.expires
        from org.user_solo_certifications s
        join identity.users u on u.id = s.user_id
        where s.expires > now()
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let loa_rows = sqlx::query_as::<_, (i64,)>(
        r#"
        select distinct u.cid
        from org.loas l
        join identity.users u on u.id = l.user_id
        where l.status = 'APPROVED'
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    let mut by_cid: HashMap<i64, RosterCertificationItem> = HashMap::new();
    let entry = |map: &mut HashMap<i64, RosterCertificationItem>, cid: i64| {
        map.entry(cid).or_insert_with(|| RosterCertificationItem {
            cid,
            certifications: Vec::new(),
            solos: Vec::new(),
            has_approved_loa: false,
        });
    };

    for (cid, type_id, option) in cert_rows {
        entry(&mut by_cid, cid);
        by_cid.get_mut(&cid).unwrap().certifications.push(RosterCertOption {
            certification_type_id: type_id,
            certification_option: option,
        });
    }
    for (cid, type_id, position, expires) in solo_rows {
        entry(&mut by_cid, cid);
        by_cid.get_mut(&cid).unwrap().solos.push(RosterSolo {
            certification_type_id: type_id,
            position,
            expires,
        });
    }
    for (cid,) in loa_rows {
        entry(&mut by_cid, cid);
        by_cid.get_mut(&cid).unwrap().has_approved_loa = true;
    }

    Ok(by_cid.into_values().collect())
}

pub async fn fetch_user_certifications(
    pool: &PgPool,
    cid: i64,
) -> Result<Vec<CertificationItem>, ApiError> {
    sqlx::query_as::<_, CertificationItem>(
        r#"
        select
            ct.id as certification_type_id,
            ct.name as certification_type_name,
            ct.sort_order,
            coalesce(uc.certification_option, 'NONE') as certification_option
        from org.certification_types ct
        join identity.users u on u.cid = $1
        left join org.user_certifications uc
            on uc.certification_type_id = ct.id and uc.user_id = u.id
        order by ct.sort_order asc, ct.name asc
        "#,
    )
    .bind(cid)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, sqlx::FromRow)]
struct CertificationTypeRow {
    id: String,
    name: String,
    sort_order: i32,
    can_solo_cert: bool,
    auto_assign_unrestricted: bool,
    certification_options: Vec<String>,
}

impl From<CertificationTypeRow> for CertificationTypeItem {
    fn from(row: CertificationTypeRow) -> Self {
        CertificationTypeItem {
            id: row.id,
            name: row.name,
            sort_order: row.sort_order,
            can_solo_cert: row.can_solo_cert,
            auto_assign_unrestricted: row.auto_assign_unrestricted,
            certification_options: row.certification_options,
        }
    }
}

pub async fn list_certification_types(
    pool: &PgPool,
) -> Result<Vec<CertificationTypeItem>, ApiError> {
    let rows = sqlx::query_as::<_, CertificationTypeRow>(
        r#"
        select
            ct.id,
            ct.name,
            ct.sort_order,
            ct.can_solo_cert,
            ct.auto_assign_unrestricted,
            coalesce(
                array_agg(o.option_key order by o.option_key)
                    filter (where o.option_key is not null),
                '{}'
            ) as certification_options
        from org.certification_types ct
        left join org.certification_type_allowed_options o
            on o.certification_type_id = ct.id
        group by ct.id
        order by ct.sort_order asc, ct.name asc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn fetch_certification_type(
    pool: &PgPool,
    id: &str,
) -> Result<Option<CertificationTypeItem>, ApiError> {
    let row = sqlx::query_as::<_, CertificationTypeRow>(
        r#"
        select
            ct.id,
            ct.name,
            ct.sort_order,
            ct.can_solo_cert,
            ct.auto_assign_unrestricted,
            coalesce(
                array_agg(o.option_key order by o.option_key)
                    filter (where o.option_key is not null),
                '{}'
            ) as certification_options
        from org.certification_types ct
        left join org.certification_type_allowed_options o
            on o.certification_type_id = ct.id
        where ct.id = $1
        group by ct.id
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(row.map(Into::into))
}

/// Lesson identifiers of any `lesson_roster_changes` for this certification type
/// whose granted option would no longer be allowed under `allowed_options`.
/// Mirrors the website's `createOrUpdateCertificationType` guard — removing an
/// option that a lesson still grants would orphan that lesson's roster change.
pub async fn conflicting_lesson_identifiers(
    pool: &PgPool,
    certification_type_id: &str,
    allowed_options: &[String],
) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        select l.identifier
        from training.lesson_roster_changes lrc
        join training.lessons l on l.id = lrc.lesson_id
        where lrc.certification_type_id = $1
          and lrc.certification_option <> all($2)
        order by l.identifier asc
        "#,
    )
    .bind(certification_type_id)
    .bind(allowed_options)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Creates (when `id` is `None`) or updates a certification type and replaces
/// its allowed-option set. Returns the type's id and name.
pub async fn upsert_certification_type(
    pool: &PgPool,
    id: Option<&str>,
    name: &str,
    can_solo_cert: bool,
    auto_assign_unrestricted: bool,
    options: &[String],
) -> Result<(String, String), ApiError> {
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    let (type_id, type_name): (String, String) = if let Some(existing_id) = id {
        sqlx::query_as::<_, (String, String)>(
            r#"
            update org.certification_types
            set name = $2,
                can_solo_cert = $3,
                auto_assign_unrestricted = $4
            where id = $1
            returning id, name
            "#,
        )
        .bind(existing_id)
        .bind(name)
        .bind(can_solo_cert)
        .bind(auto_assign_unrestricted)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?
        .ok_or(ApiError::NotFound)?
    } else {
        sqlx::query_as::<_, (String, String)>(
            r#"
            insert into org.certification_types
                (id, name, sort_order, can_solo_cert, auto_assign_unrestricted)
            values ($1, $2, 0, $3, $4)
            returning id, name
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(name)
        .bind(can_solo_cert)
        .bind(auto_assign_unrestricted)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| ApiError::Conflict)?
    };

    sqlx::query("delete from org.certification_type_allowed_options where certification_type_id = $1")
        .bind(&type_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;

    for option in options {
        sqlx::query(
            r#"
            insert into org.certification_type_allowed_options (certification_type_id, option_key)
            values ($1, $2)
            on conflict (certification_type_id, option_key) do nothing
            "#,
        )
        .bind(&type_id)
        .bind(option)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::BadRequest)?;
    }

    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok((type_id, type_name))
}

pub async fn update_certification_type_order(
    pool: &PgPool,
    items: &[(String, i32)],
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    for (id, order) in items {
        sqlx::query("update org.certification_types set sort_order = $2 where id = $1")
            .bind(id)
            .bind(order)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::Internal)?;
    }
    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(())
}

/// Deletes a certification type, returning its name (for audit logging), or
/// `None` if it did not exist.
pub async fn delete_certification_type(
    pool: &PgPool,
    id: &str,
) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "delete from org.certification_types where id = $1 returning name",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn certification_type_exists(pool: &PgPool, id: &str) -> Result<bool, ApiError> {
    sqlx::query_scalar::<_, bool>(
        "select exists(select 1 from org.certification_types where id = $1)",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Bulk-writes a controller's certification grid: upserts one option per
/// certification type on the natural key `(user_id, certification_type_id)`.
pub async fn save_user_certifications(
    pool: &PgPool,
    user_id: &str,
    entries: &[(String, String)],
    actor_id: Option<&str>,
) -> Result<(), ApiError> {
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;
    for (certification_type_id, certification_option) in entries {
        sqlx::query(
            r#"
            insert into org.user_certifications
                (id, user_id, certification_type_id, certification_option, granted_by_actor_id)
            values ($1, $2, $3, $4, $5)
            on conflict (user_id, certification_type_id) do update
            set certification_option = excluded.certification_option,
                granted_at = now(),
                granted_by_actor_id = excluded.granted_by_actor_id
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(user_id)
        .bind(certification_type_id)
        .bind(certification_option)
        .bind(actor_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::BadRequest)?;
    }
    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(())
}

/// Certification type ids flagged `auto_assign_unrestricted` (roster-sync auto grant).
pub async fn list_auto_assign_unrestricted_type_ids(
    pool: &PgPool,
) -> Result<Vec<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        "select id from org.certification_types where auto_assign_unrestricted",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// The controller's current option for one type, or `None` if unset.
pub async fn fetch_user_cert_option(
    pool: &PgPool,
    user_id: &str,
    certification_type_id: &str,
) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar::<_, String>(
        r#"
        select certification_option
        from org.user_certifications
        where user_id = $1 and certification_type_id = $2
        "#,
    )
    .bind(user_id)
    .bind(certification_type_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

/// Upserts a single user certification (roster-sync auto-UNRESTRICTED grant).
pub async fn upsert_user_certification(
    pool: &PgPool,
    user_id: &str,
    certification_type_id: &str,
    certification_option: &str,
    actor_id: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into org.user_certifications
            (id, user_id, certification_type_id, certification_option, granted_by_actor_id)
        values ($1, $2, $3, $4, $5)
        on conflict (user_id, certification_type_id) do update
        set certification_option = excluded.certification_option,
            granted_at = now(),
            granted_by_actor_id = excluded.granted_by_actor_id
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(certification_type_id)
    .bind(certification_option)
    .bind(actor_id)
    .execute(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
    Ok(())
}
