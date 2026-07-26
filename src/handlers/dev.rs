use axum::{Json, extract::State};
use serde::Serialize;

use crate::{errors::ApiError, repos::dev as dev_repo, state::AppState};

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

    let staff_user_id = dev_repo::upsert_user(
        &mut tx,
        "seed-staff",
        10000010,
        "dev-staff@example.invalid",
        "Dev Staff",
        Some("Dev"),
        Some("Staff"),
        Some("ZDC"),
        Some("S3"),
        Some("USA"),
    )
    .await?;

    let student_user_id = dev_repo::upsert_user(
        &mut tx,
        "seed-student",
        10000011,
        "dev-student@example.invalid",
        "Dev Student",
        Some("Dev"),
        Some("Student"),
        Some("ZDC"),
        Some("S1"),
        Some("USA"),
    )
    .await?;

    let trainer_user_id = dev_repo::upsert_user(
        &mut tx,
        "seed-trainer",
        10000012,
        "dev-trainer@example.invalid",
        "Dev Trainer",
        Some("Dev"),
        Some("Trainer"),
        Some("ZDC"),
        Some("C1"),
        Some("USA"),
    )
    .await?;

    dev_repo::grant_user_role(&mut tx, &student_user_id, "USER").await?;

    for user_id in [&staff_user_id, &trainer_user_id] {
        dev_repo::grant_user_role(&mut tx, user_id, "STAFF").await?;
    }

    dev_repo::upsert_event(&mut tx, &staff_user_id).await?;
    dev_repo::upsert_event_position(&mut tx, &student_user_id).await?;
    dev_repo::upsert_event_tmi(&mut tx).await?;
    dev_repo::upsert_ops_plan_file(&mut tx, &staff_user_id).await?;

    let assignment_id =
        dev_repo::upsert_training_assignment(&mut tx, &student_user_id, &staff_user_id).await?;
    dev_repo::insert_assignment_other_trainer(&mut tx, &assignment_id, &trainer_user_id).await?;
    dev_repo::upsert_assignment_request(&mut tx, &student_user_id).await?;
    dev_repo::insert_assignment_request_interested_trainer(&mut tx, &trainer_user_id).await?;
    dev_repo::upsert_trainer_release_request(&mut tx, &student_user_id).await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(Json(SeedResponse {
        ok: true,
        users: 3,
        events: 1,
        training: 3,
    }))
}
