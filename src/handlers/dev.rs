use axum::{Json, extract::State};
use serde::Serialize;

use crate::{errors::ApiError, state::AppState};

#[derive(Serialize)]
pub struct SeedResponse {
    ok: bool,
    users: usize,
    events: usize,
    training: usize,
}

pub async fn seed_data(State(state): State<AppState>) -> Result<Json<SeedResponse>, ApiError> {
    let Some(pool) = state.db.as_ref() else {
        return Err(ApiError::ServiceUnavailable);
    };

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    let staff_user_id = upsert_user(
        &mut tx,
        "seed-staff",
        10000010,
        "dev-staff@example.invalid",
        "Dev Staff",
        "STAFF",
        Some("Dev"),
        Some("Staff"),
        Some("ZDC"),
        Some("S3"),
        Some("USA"),
    )
    .await?;

    let student_user_id = upsert_user(
        &mut tx,
        "seed-student",
        10000011,
        "dev-student@example.invalid",
        "Dev Student",
        "USER",
        Some("Dev"),
        Some("Student"),
        Some("ZDC"),
        Some("S1"),
        Some("USA"),
    )
    .await?;

    let trainer_user_id = upsert_user(
        &mut tx,
        "seed-trainer",
        10000012,
        "dev-trainer@example.invalid",
        "Dev Trainer",
        "STAFF",
        Some("Dev"),
        Some("Trainer"),
        Some("ZDC"),
        Some("C1"),
        Some("USA"),
    )
    .await?;

    sqlx::query(
        "insert into access.user_roles (user_id, role_name) values ($1, 'USER') on conflict (user_id, role_name) do nothing",
    )
    .bind(&student_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    for user_id in [&staff_user_id, &trainer_user_id] {
        sqlx::query(
            "insert into access.user_roles (user_id, role_name) values ($1, 'STAFF') on conflict (user_id, role_name) do nothing",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| ApiError::Internal)?;
    }

    sqlx::query(
        r#"
        insert into events.events (id, title, type, host, description, status, published, starts_at, ends_at, created_by, created_at, updated_at)
        values (
            'seed-event-1',
            'Seeded Dev Event',
            'HOME',
            'ZDC Events',
            'Seeded event for local development',
            'SCHEDULED',
            true,
            now() + interval '1 day',
            now() + interval '1 day 4 hours',
            $1,
            now(),
            now()
        )
        on conflict (id) do update set
            title = excluded.title,
            type = excluded.type,
            host = excluded.host,
            description = excluded.description,
            status = excluded.status,
            published = excluded.published,
            starts_at = excluded.starts_at,
            ends_at = excluded.ends_at,
            created_by = excluded.created_by,
            updated_at = now()
        "#,
    )
    .bind(&staff_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into events.event_positions (id, event_id, callsign, user_id, requested_slot, assigned_slot, published, status, created_at, updated_at)
        values ('seed-event-position-1', 'seed-event-1', 'DCA_DEL', $1, 1, 1, true, 'ASSIGNED', now(), now())
        on conflict (id) do update set
            user_id = excluded.user_id,
            requested_slot = excluded.requested_slot,
            assigned_slot = excluded.assigned_slot,
            published = excluded.published,
            status = excluded.status,
            updated_at = now()
        "#,
    )
    .bind(&student_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into events.event_tmis (id, event_id, tmi_type, start_time, notes, created_at, updated_at)
        values ('seed-event-tmi-1', 'seed-event-1', 'briefing', now() + interval '23 hours', 'Seeded briefing', now(), now())
        on conflict (id) do update set
            tmi_type = excluded.tmi_type,
            start_time = excluded.start_time,
            notes = excluded.notes,
            updated_at = now()
        "#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into events.ops_plan_files (id, event_id, filename, url, file_type, uploaded_by, created_at, updated_at)
        values ('seed-ops-file-1', 'seed-event-1', 'seed-ops-plan.pdf', 'https://example.invalid/seed-ops-plan.pdf', 'pdf', $1, now(), now())
        on conflict (id) do update set
            filename = excluded.filename,
            url = excluded.url,
            file_type = excluded.file_type,
            uploaded_by = excluded.uploaded_by,
            updated_at = now()
        "#,
    )
    .bind(&staff_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    let assignment_id = sqlx::query_scalar::<_, String>(
        r#"
        insert into training.training_assignments (id, student_id, primary_trainer_id, created_at, updated_at)
        values ('seed-training-assignment-1', $1, $2, now(), now())
        on conflict (student_id) do update set
            primary_trainer_id = excluded.primary_trainer_id,
            updated_at = now()
        returning id
        "#,
    )
    .bind(&student_user_id)
    .bind(&staff_user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into training.training_assignment_other_trainers (assignment_id, trainer_id)
        values ($1, $2)
        on conflict (assignment_id, trainer_id) do nothing
        "#,
    )
    .bind(&assignment_id)
    .bind(&trainer_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into training.training_assignment_requests (id, student_id, submitted_at, status)
        values ('seed-training-request-1', $1, now(), 'PENDING')
        on conflict (id) do update set
            student_id = excluded.student_id,
            status = 'PENDING',
            decided_at = null,
            decided_by = null
        "#,
    )
    .bind(&student_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into training.training_assignment_request_interested_trainers (assignment_request_id, trainer_id)
        values ('seed-training-request-1', $1)
        on conflict (assignment_request_id, trainer_id) do nothing
        "#,
    )
    .bind(&trainer_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    sqlx::query(
        r#"
        insert into training.trainer_release_requests (id, student_id, submitted_at, status)
        values ('seed-trainer-release-1', $1, now(), 'PENDING')
        on conflict (id) do update set
            student_id = excluded.student_id,
            status = 'PENDING',
            decided_at = null,
            decided_by = null
        "#,
    )
    .bind(&student_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(Json(SeedResponse {
        ok: true,
        users: 3,
        events: 1,
        training: 3,
    }))
}

async fn upsert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &str,
    cid: i64,
    email: &str,
    display_name: &str,
    _role: &str,
    first_name: Option<&str>,
    last_name: Option<&str>,
    artcc: Option<&str>,
    rating: Option<&str>,
    division: Option<&str>,
) -> Result<String, ApiError> {
    let user_id = sqlx::query_scalar::<_, String>(
        r#"
        insert into identity.users (
            id,
            cid,
            email,
            full_name,
            display_name,
            first_name,
            last_name,
            status,
            updated_at
        )
        values ($1, $2, $3, $4, $4, $5, $6, 'ACTIVE', now())
        on conflict (cid) do update set
            email = excluded.email,
            full_name = excluded.full_name,
            display_name = excluded.display_name,
            first_name = excluded.first_name,
            last_name = excluded.last_name,
            status = excluded.status,
            updated_at = now()
        returning id
        "#,
    )
    .bind(id)
    .bind(cid)
    .bind(email)
    .bind(display_name)
    .bind(first_name)
    .bind(last_name)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

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
        values ($1, coalesce($2, 'ZDC'), $3, coalesce($4, 'USA'), 'HOME', 'ACTIVE', true, now())
        on conflict (user_id) do update
        set artcc = coalesce(excluded.artcc, org.memberships.artcc),
            rating = coalesce(excluded.rating, org.memberships.rating),
            division = coalesce(excluded.division, org.memberships.division),
            membership_status = 'ACTIVE',
            is_active = true,
            updated_at = now()
        "#,
    )
    .bind(&user_id)
    .bind(artcc)
    .bind(rating)
    .bind(division)
    .execute(&mut **tx)
    .await
    .map_err(|_| ApiError::Internal)?;

    Ok(user_id)
}
