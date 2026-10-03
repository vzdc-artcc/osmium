use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use http::HeaderMap;

use chrono::Utc;

use crate::{
    auth::{
        context::{CurrentServiceAccount, CurrentUser},
        permissions::{
            AuthProfileRead, AuthProfileUpdate, EmailsBrandingRead, EmailsBrandingUpdate,
            EmailsOutboxRead, EmailsPreviewCreate, EmailsSendCreate, EmailsSuppressionsUpdate,
            EmailsTemplatesRead,
        },
        require_permission::RequirePermission,
    },
    email::{branding::validate_branding_input, service::actor_from_context},
    errors::ApiError,
    models::{
        EmailBranding, EmailOutboxListResponse, EmailPreferencesQuery, EmailPreferencesResponse,
        EmailPreferencesUpdateRequest, EmailPreviewRequest, EmailPreviewResponse,
        EmailResubscribeRequest, EmailSendRequest, EmailSendResponse,
        EmailSuppressionRecordResponse, EmailTemplateDefinitionResponse, ListEmailOutboxQuery,
        MeEmailPreferencesUpdateRequest, PaginationMeta, PaginationQuery,
    },
    repos::{audit, email_branding},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

#[utoipa::path(
    get,
    path = "/api/v1/emails/templates",
    tag = "emails",
    responses(
        (status = 200, description = "Email templates", body = [EmailTemplateDefinitionResponse]),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks emails.templates.read"),
        (status = 503, description = "Email system unavailable")
    )
)]
pub async fn list_templates(
    State(state): State<AppState>,
    _permission: RequirePermission<EmailsTemplatesRead>,
) -> Result<Json<Vec<EmailTemplateDefinitionResponse>>, ApiError> {
    Ok(Json(state.email.templates()))
}

#[utoipa::path(
    post,
    path = "/api/v1/emails/preview",
    tag = "emails",
    request_body = EmailPreviewRequest,
    responses(
        (status = 200, description = "Rendered email preview", body = EmailPreviewResponse),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks emails.preview.create"),
        (status = 503, description = "Email system unavailable")
    )
)]
pub async fn preview_email(
    State(state): State<AppState>,
    _permission: RequirePermission<EmailsPreviewCreate>,
    Json(request): Json<EmailPreviewRequest>,
) -> Result<Json<EmailPreviewResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let branding = match request.branding_override.as_ref() {
        Some(draft) => {
            validate_branding_input(draft)?;
            EmailBranding {
                brand_name: draft.brand_name.clone(),
                tagline: draft.tagline.clone(),
                footer_text: draft.footer_text.clone(),
                logo_file_id: draft.logo_file_id.clone(),
                header_background_color: draft.header_background_color.clone(),
                header_text_color: draft.header_text_color.clone(),
                page_background_color: draft.page_background_color.clone(),
                panel_background_color: draft.panel_background_color.clone(),
                text_color: draft.text_color.clone(),
                heading_color: draft.heading_color.clone(),
                link_color: draft.link_color.clone(),
                accent_color: draft.accent_color.clone(),
                button_background_color: draft.button_background_color.clone(),
                button_text_color: draft.button_text_color.clone(),
                heading_font_family: draft.heading_font_family.clone(),
                body_font_family: draft.body_font_family.clone(),
                font_size_scale: draft.font_size_scale.clone(),
                corner_style: draft.corner_style.clone(),
                updated_at: Utc::now(),
            }
        }
        None => email_branding::fetch_branding(pool)
            .await?
            .ok_or(ApiError::Internal)?,
    };

    Ok(Json(state.email.preview_template(
        &request.template_id,
        &request.payload,
        &branding,
    )?))
}

#[utoipa::path(
    post,
    path = "/api/v1/emails/send",
    tag = "emails",
    request_body = EmailSendRequest,
    responses(
        (status = 200, description = "Queued email send", body = EmailSendResponse),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks emails.send.create"),
        (status = 503, description = "Email system unavailable")
    )
)]
pub async fn send_email(
    State(state): State<AppState>,
    _permission: RequirePermission<EmailsSendCreate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(request): Json<EmailSendRequest>,
) -> Result<ApiJson<EmailSendResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

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
    Ok(ApiJson::new(response, time))
}

