use axum::{
    Json,
    extract::{Extension, Path, State},
};
use serde_json::json;

use crate::{
    auth::{
        context::CurrentUser, permissions::TrainingImpromptuCreate,
        require_permission::RequirePermission,
    },
    errors::ApiError,
    handlers::integrations::bot_api_post,
    models::{
        AcceptImpromptuOfferRequest, CreateImpromptuOfferRequest, ImpromptuOfferDetail,
        ImpromptuOfferItem, ImpromptuOfferListResponse, RecordImpromptuClaimRequest,
    },
    repos::training::impromptu as impromptu_repo,
    state::AppState,
};

const ALLOWED_SESSION_TYPES: &[&str] = &["ground", "tower", "approach", "center"];

#[derive(serde::Deserialize)]
struct BotOfferPosted {
    channel_id: String,
    message_id: String,
}

#[utoipa::path(post, path = "/api/v1/training/impromptu-offers", tag = "training", request_body = CreateImpromptuOfferRequest, responses((status = 200, description = "Impromptu offer posted", body = ImpromptuOfferItem), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.impromptu.create"), (status = 503, description = "Discord bot unavailable")))]
pub async fn create_impromptu_offer(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingImpromptuCreate>,
    Json(payload): Json<CreateImpromptuOfferRequest>,
) -> Result<Json<ImpromptuOfferItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let session_types: Vec<String> = payload
        .session_types
        .iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| ALLOWED_SESSION_TYPES.contains(&value.as_str()))
        .collect();
    if session_types.is_empty() {
        return Err(ApiError::BadRequest);
    }

    let offer_id = impromptu_repo::create_offer(
        pool,
        &user.id,
        &session_types,
        payload.available_at,
        payload
            .notes
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
    )
    .await?;

    // Ask the bot to post the offer embed; roll the offer back if it can't.
    let posted: Result<BotOfferPosted, ApiError> = bot_api_post(
        "/impromptu_offer",
        &json!({
            "offer_id": offer_id,
            "session_types": session_types,
            "mentor_name": user.display_name,
            "available_at": payload.available_at,
            "notes": payload.notes,
        }),
    )
    .await;
    let posted = match posted {
        Ok(posted) => posted,
        Err(error) => {
            let _ = impromptu_repo::delete_offer(pool, &offer_id).await;
            return Err(error);
        }
    };
    impromptu_repo::set_offer_discord_message(
        pool,
        &offer_id,
        &posted.channel_id,
        &posted.message_id,
    )
    .await?;

    let offer = impromptu_repo::get_offer(pool, &offer_id)
        .await?
        .ok_or(ApiError::Internal)?;
    Ok(Json(offer))
}

#[utoipa::path(get, path = "/api/v1/training/impromptu-offers", tag = "training", responses((status = 200, description = "My impromptu offers", body = ImpromptuOfferListResponse), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.impromptu.create")))]
pub async fn list_impromptu_offers(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingImpromptuCreate>,
) -> Result<Json<ImpromptuOfferListResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let items = impromptu_repo::list_offers_for_creator(pool, &user.id).await?;
    Ok(Json(ImpromptuOfferListResponse { items }))
}

#[utoipa::path(get, path = "/api/v1/training/impromptu-offers/{offer_id}", tag = "training", params(("offer_id" = String, Path, description = "Offer ID")), responses((status = 200, description = "Offer with claims", body = ImpromptuOfferDetail), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.impromptu.create"), (status = 404, description = "Not found")))]
pub async fn get_impromptu_offer(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingImpromptuCreate>,
    Path(offer_id): Path<String>,
) -> Result<Json<ImpromptuOfferDetail>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let offer = impromptu_repo::get_offer(pool, &offer_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if offer.created_by_user_id != user.id {
        return Err(ApiError::Forbidden);
    }
    let claims = impromptu_repo::get_offer_claims(pool, &offer_id).await?;
    Ok(Json(ImpromptuOfferDetail { offer, claims }))
}

