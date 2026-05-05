use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use http::HeaderMap;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::{CurrentServiceAccount, CurrentUser},
        middleware::ensure_permission,
    },
    email::service::actor_from_context,
    errors::ApiError,
    models::{
        EmailPreferencesQuery, EmailPreferencesResponse, EmailPreferencesUpdateRequest,
        EmailPreviewRequest, EmailPreviewResponse, EmailResubscribeRequest, EmailSendRequest,
        EmailSendResponse, EmailSuppressionRecordResponse, EmailTemplateDefinitionResponse,
        ListEmailOutboxQuery,
    },
    repos::audit,
    state::AppState,
};

#[utoipa::path(
    get,
    path = "/api/v1/emails/templates",
    tag = "emails",
    responses(
        (status = 200, description = "Email templates", body = [EmailTemplateDefinitionResponse]),
        (status = 401, description = "Not authorized"),
        (status = 503, description = "Email system unavailable")
    )
)]
pub async fn list_templates(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
) -> Result<Json<Vec<EmailTemplateDefinitionResponse>>, ApiError> {
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "templates"], PermissionAction::Read),
    )
    .await?;

    Ok(Json(state.email.templates()))
}

#[utoipa::path(
    post,
    path = "/api/v1/emails/preview",
    tag = "emails",
    request_body = EmailPreviewRequest,
    responses(
        (status = 200, description = "Rendered email preview", body = EmailPreviewResponse),
        (status = 401, description = "Not authorized"),
        (status = 503, description = "Email system unavailable")
    )
)]
pub async fn preview_email(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Json(request): Json<EmailPreviewRequest>,
) -> Result<Json<EmailPreviewResponse>, ApiError> {
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "preview"], PermissionAction::Create),
    )
    .await?;

    Ok(Json(state.email.preview_template(
        &request.template_id,
        &request.payload,
    )?))
}

#[utoipa::path(
    post,
    path = "/api/v1/emails/send",
    tag = "emails",
    request_body = EmailSendRequest,
    responses(
        (status = 200, description = "Queued email send", body = EmailSendResponse),
        (status = 401, description = "Not authorized"),
        (status = 503, description = "Email system unavailable")
    )
)]
pub async fn send_email(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    headers: HeaderMap,
    Json(request): Json<EmailSendRequest>,
) -> Result<Json<EmailSendResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "send"], PermissionAction::Create),
    )
    .await?;

    let resolved_actor = audit::resolve_audit_actor(
        pool,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;

    let actor = actor_from_context(
        current_user.as_ref(),
        current_service_account.as_ref(),
        resolved_actor.actor_id,
        "api",
    );

    let response = state
        .email
        .enqueue_template_send(pool, actor, request)
        .await?;
    let _ = headers;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/v1/emails/outbox",
    tag = "emails",
    params(ListEmailOutboxQuery),
    responses(
        (status = 200, description = "Email outbox", body = [crate::models::EmailOutboxListItem]),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_outbox(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Query(query): Query<ListEmailOutboxQuery>,
) -> Result<Json<Vec<crate::models::EmailOutboxListItem>>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "outbox"], PermissionAction::Read),
    )
    .await?;

    Ok(Json(state.email.list_outbox(pool, &query).await?))
}

#[utoipa::path(
    get,
    path = "/api/v1/emails/outbox/{id}",
    tag = "emails",
    params(("id" = String, Path, description = "Outbox id")),
    responses(
        (status = 200, description = "Email outbox detail", body = crate::models::EmailOutboxDetailResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn get_outbox_detail(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(id): Path<String>,
) -> Result<Json<crate::models::EmailOutboxDetailResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "outbox"], PermissionAction::Read),
    )
    .await?;

    Ok(Json(state.email.get_outbox_detail(pool, &id).await?))
}

#[utoipa::path(
    get,
    path = "/api/v1/emails/preferences",
    tag = "emails",
    params(EmailPreferencesQuery),
    responses(
        (status = 200, description = "Email preference state", body = EmailPreferencesResponse),
        (status = 400, description = "Invalid token")
    )
)]
pub async fn get_preferences(
    State(state): State<AppState>,
    Query(query): Query<EmailPreferencesQuery>,
) -> Result<Json<EmailPreferencesResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(Json(state.email.get_preferences(pool, &query.token).await?))
}

#[utoipa::path(
    post,
    path = "/api/v1/emails/preferences",
    tag = "emails",
    request_body = EmailPreferencesUpdateRequest,
    responses(
        (status = 200, description = "Updated email preference state", body = EmailPreferencesResponse),
        (status = 400, description = "Invalid token or preferences")
    )
)]
pub async fn update_preferences(
    State(state): State<AppState>,
    Json(request): Json<EmailPreferencesUpdateRequest>,
) -> Result<Json<EmailPreferencesResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(Json(state.email.update_preferences(pool, &request).await?))
}

#[utoipa::path(
    post,
    path = "/api/v1/emails/resubscribe",
    tag = "emails",
    request_body = EmailResubscribeRequest,
    responses(
        (status = 200, description = "Resubscribed", body = EmailSuppressionRecordResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn resubscribe(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Json(request): Json<EmailResubscribeRequest>,
) -> Result<Json<EmailSuppressionRecordResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "suppressions"], PermissionAction::Update),
    )
    .await?;
    Ok(Json(state.email.resubscribe(pool, &request).await?))
}
