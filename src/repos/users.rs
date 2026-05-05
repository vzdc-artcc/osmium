use std::collections::HashSet;

use sqlx::{Error as SqlxError, PgPool, Postgres, Transaction};

use crate::{
    errors::ApiError,
    models::users::{
        AdminUserListItem, MeProfileBody, RosterUserRow, TeamSpeakUidBody, UserStats,
        VisitorApplicationItem,
    },
};

const DEFAULT_TIMEZONE: &str = "America/New_York";

pub struct SelfProfileUpdate {
    pub preferred_name: Option<String>,
    pub bio: Option<String>,
    pub timezone: String,
    pub receive_event_notifications: bool,
}

#[derive(sqlx::FromRow)]
pub struct LoginUserRow {
    pub id: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct LoginMembershipRow {
    pub rating: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ExistingTeamSpeakUidRow {
    user_id: String,
    id: String,
    uid: String,
    linked_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_admin_users(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<AdminUserListItem>, ApiError> {
    sqlx::query_as::<_, AdminUserListItem>(
        r#"
        select
            id,
            cid,
            coalesce(email::text, '') as email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status
        from org.v_user_roster_profile
        order by cid asc
        limit $1 offset $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_admin_user_by_cid(
    pool: &PgPool,
    cid: i64,
) -> Result<Option<AdminUserListItem>, ApiError> {
    sqlx::query_as::<_, AdminUserListItem>(
        r#"
        select
            id,
            cid,
            coalesce(email::text, '') as email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status
        from org.v_user_roster_profile
        where cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_roster_users(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<RosterUserRow>, ApiError> {
    sqlx::query_as::<_, RosterUserRow>(
        r#"
        select
            id,
            cid,
            coalesce(email::text, '') as email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status,
            controller_status,
            membership_status,
            join_date,
            home_facility,
            visitor_home_facility,
            is_active
        from org.v_user_roster_profile
        order by cid asc
        limit $1 offset $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_roster_user_by_cid(
    pool: &PgPool,
    cid: i64,
) -> Result<Option<RosterUserRow>, ApiError> {
    sqlx::query_as::<_, RosterUserRow>(
        r#"
        select
            id,
            cid,
            coalesce(email::text, '') as email,
            display_name,
            role,
            first_name,
            last_name,
            artcc,
            rating,
            division,
            status,
            controller_status,
            membership_status,
            join_date,
            home_facility,
            visitor_home_facility,
            is_active
        from org.v_user_roster_profile
        where cid = $1
        "#,
    )
    .bind(cid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn update_controller_status(
    pool: &PgPool,
    cid: i64,
    controller_status: &str,
    artcc: Option<&str>,
) -> Result<Option<(i64, String, Option<String>)>, ApiError> {
    sqlx::query_as::<_, (i64, String, Option<String>)>(
        r#"
        update org.memberships
        set controller_status = $2,
            artcc = coalesce($3, artcc),
            updated_at = now()
        where user_id = (select id from identity.users where cid = $1)
        returning
            (select cid from identity.users where id = org.memberships.user_id) as cid,
            controller_status,
            artcc
        "#,
    )
    .bind(cid)
    .bind(controller_status)
    .bind(artcc)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_user_stats(pool: &PgPool, user_id: &str) -> Result<UserStats, ApiError> {
    sqlx::query_as::<_, UserStats>(
        r#"
        select
            (select count(*)::bigint from identity.sessions s where s.user_id = $1 and s.revoked_at is null and s.expires_at > now()) as active_sessions,
            (select count(*)::bigint from events.event_positions ep where ep.user_id = $1) as assigned_event_positions,
            (select count(*)::bigint from training.training_assignments ta where ta.student_id = $1) as training_assignments_as_student,
            (select count(*)::bigint from training.training_assignments ta where ta.primary_trainer_id = $1) as training_assignments_as_primary_trainer,
            (select count(*)::bigint from training.training_assignment_other_trainers taot where taot.trainer_id = $1) as training_assignments_as_other_trainer,
            (select count(*)::bigint from training.training_assignment_requests tar where tar.student_id = $1) as training_assignment_requests,
            (select count(*)::bigint from training.training_assignment_request_interested_trainers tarit where tarit.trainer_id = $1) as training_assignment_interests,
            (select count(*)::bigint from training.trainer_release_requests trr where trr.student_id = $1) as trainer_release_requests
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_me_profile(pool: &PgPool, user_id: &str) -> Result<MeProfileBody, ApiError> {
    sqlx::query_as::<_, MeProfileBody>(
        r#"
        select
            u.first_name,
            u.last_name,
            u.preferred_name,
            p.bio,
            coalesce(p.timezone, $2) as timezone,
            coalesce(p.new_event_notifications, false) as receive_event_notifications,
            m.operating_initials
        from identity.users u
        left join identity.user_profiles p on p.user_id = u.id
        left join org.memberships m on m.user_id = u.id
        where u.id = $1
        "#,
    )
    .bind(user_id)
    .bind(DEFAULT_TIMEZONE)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn update_me_profile(
    pool: &PgPool,
    user_id: &str,
    update: &SelfProfileUpdate,
) -> Result<MeProfileBody, ApiError> {
    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        update identity.users
        set preferred_name = $2,
            updated_at = now()
        where id = $1
        "#,
    )
    .bind(user_id)
    .bind(&update.preferred_name)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into identity.user_profiles (user_id, bio, timezone, new_event_notifications)
        values ($1, $2, $3, $4)
        on conflict (user_id) do update
        set bio = excluded.bio,
            timezone = excluded.timezone,
            new_event_notifications = excluded.new_event_notifications,
            updated_at = now()
        "#,
    )
    .bind(user_id)
    .bind(&update.bio)
    .bind(&update.timezone)
    .bind(update.receive_event_notifications)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let profile = sqlx::query_as::<_, MeProfileBody>(
        r#"
        select
            u.first_name,
            u.last_name,
            u.preferred_name,
            p.bio,
            p.timezone,
            p.new_event_notifications as receive_event_notifications,
            m.operating_initials
        from identity.users u
        join identity.user_profiles p on p.user_id = u.id
        left join org.memberships m on m.user_id = u.id
        where u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(profile)
}

pub async fn list_teamspeak_uids(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<TeamSpeakUidBody>, ApiError> {
    sqlx::query_as::<_, TeamSpeakUidBody>(
        r#"
        select
            id,
            provider_subject as uid,
            linked_at
        from identity.user_identities
        where user_id = $1
          and provider = 'TEAMSPEAK'
        order by linked_at asc, id asc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn create_teamspeak_uid(
    pool: &PgPool,
    user_id: &str,
    uid: &str,
) -> Result<TeamSpeakUidBody, ApiError> {
    let inserted = sqlx::query_as::<_, TeamSpeakUidBody>(
        r#"
        insert into identity.user_identities (
            id,
            user_id,
            provider,
            provider_subject,
            metadata
        )
        values (gen_random_uuid()::text, $1, 'TEAMSPEAK', $2, '{}'::jsonb)
        on conflict (provider, provider_subject) do nothing
        returning
            id,
            provider_subject as uid,
            linked_at
        "#,
    )
    .bind(user_id)
    .bind(uid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    if let Some(row) = inserted {
        return Ok(row);
    }

    let existing = sqlx::query_as::<_, ExistingTeamSpeakUidRow>(
        r#"
        select
            user_id,
            id,
            provider_subject as uid,
            linked_at
        from identity.user_identities
        where provider = 'TEAMSPEAK'
          and provider_subject = $1
        "#,
    )
    .bind(uid)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    match existing {
        Some(row) if row.user_id == user_id => Ok(TeamSpeakUidBody {
            id: row.id,
            uid: row.uid,
            linked_at: row.linked_at,
        }),
        Some(_) => Err(ApiError::BadRequest),
        None => Err(ApiError::Internal),
    }
}

pub async fn delete_teamspeak_uid(
    pool: &PgPool,
    user_id: &str,
    identity_id: &str,
) -> Result<(), ApiError> {
    let deleted = sqlx::query_scalar::<_, String>(
        r#"
        delete from identity.user_identities
        where id = $1
          and user_id = $2
          and provider = 'TEAMSPEAK'
        returning id
        "#,
    )
    .bind(identity_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)?;

    if deleted.is_some() {
        Ok(())
    } else {
        Err(ApiError::BadRequest)
    }
}

pub async fn ensure_user_profile(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into identity.user_profiles (user_id)
        values ($1)
        on conflict (user_id) do nothing
        "#,
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn upsert_login_user(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    cid: i64,
    email: &str,
    full_name: &str,
    display_name: &str,
) -> Result<LoginUserRow, ApiError> {
    sqlx::query_as::<_, LoginUserRow>(
        r#"
        insert into identity.users (id, cid, email, full_name, display_name)
        values ($1, $2, $3, $4, $5)
        on conflict (cid) do update
        set email = excluded.email,
            full_name = excluded.full_name,
            display_name = excluded.display_name,
            updated_at = now()
        returning id, first_name, last_name
        "#,
    )
    .bind(user_id)
    .bind(cid)
    .bind(email)
    .bind(full_name)
    .bind(display_name)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn upsert_login_membership(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    rating: Option<&str>,
) -> Result<LoginMembershipRow, ApiError> {
    sqlx::query_as::<_, LoginMembershipRow>(
        r#"
        insert into org.memberships (
            user_id,
            artcc,
            division,
            rating,
            membership_status,
            controller_status,
            updated_at
        )
        values ($1, 'ZDC', 'USA', $2, 'ACTIVE', 'NONE', now())
        on conflict (user_id) do update
        set rating = coalesce(excluded.rating, org.memberships.rating),
            membership_status = 'ACTIVE',
            updated_at = now()
        returning rating
        "#,
    )
    .bind(user_id)
    .bind(rating)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn ensure_operating_initials(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    first_name: Option<&str>,
    last_name: Option<&str>,
    display_name: &str,
) -> Result<Option<String>, ApiError> {
    let existing = sqlx::query_scalar::<_, Option<String>>(
        "select operating_initials from org.memberships where user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    match existing {
        Some(Some(initials)) => return Ok(Some(initials)),
        Some(None) => {}
        None => return Ok(None),
    }

    let candidates = operating_initial_candidates(first_name, last_name, display_name);

    for candidate in candidates {
        let updated = sqlx::query(
            r#"
            update org.memberships
            set operating_initials = $2,
                updated_at = now()
            where user_id = $1
              and operating_initials is null
            "#,
        )
        .bind(user_id)
        .bind(&candidate)
        .execute(&mut **tx)
        .await;

        match updated {
            Ok(result) if result.rows_affected() == 1 => return Ok(Some(candidate)),
            Ok(_) => {
                let current = sqlx::query_scalar::<_, Option<String>>(
                    "select operating_initials from org.memberships where user_id = $1",
                )
                .bind(user_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(|_| ApiError::Internal)?;

                return Ok(current.flatten());
            }
            Err(error) if is_unique_violation(&error) => continue,
            Err(_) => return Err(ApiError::Internal),
        }
    }

    Err(ApiError::Internal)
}

pub async fn ensure_visitor_membership(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    artcc: &str,
    rating: Option<&str>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into org.memberships (
            user_id,
            artcc,
            rating,
            division,
            controller_status,
            membership_status,
            is_active,
            updated_at
        )
        values ($3, $1, $2, 'USA', 'VISITOR', 'ACTIVE', true, now())
        on conflict (user_id) do update
        set artcc = excluded.artcc,
            rating = coalesce(excluded.rating, org.memberships.rating),
            membership_status = 'ACTIVE',
            is_active = true,
            updated_at = now()
        "#,
    )
    .bind(artcc)
    .bind(rating)
    .bind(user_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

pub async fn fetch_user_cid_artcc_rating(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> Result<(i64, Option<String>, Option<String>), ApiError> {
    sqlx::query_as::<_, (i64, Option<String>, Option<String>)>(
        "select u.cid, m.artcc, m.rating from identity.users u join org.memberships m on m.user_id = u.id where u.id = $1",
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_visitor_application_by_user_id(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<VisitorApplicationItem>, ApiError> {
    sqlx::query_as::<_, VisitorApplicationItem>(
        r#"
        select
            va.id,
            va.user_id,
            u.cid,
            u.display_name,
            va.home_facility,
            va.why_visit,
            va.status,
            va.reason_for_denial,
            va.submitted_at,
            va.decided_at,
            va.decided_by_actor_id
        from org.visitor_applications va
        join identity.users u on u.id = va.user_id
        where va.user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn list_visitor_applications(
    pool: &PgPool,
    status: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<VisitorApplicationItem>, ApiError> {
    sqlx::query_as::<_, VisitorApplicationItem>(
        r#"
        select
            va.id,
            va.user_id,
            u.cid,
            u.display_name,
            va.home_facility,
            va.why_visit,
            va.status,
            va.reason_for_denial,
            va.submitted_at,
            va.decided_at,
            va.decided_by_actor_id
        from org.visitor_applications va
        join identity.users u on u.id = va.user_id
        where ($1::text is null or va.status = $1)
        order by va.submitted_at desc
        limit $2 offset $3
        "#,
    )
    .bind(status)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn find_visitor_application_by_id(
    pool: &PgPool,
    application_id: &str,
) -> Result<Option<VisitorApplicationItem>, ApiError> {
    sqlx::query_as::<_, VisitorApplicationItem>(
        r#"
        select
            va.id,
            va.user_id,
            u.cid,
            u.display_name,
            va.home_facility,
            va.why_visit,
            va.status,
            va.reason_for_denial,
            va.submitted_at,
            va.decided_at,
            va.decided_by_actor_id
        from org.visitor_applications va
        join identity.users u on u.id = va.user_id
        where va.id = $1
        "#,
    )
    .bind(application_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn upsert_visitor_application(
    pool: &PgPool,
    user_id: &str,
    home_facility: &str,
    why_visit: &str,
) -> Result<VisitorApplicationItem, ApiError> {
    sqlx::query_as::<_, VisitorApplicationItem>(
        r#"
        insert into org.visitor_applications (
            user_id,
            home_facility,
            why_visit,
            status,
            submitted_at,
            reason_for_denial,
            decided_at,
            decided_by_actor_id
        )
        values ($1, $2, $3, 'PENDING', now(), null, null, null)
        on conflict (user_id) do update
        set home_facility = excluded.home_facility,
            why_visit = excluded.why_visit,
            status = 'PENDING',
            submitted_at = now(),
            reason_for_denial = null,
            decided_at = null,
            decided_by_actor_id = null,
            updated_at = now()
        returning
            id,
            user_id,
            (select cid from identity.users where id = org.visitor_applications.user_id) as cid,
            (select display_name from identity.users where id = org.visitor_applications.user_id) as display_name,
            home_facility,
            why_visit,
            status,
            reason_for_denial,
            submitted_at,
            decided_at,
            decided_by_actor_id
        "#,
    )
    .bind(user_id)
    .bind(home_facility)
    .bind(why_visit)
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn decide_visitor_application(
    tx: &mut Transaction<'_, Postgres>,
    application_id: &str,
    status: &str,
    reason_for_denial: Option<&str>,
    decided_by_actor_id: Option<&str>,
    artcc: &str,
) -> Result<Option<VisitorApplicationItem>, ApiError> {
    let updated = sqlx::query_as::<_, VisitorApplicationItem>(
        r#"
        update org.visitor_applications
        set status = $2,
            reason_for_denial = $3,
            decided_at = now(),
            decided_by_actor_id = $4,
            updated_at = now()
        where id = $1
        returning
            id,
            user_id,
            (select cid from identity.users where id = org.visitor_applications.user_id) as cid,
            (select display_name from identity.users where id = org.visitor_applications.user_id) as display_name,
            home_facility,
            why_visit,
            status,
            reason_for_denial,
            submitted_at,
            decided_at,
            decided_by_actor_id
        "#,
    )
    .bind(application_id)
    .bind(status)
    .bind(reason_for_denial)
    .bind(decided_by_actor_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    if let Some(application) = updated
        .as_ref()
        .filter(|application| application.status == "APPROVED")
    {
        activate_visitor_membership(tx, &application.user_id, artcc, &application.home_facility)
            .await?;
    }

    Ok(updated)
}

async fn activate_visitor_membership(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    artcc: &str,
    home_facility: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"
        insert into org.memberships (
            user_id,
            artcc,
            division,
            controller_status,
            membership_status,
            visitor_home_facility,
            home_facility,
            is_active,
            updated_at
        )
        values ($1, $2, 'USA', 'VISITOR', 'ACTIVE', $3, null, true, now())
        on conflict (user_id) do update
        set artcc = excluded.artcc,
            division = 'USA',
            controller_status = 'VISITOR',
            membership_status = 'ACTIVE',
            visitor_home_facility = excluded.visitor_home_facility,
            home_facility = null,
            is_active = true,
            updated_at = now()
        "#,
    )
    .bind(user_id)
    .bind(artcc)
    .bind(home_facility)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(())
}

fn is_unique_violation(error: &SqlxError) -> bool {
    match error {
        SqlxError::Database(database_error) => database_error.code().as_deref() == Some("23505"),
        _ => false,
    }
}

fn operating_initial_candidates(
    first_name: Option<&str>,
    last_name: Option<&str>,
    display_name: &str,
) -> Vec<String> {
    let (resolved_first, resolved_last) = resolve_name_parts(first_name, last_name, display_name);
    let first_letters = normalized_letters(&resolved_first);
    let last_letters = normalized_letters(&resolved_last);

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    if !first_letters.is_empty() && !last_letters.is_empty() {
        push_candidate(
            &mut candidates,
            &mut seen,
            [first_letters[0], last_letters[0]],
        );

        for &letter in last_letters.iter().skip(1) {
            push_candidate(&mut candidates, &mut seen, [first_letters[0], letter]);
        }

        for &letter in first_letters.iter().skip(1) {
            push_candidate(&mut candidates, &mut seen, [letter, last_letters[0]]);
        }

        let mut combined = first_letters.clone();
        combined.extend(last_letters.iter().copied());
        push_pair_combinations(&combined, &mut candidates, &mut seen);
    } else {
        let single_letters = if first_letters.is_empty() {
            last_letters
        } else {
            first_letters
        };

        if single_letters.len() >= 2 {
            push_candidate(
                &mut candidates,
                &mut seen,
                [single_letters[0], single_letters[1]],
            );
        }

        push_pair_combinations(&single_letters, &mut candidates, &mut seen);
    }

    for first in b'A'..=b'Z' {
        for second in b'A'..=b'Z' {
            push_candidate(
                &mut candidates,
                &mut seen,
                [char::from(first), char::from(second)],
            );
        }
    }

    candidates
}

fn resolve_name_parts(
    first_name: Option<&str>,
    last_name: Option<&str>,
    display_name: &str,
) -> (String, String) {
    let first = first_name.unwrap_or_default().trim();
    let last = last_name.unwrap_or_default().trim();

    if !first.is_empty() || !last.is_empty() {
        return (first.to_string(), last.to_string());
    }

    let mut parts = display_name
        .split_whitespace()
        .filter(|part| !part.is_empty());
    let first_part = parts.next().unwrap_or_default().to_string();
    let last_part = parts.last().unwrap_or_default().to_string();

    if last_part.is_empty() {
        (first_part, String::new())
    } else {
        (first_part, last_part)
    }
}

fn normalized_letters(value: &str) -> Vec<char> {
    value
        .chars()
        .flat_map(|ch| ch.to_uppercase())
        .filter(|ch| ch.is_ascii_alphabetic())
        .collect()
}

fn push_pair_combinations(
    letters: &[char],
    candidates: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    for first_index in 0..letters.len() {
        for second_index in (first_index + 1)..letters.len() {
            push_candidate(
                candidates,
                seen,
                [letters[first_index], letters[second_index]],
            );
        }
    }
}

fn push_candidate(candidates: &mut Vec<String>, seen: &mut HashSet<String>, letters: [char; 2]) {
    let candidate: String = letters.into_iter().collect();
    if candidate.len() == 2 && seen.insert(candidate.clone()) {
        candidates.push(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::operating_initial_candidates;

    #[test]
    fn operating_initials_start_with_first_and_last_initial() {
        let candidates = operating_initial_candidates(Some("Jane"), Some("Controller"), "");

        assert_eq!(candidates.first().map(String::as_str), Some("JC"));
    }

    #[test]
    fn operating_initials_fallback_uses_remaining_last_name_letters_first() {
        let candidates = operating_initial_candidates(Some("Jane"), Some("Controller"), "");

        assert_eq!(candidates.get(1).map(String::as_str), Some("JO"));
        assert_eq!(candidates.get(2).map(String::as_str), Some("JN"));
    }

    #[test]
    fn operating_initials_strip_non_letters() {
        let candidates = operating_initial_candidates(Some("Jo-An"), Some("O'Neil"), "");

        assert_eq!(candidates.first().map(String::as_str), Some("JO"));
    }

    #[test]
    fn operating_initials_handle_single_token_names() {
        let candidates = operating_initial_candidates(None, None, "Madonna");

        assert_eq!(candidates.first().map(String::as_str), Some("MA"));
    }

    #[test]
    fn operating_initials_fall_back_to_full_scan() {
        let candidates = operating_initial_candidates(None, None, "A");

        assert_eq!(candidates.first().map(String::as_str), Some("AA"));
        assert_eq!(candidates.get(25).map(String::as_str), Some("AZ"));
    }

    #[test]
    fn operating_initials_derive_from_display_name_when_parts_missing() {
        let candidates = operating_initial_candidates(None, None, "Jane Controller");

        assert_eq!(candidates.first().map(String::as_str), Some("JC"));
    }
}

pub async fn find_user_identity_by_cid(
    pool: &PgPool,
    cid: i64,
) -> Result<Option<(String, i64)>, ApiError> {
    sqlx::query_as::<_, (String, i64)>("select id, cid from identity.users where cid = $1")
        .bind(cid)
        .fetch_optional(pool)
        .await
        .map_err(|_| ApiError::Internal)
}
