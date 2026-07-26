use axum::{
    extract::{Extension, State},
    http::HeaderMap,
};
use chrono::Utc;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    auth::{context::CurrentUser, permissions::AuthProfileRead, require_permission::RequirePermission},
    errors::ApiError,
    models::data_export::{DataExportDocument, DataExportMeta, GdprNotice},
    repos::{audit as audit_repo, data_export as export_repo, users as user_repo},
    state::AppState,
    time::{ApiJson, ResponseTimeContext},
};

/// GDPR Article 15 self-service data export.
///
/// Returns a single JSON document assembling every domain that links to the
/// authenticated caller — identity, training, certifications, events, feedback,
/// incidents, workflows, notifications, visitor application, and their own audit
/// activity — alongside the Article 15(1) transparency notice. The request itself
/// is logged to the audit trail (Article 5(2) accountability).
///
/// Self-service only: gated by `auth.profile.read` and hard-scoped to the caller's
/// own `user.id` — there is no path to export another subject here.
#[utoipa::path(
    get,
    path = "/api/v1/me/data-export",
    tag = "data-export",
    responses(
        (status = 200, description = "The caller's personal-data export", body = DataExportDocument),
        (status = 401, description = "Not authenticated")
    )
)]
pub async fn export_my_data(
    State(state): State<AppState>,
    Extension(current_user): Extension<Option<CurrentUser>>,
    _permission: RequirePermission<AuthProfileRead>,
    headers: HeaderMap,
    time: ResponseTimeContext,
) -> Result<ApiJson<DataExportDocument>, ApiError> {
    let user = current_user.as_ref().ok_or(ApiError::Unauthorized)?;
    let pool = state.db.as_ref().ok_or(ApiError::ServiceUnavailable)?;
    let uid = user.id.as_str();

    // Dedicated tight per-user limit for this expensive cross-domain export — the
    // loose global per-IP limiter (spec 010) doesn't stop hammering this one
    // endpoint. Checked up front so a throttled request does no work.
    if state.rate_limit_enabled && state.data_export_limiter.check_key(&user.id).is_err() {
        return Err(ApiError::TooManyRequests);
    }

    // ----- identity -----
    let identity_core = export_repo::fetch_identity_core(pool, uid).await?;
    let flags = user_repo::fetch_user_flags(pool, uid).await?;
    let staff_positions = user_repo::list_held_staff_positions(pool, user.cid).await?;
    let roles = export_repo::fetch_roles(pool, uid).await?;
    let identity_links = export_repo::fetch_identity_links(pool, uid).await?;

    let show_welcome_message = identity_core
        .as_ref()
        .and_then(|row| row.show_welcome_message)
        .unwrap_or(false);

    let identity = json!({
        "profile": jv(&identity_core)?,
        "flags": jv(&flags)?,
        "staff_positions": jv(&staff_positions)?,
        "roles": jv(&roles)?,
        "linked_accounts": jv(&identity_links)?,
    });

    // ----- training -----
    let training = json!({
        "sessions_as_student": jv(&export_repo::fetch_sessions_as_student(pool, uid).await?)?,
        "sessions_as_instructor": jv(&export_repo::fetch_sessions_as_instructor(pool, uid).await?)?,
        "session_tickets": jv(&export_repo::fetch_tickets(pool, uid).await?)?,
        "appointments": jv(&export_repo::fetch_appointments(pool, uid).await?)?,
        "assignment": jv(&export_repo::fetch_assignment(pool, uid).await?)?,
        "assignment_requests": jv(&export_repo::fetch_assignment_requests(pool, uid).await?)?,
        "release_requests": jv(&export_repo::fetch_release_requests(pool, uid).await?)?,
        "progression": jv(&export_repo::fetch_progression(pool, uid).await?)?,
        "dossier_entries": jv(&export_repo::fetch_dossier(pool, uid).await?)?,
    });

    // ----- certifications -----
    let certifications = json!({
        "certifications": jv(&export_repo::fetch_certifications(pool, uid).await?)?,
        "solo_certifications": jv(&export_repo::fetch_solo_certifications(pool, uid).await?)?,
    });

    // ----- events -----
    let events = json!({
        "position_history": jv(&export_repo::fetch_event_positions(pool, uid).await?)?,
    });

    // ----- feedback -----
    let feedback = json!({
        "submitted": jv(&export_repo::fetch_feedback_submitted(pool, uid).await?)?,
        "received": jv(&export_repo::fetch_feedback_received(pool, uid).await?)?,
    });

    // ----- incidents -----
    let incidents = json!({
        "reported_by_me": jv(&export_repo::fetch_incidents_reported_by(pool, uid).await?)?,
        "reported_about_me": jv(&export_repo::fetch_incidents_reported_about(pool, uid).await?)?,
    });

    // ----- workflows -----
    let workflows = json!({
        "loas": jv(&export_repo::fetch_loas(pool, uid).await?)?,
        "staffing_requests": jv(&export_repo::fetch_staffing_requests(pool, uid).await?)?,
        "sua_requests": jv(&export_repo::fetch_sua_requests(pool, uid).await?)?,
    });

    // ----- notifications -----
    let notifications = json!({
        "broadcast_state": jv(&export_repo::fetch_broadcast_state(pool, uid).await?)?,
        "welcome_message": json!({ "show_welcome_message": show_welcome_message }),
        "emails_sent_to_me": jv(&export_repo::fetch_emails(pool, uid).await?)?,
    });

    // ----- visitor application -----
    let visitor_application = json!({
        "application": jv(&export_repo::fetch_visitor_application(pool, uid).await?)?,
    });

    // ----- activity log (own audit entries, metadata only) -----
    let activity_log = fetch_activity_log(pool, user).await?;

    // Article 5(2): log that the access request happened, and record the request.
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    audit_repo::record_audit(
        pool,
        audit_repo::AuditEntryInput {
            actor_id: actor.actor_id.clone(),
            action: "EXPORT".to_string(),
            resource_type: "DATA_EXPORT".to_string(),
            resource_id: Some(user.id.clone()),
            // Must be one of the audit_logs.scope_type CHECK values (0003_access.sql);
            // "self" is not valid and made this insert (and the whole export) 500.
            scope_type: "global".to_string(),
            scope_key: Some(user.cid.to_string()),
            before_state: None,
            after_state: None,
            ip_address: audit_repo::client_ip(&headers),
        },
    )
    .await?;

    let document = DataExportDocument {
        meta: DataExportMeta {
            generated_at: Utc::now(),
            subject_cid: user.cid,
            subject_user_id: user.id.clone(),
            format: "json".to_string(),
            gdpr_notice: GdprNotice::vzdc(),
        },
        identity,
        training,
        certifications,
        events,
        feedback,
        incidents,
        workflows,
        notifications,
        visitor_application,
        activity_log,
    };

    Ok(ApiJson::new(document, time))
}

