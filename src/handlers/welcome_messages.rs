use axum::{
    Json,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
};
use chrono::Utc;

use crate::{
    auth::{
        context::{CurrentServiceAccount, CurrentUser},
        permissions::{
            AuthProfileRead, AuthProfileUpdate, WebWelcomeMessagesRead, WebWelcomeMessagesUpdate,
        },
        require_permission::RequirePermission,
    },
    errors::ApiError,
    models::{MyWelcomeMessageResponse, UpdateWelcomeMessageContentRequest, WelcomeMessageContent},
    repos::{
        audit as audit_repo, org::controller_lifecycle, welcome_messages as welcome_messages_repo,
    },
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

#[utoipa::path(
    get,
    path = "/api/v1/admin/welcome-messages",
    tag = "welcome-messages",
    responses(
        (status = 200, description = "Welcome message content", body = WelcomeMessageContent),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn get_welcome_message_content(
    State(state): State<AppState>,
    _permission: RequirePermission<WebWelcomeMessagesRead>,
    time: ResponseTimeContext,
) -> Result<ApiJson<WelcomeMessageContent>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let content = welcome_messages_repo::fetch_welcome_message_content(pool).await?;

    Ok(ApiJson::new(content, time))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/welcome-messages",
    tag = "welcome-messages",
    request_body = UpdateWelcomeMessageContentRequest,
    responses(
        (status = 200, description = "Welcome message content updated", body = WelcomeMessageContent),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn update_welcome_message_content(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    _permission: RequirePermission<WebWelcomeMessagesUpdate>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<UpdateWelcomeMessageContentRequest>,
) -> Result<ApiJson<WelcomeMessageContent>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    let before = welcome_messages_repo::fetch_welcome_message_content(pool).await?;

    welcome_messages_repo::update_welcome_message_content(
        &mut *tx,
        payload.home_text.trim(),
        payload.visitor_text.trim(),
        Some(&user.id),
        Utc::now(),
    )
    .await?;

    let after = WelcomeMessageContent {
        home_text: payload.home_text.trim().to_string(),
        visitor_text: payload.visitor_text.trim().to_string(),
    };

    let actor = audit_repo::resolve_audit_actor(
        &mut *tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;
    audit_repo::record_audit(
        &mut *tx,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "WELCOME_MESSAGES".to_string(),
            resource_id: Some("welcome_messages".to_string()),
            scope_type: "web".to_string(),
            scope_key: Some("welcome_messages".to_string()),
            before_state: Some(audit_repo::sanitized_snapshot(&before)?),
            after_state: Some(audit_repo::sanitized_snapshot(&after)?),
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(ApiJson::new(after, time))
}

#[utoipa::path(
    get,
    path = "/api/v1/welcome-message",
    tag = "welcome-messages",
    responses(
        (status = 200, description = "Current user's welcome message state", body = MyWelcomeMessageResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn get_my_welcome_message(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
    time: ResponseTimeContext,
) -> Result<ApiJson<MyWelcomeMessageResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let state_row = welcome_messages_repo::fetch_my_welcome_state(pool, &user.id).await?;

    let Some(state_row) = state_row else {
        return Ok(ApiJson::new(
            MyWelcomeMessageResponse {
                show: false,
                text: None,
            },
            time,
        ));
    };

    if !state_row.show_welcome_message {
        return Ok(ApiJson::new(
            MyWelcomeMessageResponse {
                show: false,
                text: None,
            },
            time,
        ));
    }

    let content = welcome_messages_repo::fetch_welcome_message_content(pool).await?;
    let text = match state_row.controller_status.as_deref() {
        Some("HOME") => Some(content.home_text),
        Some("VISITOR") => Some(content.visitor_text),
        _ => None,
    };

    Ok(ApiJson::new(
        MyWelcomeMessageResponse { show: true, text },
        time,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/welcome-message/ack",
    tag = "welcome-messages",
    responses(
        (status = 204, description = "Welcome message acknowledged"),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn acknowledge_welcome_message(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileUpdate>,
) -> Result<StatusCode, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    controller_lifecycle::disable_welcome_message(pool, &user.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