#[utoipa::path(
    get,
    path = "/api/v1/emails/outbox",
    tag = "emails",
    params(ListEmailOutboxQuery),
    responses(
        (status = 200, description = "Email outbox", body = EmailOutboxListResponse),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks emails.outbox.read")
    )
)]
pub async fn list_outbox(
    State(state): State<AppState>,
    _permission: RequirePermission<EmailsOutboxRead>,
    Query(query): Query<ListEmailOutboxQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<EmailOutboxListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 200);
    let total = state.email.count_outbox(pool, &query).await?;
    let items = state.email.list_outbox(pool, &query).await?;
    let meta = PaginationMeta::new(total, pagination.page, pagination.page_size);

    Ok(ApiJson::new(
        EmailOutboxListResponse {
            items,
            pagination: meta,
        },
        time,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/emails/outbox/{id}",
    tag = "emails",
    params(("id" = String, Path, description = "Outbox id")),
    responses(
        (status = 200, description = "Email outbox detail", body = crate::models::EmailOutboxDetailResponse),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks emails.outbox.read")
    )
)]
pub async fn get_outbox_detail(
    State(state): State<AppState>,
    _permission: RequirePermission<EmailsOutboxRead>,
    Path(id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<crate::models::EmailOutboxDetailResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;

    Ok(ApiJson::new(
        state.email.get_outbox_detail(pool, &id).await?,
        time,
    ))
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

/// Self-service, session-authenticated email preferences (the profile "Email
/// Preferences" section). Same per-category model as the token-based unsubscribe
/// flow, but the caller's email is resolved from their session rather than an
/// unsubscribe token. Gated by `auth.profile.read` (baseline self-read).
#[utoipa::path(
    get,
    path = "/api/v1/me/email-preferences",
    tag = "emails",
    responses(
        (status = 200, description = "The caller's per-category email preferences", body = EmailPreferencesResponse),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks auth.profile.read")
    )
)]
pub async fn get_my_email_preferences(
    State(state): State<AppState>,
    _permission: RequirePermission<AuthProfileRead>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
) -> Result<ApiJson<EmailPreferencesResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(ApiJson::new(
        state.email.get_email_preferences(pool, &user.email).await?,
        time,
    ))
}

/// Update the caller's own per-category email preferences. Subscribe revokes the
/// suppression; unsubscribe creates it. Transactional categories cannot be
/// unsubscribed (rejected as a bad request). Gated by `auth.profile.update`.
#[utoipa::path(
    put,
    path = "/api/v1/me/email-preferences",
    tag = "emails",
    request_body = MeEmailPreferencesUpdateRequest,
    responses(
        (status = 200, description = "Updated per-category email preferences", body = EmailPreferencesResponse),
        (status = 400, description = "Invalid request (unknown or transactional category)"),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks auth.profile.update")
    )
)]
pub async fn update_my_email_preferences(
    State(state): State<AppState>,
    _permission: RequirePermission<AuthProfileUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    time: ResponseTimeContext,
    Json(request): Json<MeEmailPreferencesUpdateRequest>,
) -> Result<ApiJson<EmailPreferencesResponse>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(ApiJson::new(
        state
            .email
            .update_email_preferences(pool, &user.email, Some(&user.id), &request.preferences)
            .await?,
        time,
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/emails/resubscribe",
    tag = "emails",
    request_body = EmailResubscribeRequest,
    responses(
        (status = 200, description = "Resubscribed", body = EmailSuppressionRecordResponse),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks emails.suppressions.update")
    )
)]
pub async fn resubscribe(
    State(state): State<AppState>,
    _permission: RequirePermission<EmailsSuppressionsUpdate>,
    Json(request): Json<EmailResubscribeRequest>,
) -> Result<Json<EmailSuppressionRecordResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    Ok(Json(state.email.resubscribe(pool, &request).await?))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/emails/branding",
    tag = "emails",
    responses(
        (status = 200, description = "Email branding configuration", body = EmailBranding),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks emails.branding.read")
    )
)]
pub async fn get_email_branding(
    State(state): State<AppState>,
    _permission: RequirePermission<EmailsBrandingRead>,
    time: ResponseTimeContext,
) -> Result<ApiJson<EmailBranding>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let branding = email_branding::fetch_branding(pool)
        .await?
        .ok_or(ApiError::Internal)?;

    Ok(ApiJson::new(branding, time))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/emails/branding",
    tag = "emails",
    request_body = crate::models::UpdateEmailBrandingRequest,
    responses(
        (status = 200, description = "Email branding configuration updated", body = EmailBranding),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Not authenticated"), (status = 403, description = "Lacks emails.branding.update")
    )
)]
pub async fn update_email_branding(
    State(state): State<AppState>,
    _permission: RequirePermission<EmailsBrandingUpdate>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<crate::models::UpdateEmailBrandingRequest>,
) -> Result<ApiJson<EmailBranding>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    validate_branding_input(&payload)?;

    if let Some(file_id) = payload.logo_file_id.as_deref() {
        let is_public = email_branding::logo_file_is_public(pool, file_id).await?;
        if !is_public {
            return Err(ApiError::BadRequest);
        }
    }

    let mut tx = pool.begin().await.map_err(|_| ApiError::Internal)?;

    let before = email_branding::fetch_branding(&mut *tx).await?;

    let actor = audit::resolve_audit_actor(
        &mut *tx,
        current_user.as_ref(),
        current_service_account.as_ref(),
    )
    .await?;

    let updated_by_user_id = current_user.as_ref().map(|user| user.id.as_str());
    let after =
        email_branding::upsert_branding(&mut *tx, &payload, updated_by_user_id, Utc::now()).await?;

    audit::record_audit(
        &mut *tx,
        audit::AuditEntryInput {
            actor_id: actor.actor_id,
            action: "UPDATE".to_string(),
            resource_type: "EMAIL_BRANDING".to_string(),
            resource_id: Some("default".to_string()),
            scope_type: "web".to_string(),
            scope_key: Some("default".to_string()),
            message: None,
            before_state: before.as_ref().map(audit::sanitized_snapshot).transpose()?,
            after_state: Some(audit::sanitized_snapshot(&after)?),
            ip_address: audit::client_ip(&headers),
        },
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(ApiJson::new(after, time))
}