/// The subject's own audit activity, reduced to metadata. `before_state` /
/// `after_state` bodies are deliberately dropped: for a staff member they can
/// contain other data subjects' data (e.g. a profile they edited), which is not
/// the requester's personal data. If the caller has no actor row, there is no
/// activity — and, critically, we must NOT fall through to an unfiltered
/// `fetch_audit_logs` (a `None` actor filter matches every row).
async fn fetch_activity_log(pool: &sqlx::PgPool, user: &CurrentUser) -> Result<Value, ApiError> {
    let actor = audit_repo::resolve_audit_actor(pool, Some(user), None).await?;
    let Some(actor_id) = actor.actor_id else {
        return Ok(json!([]));
    };

    let rows = audit_repo::fetch_audit_logs(
        pool,
        &audit_repo::AuditLogFilters {
            resource_type: None,
            resource_id: None,
            actor_id: Some(actor_id),
            actor_type: None,
            scope_type: None,
            scope_key: None,
            action: None,
            limit: 1000,
            offset: 0,
            // A self-service export must not surface server-level impersonation
            // (AUTH_IMPERSONATION) rows (spec 012 #2). Field added by Worker A's
            // impersonation change to the shared AuditLogFilters.
            include_server_sensitive: false,
        },
    )
    .await?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "action": row.action,
                "resource_type": row.resource_type,
                "resource_id": row.resource_id,
                "scope_type": row.scope_type,
                "scope_key": row.scope_key,
                "ip_address": row.ip_address,
                "created_at": row.created_at,
            })
        })
        .collect();

    Ok(json!(entries))
}

/// Serialize a value into the export document, mapping failures to `Internal`.
fn jv<T: Serialize>(value: &T) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|_| ApiError::Internal)
}