#[utoipa::path(post, path = "/api/v1/training/impromptu-offers/{offer_id}/accept", tag = "training", params(("offer_id" = String, Path, description = "Offer ID")), request_body = AcceptImpromptuOfferRequest, responses((status = 200, description = "Accepted", body = ImpromptuOfferItem), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.impromptu.create")))]
pub async fn accept_impromptu_offer(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingImpromptuCreate>,
    Path(offer_id): Path<String>,
    Json(payload): Json<AcceptImpromptuOfferRequest>,
) -> Result<Json<ImpromptuOfferItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let offer = impromptu_repo::get_offer(pool, &offer_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if offer.created_by_user_id != user.id {
        return Err(ApiError::Forbidden);
    }

    let (accepted, rejected) =
        impromptu_repo::accept_offer(pool, &offer_id, &payload.user_id).await?;
    let discord = impromptu_repo::get_offer_discord_message(pool, &offer_id).await?;

    // Best-effort: DM the participants and delete the channel embed.
    let _: Result<serde_json::Value, ApiError> = bot_api_post(
        "/impromptu_offer/finalize",
        &json!({
            "channel_id": discord.as_ref().map(|(c, _)| c),
            "message_id": discord.as_ref().map(|(_, m)| m),
            "mentor_name": user.display_name,
            "session_types": offer.session_types,
            "accepted": accepted.as_ref().map(|c| json!({
                "discord_id": c.discord_id,
                "name": c.name,
            })),
            "rejected": rejected.iter().map(|c| json!({
                "discord_id": c.discord_id,
                "name": c.name,
            })).collect::<Vec<_>>(),
        }),
    )
    .await;

    let offer = impromptu_repo::get_offer(pool, &offer_id)
        .await?
        .ok_or(ApiError::Internal)?;
    Ok(Json(offer))
}

#[utoipa::path(post, path = "/api/v1/training/impromptu-offers/{offer_id}/cancel", tag = "training", params(("offer_id" = String, Path, description = "Offer ID")), responses((status = 200, description = "Cancelled", body = ImpromptuOfferItem), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks training.impromptu.create")))]
pub async fn cancel_impromptu_offer(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<TrainingImpromptuCreate>,
    Path(offer_id): Path<String>,
) -> Result<Json<ImpromptuOfferItem>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let offer = impromptu_repo::get_offer(pool, &offer_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if offer.created_by_user_id != user.id {
        return Err(ApiError::Forbidden);
    }

    impromptu_repo::cancel_offer(pool, &offer_id).await?;
    let discord = impromptu_repo::get_offer_discord_message(pool, &offer_id).await?;
    if let Some((channel_id, message_id)) = discord {
        let _: Result<serde_json::Value, ApiError> = bot_api_post(
            "/impromptu_offer/finalize",
            &json!({
                "channel_id": channel_id,
                "message_id": message_id,
                "mentor_name": user.display_name,
                "session_types": offer.session_types,
                "accepted": serde_json::Value::Null,
                "rejected": [],
            }),
        )
        .await;
    }

    let offer = impromptu_repo::get_offer(pool, &offer_id)
        .await?
        .ok_or(ApiError::Internal)?;
    Ok(Json(offer))
}

/// Bot-facing: record a student's claim by Discord id. Gated on the integrations
/// permission the bot's service account holds.
#[utoipa::path(post, path = "/api/v1/admin/integrations/discord/impromptu-claims", tag = "integrations", request_body = RecordImpromptuClaimRequest, responses((status = 200, description = "Claim recorded"), (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks integrations.stats.update"), (status = 404, description = "Discord account not linked")))]
pub async fn record_impromptu_claim(
    State(state): State<AppState>,
    _permission: RequirePermission<crate::auth::permissions::IntegrationsStatsUpdate>,
    Json(payload): Json<RecordImpromptuClaimRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    match impromptu_repo::record_claim_by_discord_id(pool, &payload.offer_id, &payload.discord_id)
        .await?
    {
        Some(()) => Ok(Json(json!({ "linked": true }))),
        None => Ok(Json(json!({ "linked": false }))),
    }
}
