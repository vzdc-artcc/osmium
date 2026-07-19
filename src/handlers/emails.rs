use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use http::HeaderMap;

use chrono::Utc;

use crate::{
    auth::{
        acl::{PermissionAction, PermissionPath},
        context::{CurrentServiceAccount, CurrentUser},
        middleware::ensure_permission,
    },
    email::{branding::validate_branding_input, service::actor_from_context},
    errors::ApiError,
    models::{
        EmailBranding, EmailOutboxListResponse, EmailPreferencesQuery, EmailPreferencesResponse,
        EmailPreferencesUpdateRequest, EmailPreviewRequest, EmailPreviewResponse,
        EmailResubscribeRequest, EmailSendRequest, EmailSendResponse,
        EmailSuppressionRecordResponse, EmailTemplateDefinitionResponse, ListEmailOutboxQuery,
        PaginationMeta, PaginationQuery,
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
        (status = 401, description = "Not authorized"),
        (status = 503, description = "Email system unavailable")
    )
)]
pub async fn send_email(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(request): Json<EmailSendRequest>,
) -> Result<ApiJson<EmailSendResponse>, ApiError> {
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
    Ok(ApiJson::new(response, time))
}

#[utoipa::path(
    get,
    path = "/api/v1/emails/outbox",
    tag = "emails",
    params(
        PaginationQuery,
        ("status" = Option<String>, Query, description = "Optional outbox status filter"),
        ("template_id" = Option<String>, Query, description = "Optional template filter")
    ),
    responses(
        (status = 200, description = "Email outbox", body = EmailOutboxListResponse),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn list_outbox(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Query(query): Query<ListEmailOutboxQuery>,
    time: ResponseTimeContext,
) -> Result<ApiJson<EmailOutboxListResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "outbox"], PermissionAction::Read),
    )
    .await?;

    let pagination =
        PaginationQuery::from_parts(query.page, query.page_size, query.limit, query.offset)
            .resolve(50, 200);
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        select count(*)::bigint
        from email.outbox o
        where ($1::text is null or o.status = $1)
          and ($2::text is null or o.template_id = $2)
        "#,
    )
    .bind(query.status.as_deref())
    .bind(query.template_id.as_deref())
    .fetch_one(pool)
    .await
    .map_err(|_| ApiError::Internal)?;
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
        (status = 401, description = "Not authorized")
    )
)]
pub async fn get_outbox_detail(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    Path(id): Path<String>,
    time: ResponseTimeContext,
) -> Result<ApiJson<crate::models::EmailOutboxDetailResponse>, ApiError> {
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "outbox"], PermissionAction::Read),
    )
    .await?;

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

#[utoipa::path(
    get,
    path = "/api/v1/admin/emails/branding",
    tag = "emails",
    responses(
        (status = 200, description = "Email branding configuration", body = EmailBranding),
        (status = 401, description = "Not authorized")
    )
)]
pub async fn get_email_branding(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    time: ResponseTimeContext,
) -> Result<ApiJson<EmailBranding>, ApiError> {
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "branding"], PermissionAction::Read),
    )
    .await?;

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
        (status = 401, description = "Not authorized")
    )
)]
pub async fn update_email_branding(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    Extension(current_service_account): Extension<Option<CurrentServiceAccount>>,
    headers: HeaderMap,
    time: ResponseTimeContext,
    Json(payload): Json<crate::models::UpdateEmailBrandingRequest>,
) -> Result<ApiJson<EmailBranding>, ApiError> {
    ensure_permission(
        &state,
        current_user.as_ref(),
        current_service_account.as_ref(),
        PermissionPath::from_segments(["emails", "branding"], PermissionAction::Update),
    )
    .await?;

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
            before_state: before.as_ref().map(audit::sanitized_snapshot).transpose()?,
            after_state: Some(audit::sanitized_snapshot(&after)?),
            ip_address: audit::client_ip(&headers),
        },
    )
    .await?;

    tx.commit().await.map_err(|_| ApiError::Internal)?;

    Ok(ApiJson::new(after, time))
}
