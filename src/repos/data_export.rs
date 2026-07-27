//! Read-only cross-domain queries backing the GDPR self-service data export
//! (`GET /api/v1/me/data-export`, spec 012, Worker B).
//!
//! Every query is scoped to a single subject `user_id`. Row structs are internal
//! (`Serialize` for the JSON dump, `FromRow` for the query) — the export's typed
//! OpenAPI surface lives in `models/data_export.rs`.
//!
//! Third-party-data-leakage rule applied throughout (Article 15(4)): records are
//! scoped to "identifying context for records about *this* subject", never full
//! profiles of everyone mentioned. Concretely:
//!   * Secrets are never selected (no OAuth tokens from `user_identities`).
//!   * For records with two parties (feedback, incidents), the *other* party's
//!     identity is included only when the subject authored the record (their own
//!     action); when the subject is the passive party, the counterparty identity
//!     is omitted.
//!   * Staff authors of evaluative notes (dossier, feedback staff comments) are
//!     not identified.

use serde::Serialize;
use sqlx::PgPool;

use crate::errors::ApiError;

// ---------------------------------------------------------------------------
// Roster subject enumeration (admin mass export)
// ---------------------------------------------------------------------------

/// Lists `(user_id, cid)` for every on-roster controller, ordered by CID.
///
/// Same roster predicate as `users::list_roster_users` (`controller_status` set
/// and not `NONE`) against the same `org.v_user_roster_profile` view, so the mass
/// export covers exactly the population the roster page shows — bounded, and never
/// the full `identity.users` history of everyone who ever logged in.
pub async fn list_roster_subject_ids(pool: &PgPool) -> Result<Vec<(String, i64)>, ApiError> {
    sqlx::query_as::<_, (String, i64)>(
        r#"
        select v.id, v.cid
        from org.v_user_roster_profile v
        where v.controller_status is not null and v.controller_status <> 'NONE'
        order by v.cid asc
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IdentityCoreRow {
    pub cid: Option<i64>,
    pub email: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub full_name: String,
    pub preferred_name: Option<String>,
    pub display_name: String,
    pub status: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub bio: Option<String>,
    pub timezone: Option<String>,
    pub receive_email: Option<bool>,
    pub new_event_notifications: Option<bool>,
    pub show_welcome_message: Option<bool>,
    pub artcc: Option<String>,
    pub division: Option<String>,
    pub rating: Option<String>,
    pub controller_status: Option<String>,
    pub membership_status: Option<String>,
    pub operating_initials: Option<String>,
    pub join_date: Option<chrono::DateTime<chrono::Utc>>,
    pub home_facility: Option<String>,
    pub visitor_home_facility: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn fetch_identity_core(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<IdentityCoreRow>, ApiError> {
    sqlx::query_as::<_, IdentityCoreRow>(
        r#"
        select
            u.cid, u.email, u.first_name, u.last_name, u.full_name, u.preferred_name,
            u.display_name, u.status, u.joined_at, u.last_seen_at, u.created_at,
            p.bio, p.timezone, p.receive_email, p.new_event_notifications, p.show_welcome_message,
            m.artcc, m.division, m.rating, m.controller_status, m.membership_status,
            m.operating_initials, m.join_date, m.home_facility, m.visitor_home_facility, m.is_active
        from identity.users u
        left join identity.user_profiles p on p.user_id = u.id
        left join org.memberships m on m.user_id = u.id
        where u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RoleRow {
    pub role_name: String,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn fetch_roles(pool: &PgPool, user_id: &str) -> Result<Vec<RoleRow>, ApiError> {
    sqlx::query_as::<_, RoleRow>(
        "select role_name, source, created_at from access.user_roles where user_id = $1 order by role_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IdentityLinkRow {
    pub provider: String,
    pub provider_subject: String,
    pub provider_username: Option<String>,
    pub provider_email: Option<String>,
    pub scopes: Vec<String>,
    pub linked_at: chrono::DateTime<chrono::Utc>,
    pub last_refreshed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub token_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Linked external accounts. Deliberately never selects `access_token`,
/// `refresh_token`, or `id_token` — those are secrets, not personal data to export.
pub async fn fetch_identity_links(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<IdentityLinkRow>, ApiError> {
    sqlx::query_as::<_, IdentityLinkRow>(
        r#"
        select provider, provider_subject, provider_username, provider_email,
               scopes, linked_at, last_refreshed_at, token_expires_at
        from identity.user_identities
        where user_id = $1
        order by provider
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Training
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TrainingSessionRow {
    pub id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end_at: chrono::DateTime<chrono::Utc>,
    pub additional_comments: Option<String>,
    /// Trainer-only evaluative comments. Included as the subject's own personal
    /// data per the Article 15 disclosure decision (see GdprNotice).
    pub trainer_comments: Option<String>,
    pub enable_markdown: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub counterparty_cid: Option<i64>,
    pub counterparty_name: String,
}

/// Sessions where the subject was the student. `counterparty` is the instructor.
pub async fn fetch_sessions_as_student(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<TrainingSessionRow>, ApiError> {
    fetch_sessions(pool, user_id, true).await
}

/// Sessions where the subject was the instructor. `counterparty` is the student.
pub async fn fetch_sessions_as_instructor(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<TrainingSessionRow>, ApiError> {
    fetch_sessions(pool, user_id, false).await
}

async fn fetch_sessions(
    pool: &PgPool,
    user_id: &str,
    as_student: bool,
) -> Result<Vec<TrainingSessionRow>, ApiError> {
    // Both roles are the subject's own operational training data, so trainer
    // comments are included in either direction.
    let query = if as_student {
        r#"
        select s.id, s.start, s."end" as end_at, s.additional_comments, s.trainer_comments,
               s.enable_markdown, s.created_at,
               c.cid as counterparty_cid, c.display_name as counterparty_name
        from training.training_sessions s
        join identity.users c on c.id = s.instructor_id
        where s.student_id = $1
        order by s.start desc
        "#
    } else {
        r#"
        select s.id, s.start, s."end" as end_at, s.additional_comments, s.trainer_comments,
               s.enable_markdown, s.created_at,
               c.cid as counterparty_cid, c.display_name as counterparty_name
        from training.training_sessions s
        join identity.users c on c.id = s.student_id
        where s.instructor_id = $1
        order by s.start desc
        "#
    };

    sqlx::query_as::<_, TrainingSessionRow>(query)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TrainingTicketRow {
    pub session_id: String,
    pub lesson_identifier: String,
    pub lesson_name: String,
    pub passed: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Tickets for every session in which the subject was the student or instructor.
pub async fn fetch_tickets(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<TrainingTicketRow>, ApiError> {
    sqlx::query_as::<_, TrainingTicketRow>(
        r#"
        select t.session_id, l.identifier as lesson_identifier, l.name as lesson_name,
               t.passed, t.created_at
        from training.training_tickets t
        join training.training_sessions s on s.id = t.session_id
        join training.lessons l on l.id = t.lesson_id
        where s.student_id = $1 or s.instructor_id = $1
        order by t.created_at desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AppointmentRow {
    pub id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub environment: Option<String>,
    pub double_booking: bool,
    pub preparation_completed: bool,
    pub notes: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub counterparty_cid: Option<i64>,
    pub counterparty_name: String,
}

pub async fn fetch_appointments(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<AppointmentRow>, ApiError> {
    sqlx::query_as::<_, AppointmentRow>(
        r#"
        select a.id, a.start, a.environment, a.double_booking, a.preparation_completed,
               a.notes, a.created_at,
               t.cid as counterparty_cid, t.display_name as counterparty_name
        from training.training_appointments a
        join identity.users t on t.id = a.trainer_id
        where a.student_id = $1
        order by a.start desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AssignmentRow {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub primary_trainer_cid: Option<i64>,
    pub primary_trainer_name: String,
}

pub async fn fetch_assignment(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<AssignmentRow>, ApiError> {
    sqlx::query_as::<_, AssignmentRow>(
        r#"
        select a.created_at, pt.cid as primary_trainer_cid, pt.display_name as primary_trainer_name
        from training.training_assignments a
        join identity.users pt on pt.id = a.primary_trainer_id
        where a.student_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RequestRow {
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn fetch_assignment_requests(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<RequestRow>, ApiError> {
    sqlx::query_as::<_, RequestRow>(
        "select submitted_at, status, decided_at from training.training_assignment_requests where student_id = $1 order by submitted_at desc",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

pub async fn fetch_release_requests(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<RequestRow>, ApiError> {
    sqlx::query_as::<_, RequestRow>(
        "select submitted_at, status, decided_at from training.trainer_release_requests where student_id = $1 order by submitted_at desc",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ProgressionRow {
    pub progression_name: String,
    pub assigned_at: chrono::DateTime<chrono::Utc>,
}

pub async fn fetch_progression(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<ProgressionRow>, ApiError> {
    sqlx::query_as::<_, ProgressionRow>(
        r#"
        select tp.name as progression_name, up.assigned_at
        from training.user_progressions up
        join training.training_progressions tp on tp.id = up.progression_id
        where up.user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DossierRow {
    /// Always `"[redacted]"`. The dossier author (a staff member) is third-party
    /// personal data and is deliberately withheld from the data subject's export;
    /// see the GDPR `evaluative_notes_disclosure` notice. The real `writer_id` is
    /// never selected from the database.
    pub author: String,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub is_confidential: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Dossier entries written about the subject. The staff author (`writer_id`) is
/// deliberately not selected — the content is the subject's personal data, the
/// author's identity is the author's.
pub async fn fetch_dossier(pool: &PgPool, user_id: &str) -> Result<Vec<DossierRow>, ApiError> {
    sqlx::query_as::<_, DossierRow>(
        "select '[redacted]' as author, message, timestamp, is_confidential, created_at from feedback.dossier_entries where user_id = $1 order by timestamp desc",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Certifications
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CertificationRow {
    pub certification_type: String,
    pub certification_option: String,
    pub granted_at: chrono::DateTime<chrono::Utc>,
}

pub async fn fetch_certifications(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<CertificationRow>, ApiError> {
    sqlx::query_as::<_, CertificationRow>(
        r#"
        select ct.name as certification_type, uc.certification_option, uc.granted_at
        from org.user_certifications uc
        join org.certification_types ct on ct.id = uc.certification_type_id
        where uc.user_id = $1
        order by ct.sort_order
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SoloCertificationRow {
    pub certification_type: String,
    pub position: String,
    pub expires: chrono::DateTime<chrono::Utc>,
    pub granted_at: chrono::DateTime<chrono::Utc>,
}

pub async fn fetch_solo_certifications(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<SoloCertificationRow>, ApiError> {
    sqlx::query_as::<_, SoloCertificationRow>(
        r#"
        select ct.name as certification_type, sc.position, sc.expires, sc.granted_at
        from org.user_solo_certifications sc
        join org.certification_types ct on ct.id = sc.certification_type_id
        where sc.user_id = $1
        order by sc.expires desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EventPositionRow {
    pub event_title: String,
    pub event_start: chrono::DateTime<chrono::Utc>,
    pub callsign: String,
    pub requested_position: Option<String>,
    pub final_position: Option<String>,
    pub final_start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub final_end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub published: bool,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

pub async fn fetch_event_positions(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<EventPositionRow>, ApiError> {
    sqlx::query_as::<_, EventPositionRow>(
        r#"
        select e.title as event_title, e.starts_at as event_start, ep.callsign,
               ep.requested_position, ep.final_position, ep.final_start_time, ep.final_end_time,
               ep.status, ep.published, ep.submitted_at
        from events.event_positions ep
        join events.events e on e.id = ep.event_id
        where ep.user_id = $1
        order by e.starts_at desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Feedback
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FeedbackSubmittedRow {
    pub target_cid: Option<i64>,
    pub target_name: String,
    pub pilot_callsign: String,
    pub controller_position: String,
    pub rating: i32,
    pub comments: Option<String>,
    pub status: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Feedback the subject submitted about others. The target is named — it's the
/// subject's own submission. Staff comments (internal, about the target) omitted.
pub async fn fetch_feedback_submitted(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<FeedbackSubmittedRow>, ApiError> {
    sqlx::query_as::<_, FeedbackSubmittedRow>(
        r#"
        select target.cid as target_cid, target.display_name as target_name,
               f.pilot_callsign, f.controller_position, f.rating, f.comments, f.status,
               f.submitted_at, f.decided_at
        from feedback.feedback_items f
        join identity.users target on target.id = f.target_user_id
        where f.submitter_user_id = $1
        order by f.submitted_at desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FeedbackReceivedRow {
    pub pilot_callsign: String,
    pub controller_position: String,
    pub rating: i32,
    pub comments: Option<String>,
    /// Staff-only evaluative comments about the subject — included as their
    /// personal data (Article 15); the submitter's identity is omitted.
    pub staff_comments: Option<String>,
    pub status: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Feedback received about the subject. Submitter identity omitted (third party).
pub async fn fetch_feedback_received(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<FeedbackReceivedRow>, ApiError> {
    sqlx::query_as::<_, FeedbackReceivedRow>(
        r#"
        select f.pilot_callsign, f.controller_position, f.rating, f.comments,
               f.staff_comments, f.status, f.submitted_at, f.decided_at
        from feedback.feedback_items f
        where f.target_user_id = $1
        order by f.submitted_at desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Incidents
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IncidentReportedByRow {
    pub reportee_cid: Option<i64>,
    pub reportee_name: String,
    pub reason: String,
    pub reporter_callsign: Option<String>,
    pub reportee_callsign: Option<String>,
    pub closed: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Incidents the subject filed (they're the reporter). The reportee is named —
/// it's the subject's own report.
pub async fn fetch_incidents_reported_by(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<IncidentReportedByRow>, ApiError> {
    sqlx::query_as::<_, IncidentReportedByRow>(
        r#"
        select reportee.cid as reportee_cid, reportee.display_name as reportee_name,
               ir.reason, ir.reporter_callsign, ir.reportee_callsign, ir.closed,
               ir.timestamp, ir.created_at
        from feedback.incident_reports ir
        join identity.users reportee on reportee.id = ir.reportee_id
        where ir.reporter_id = $1
        order by ir.timestamp desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IncidentReportedAboutRow {
    pub reason: String,
    pub reporter_callsign: Option<String>,
    pub reportee_callsign: Option<String>,
    pub closed: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Incidents filed about the subject. Reporter identity omitted (third party).
pub async fn fetch_incidents_reported_about(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<IncidentReportedAboutRow>, ApiError> {
    sqlx::query_as::<_, IncidentReportedAboutRow>(
        r#"
        select ir.reason, ir.reporter_callsign, ir.reportee_callsign, ir.closed,
               ir.timestamp, ir.created_at
        from feedback.incident_reports ir
        where ir.reportee_id = $1
        order by ir.timestamp desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Workflows: LOA / staffing / SUA
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LoaRow {
    pub start: chrono::DateTime<chrono::Utc>,
    pub end_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
    pub status: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn fetch_loas(pool: &PgPool, user_id: &str) -> Result<Vec<LoaRow>, ApiError> {
    sqlx::query_as::<_, LoaRow>(
        r#"select start, "end" as end_at, reason, status, submitted_at, decided_at
           from org.loas where user_id = $1 order by submitted_at desc"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StaffingRequestRow {
    pub name: String,
    pub description: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn fetch_staffing_requests(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<StaffingRequestRow>, ApiError> {
    sqlx::query_as::<_, StaffingRequestRow>(
        "select name, description, created_at from org.staffing_requests where user_id = $1 order by created_at desc",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SuaRequestRow {
    pub start_at: chrono::DateTime<chrono::Utc>,
    pub end_at: chrono::DateTime<chrono::Utc>,
    pub afiliation: String,
    pub details: String,
    pub mission_number: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn fetch_sua_requests(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<SuaRequestRow>, ApiError> {
    sqlx::query_as::<_, SuaRequestRow>(
        r#"select start_at, end_at, afiliation, details, mission_number, created_at
           from org.sua_blocks where user_id = $1 order by start_at desc"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Notifications: broadcasts / welcome / emails
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BroadcastStateRow {
    pub title: String,
    pub seen_at: Option<chrono::DateTime<chrono::Utc>>,
    pub agreed_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn fetch_broadcast_state(
    pool: &PgPool,
    user_id: &str,
) -> Result<Vec<BroadcastStateRow>, ApiError> {
    sqlx::query_as::<_, BroadcastStateRow>(
        r#"
        select cb.title, cbs.seen_at, cbs.agreed_at
        from web.change_broadcast_user_state cbs
        join web.change_broadcasts cb on cb.id = cbs.broadcast_id
        where cbs.user_id = $1
        order by cb.timestamp desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EmailRow {
    pub template_id: String,
    pub category: String,
    pub subject_override: Option<String>,
    pub email: String,
    pub delivery_status: String,
    pub sent_at: Option<chrono::DateTime<chrono::Utc>>,
    pub queued_at: chrono::DateTime<chrono::Utc>,
}

/// Emails sent to the subject. The rendered body/payload is intentionally omitted
/// (it can reference other data); metadata is sufficient for an access request.
pub async fn fetch_emails(pool: &PgPool, user_id: &str) -> Result<Vec<EmailRow>, ApiError> {
    sqlx::query_as::<_, EmailRow>(
        r#"
        select o.template_id, o.category, o.subject_override, r.email,
               r.delivery_status, r.sent_at, o.queued_at
        from email.outbox_recipients r
        join email.outbox o on o.id = r.outbox_id
        where r.user_id = $1
        order by o.queued_at desc
        limit 500
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|_| ApiError::Internal)
}

// ---------------------------------------------------------------------------
// Visitor application
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct VisitorApplicationRow {
    pub home_facility: String,
    pub why_visit: String,
    pub status: String,
    pub reason_for_denial: Option<String>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn fetch_visitor_application(
    pool: &PgPool,
    user_id: &str,
) -> Result<Option<VisitorApplicationRow>, ApiError> {
    sqlx::query_as::<_, VisitorApplicationRow>(
        r#"
        select home_facility, why_visit, status, reason_for_denial, submitted_at, decided_at
        from org.visitor_applications
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiError::Internal)
}
