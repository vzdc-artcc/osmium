use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

/// GDPR Article 15 "right of access" self-service export document.
///
/// The heavy per-domain sections are carried as free-form JSON objects
/// (`serde_json::Value`) so the OpenAPI surface stays legible while the handler
/// assembles them from many typed row queries. The section names document what an
/// export contains; see `handlers/data_export.rs` for the exact per-table shapes.
#[derive(Debug, Serialize, ToSchema)]
pub struct DataExportDocument {
    /// Transparency metadata required alongside the raw records (Article 15(1)).
    pub meta: DataExportMeta,
    /// Identity, profile, membership, flags, roles, and linked accounts.
    #[schema(value_type = Object)]
    pub identity: Value,
    /// Training sessions (as student and as instructor), appointments, assignment,
    /// requests, progression, and dossier entries about the subject.
    #[schema(value_type = Object)]
    pub training: Value,
    /// Certification grid and solo certifications.
    #[schema(value_type = Object)]
    pub certifications: Value,
    /// Published event-position history.
    #[schema(value_type = Object)]
    pub events: Value,
    /// Feedback submitted by, and received about, the subject.
    #[schema(value_type = Object)]
    pub feedback: Value,
    /// Incident reports filed by, and about, the subject.
    #[schema(value_type = Object)]
    pub incidents: Value,
    /// LOA, staffing, and SUA requests.
    #[schema(value_type = Object)]
    pub workflows: Value,
    /// Broadcast acknowledgements, welcome-message state, and emails sent to the subject.
    #[schema(value_type = Object)]
    pub notifications: Value,
    /// Visitor application, if any.
    #[schema(value_type = Object)]
    pub visitor_application: Value,
    /// Activity log — audit entries for actions the subject performed (metadata only).
    #[schema(value_type = Object)]
    pub activity_log: Value,
}

/// Admin bulk export: every on-roster controller's `DataExportDocument` in one
/// payload. SERVER_ADMIN-only (`users.data_export.read`) — this is the entire
/// roster's personal data, so it is gated far more tightly than the self-service
/// Article 15 export and is not something a data subject can reach.
#[derive(Debug, Serialize, ToSchema)]
pub struct MassDataExportDocument {
    pub generated_at: DateTime<Utc>,
    /// Number of subjects (on-roster controllers) included.
    pub subject_count: i64,
    /// The same transparency notice that accompanies each per-subject document.
    pub gdpr_notice: GdprNotice,
    /// One full export document per on-roster controller, ordered by CID.
    pub subjects: Vec<DataExportDocument>,
}

/// Article 15(1)(a)–(h) transparency information that must accompany the records.
#[derive(Debug, Serialize, ToSchema)]
pub struct DataExportMeta {
    pub generated_at: DateTime<Utc>,
    pub subject_cid: i64,
    pub subject_user_id: String,
    /// Machine-readable format (also satisfies the Article 20 portability right).
    pub format: String,
    pub gdpr_notice: GdprNotice,
}

/// Static GDPR transparency notice. Boilerplate, not queried per-request, but must
/// be present in what is returned (Article 15(1)).
#[derive(Debug, Serialize, ToSchema)]
pub struct GdprNotice {
    pub legal_basis: String,
    pub purposes: Vec<String>,
    pub data_categories: Vec<String>,
    pub recipients: Vec<String>,
    pub retention: String,
    pub data_sources: Vec<String>,
    pub your_rights: Vec<String>,
    /// Documents how trainer/staff internal evaluative notes are handled in this export.
    pub evaluative_notes_disclosure: String,
    pub contact: String,
}

impl GdprNotice {
    /// The vZDC-specific access notice. Kept here as one place to review the wording.
    pub fn vzdc() -> Self {
        GdprNotice {
            legal_basis:
                "GDPR Article 15 (right of access) and Article 20 (data portability)."
                    .to_string(),
            purposes: vec![
                "Operating a VATSIM Air Route Traffic Control Center (vZDC): roster \
                 management, controller training and certification, event staffing, \
                 feedback and incident handling, and member communications."
                    .to_string(),
            ],
            data_categories: vec![
                "Identity and profile (name, CID, email, timezone, bio)".to_string(),
                "Membership and roster status".to_string(),
                "Training records (sessions, appointments, tickets, progression, dossier)"
                    .to_string(),
                "Certifications and solo endorsements".to_string(),
                "Event participation history".to_string(),
                "Feedback and incident reports".to_string(),
                "Leave-of-absence, staffing, and SUA requests".to_string(),
                "Linked external accounts (Discord, TeamSpeak)".to_string(),
                "Email delivery records and audit activity".to_string(),
            ],
            recipients: vec![
                "vZDC training and administrative staff (internal)".to_string(),
                "VATUSA / VATSIM, via roster synchronization".to_string(),
                "The configured transactional email provider, for messages sent to you"
                    .to_string(),
            ],
            retention:
                "Records are retained for as long as you hold a vZDC roster membership and, \
                 where required, afterward to meet operational and audit obligations."
                    .to_string(),
            data_sources: vec![
                "Provided directly by you (profile, form submissions)".to_string(),
                "Generated by vZDC staff about you (training records, feedback, dossier)"
                    .to_string(),
                "Synchronized from VATSIM/VATUSA (roster, rating)".to_string(),
            ],
            your_rights: vec![
                "Rectification of inaccurate data (Article 16)".to_string(),
                "Erasure, subject to retention obligations (Article 17)".to_string(),
                "Restriction of processing (Article 18)".to_string(),
                "Objection to processing (Article 21)".to_string(),
                "Lodging a complaint with a supervisory authority".to_string(),
            ],
            evaluative_notes_disclosure:
                "This export includes staff- and trainer-authored evaluative notes written \
                 about you (training-session trainer comments, feedback staff comments, and \
                 dossier entries), as these constitute your personal data under Article 15. \
                 The identities of the individual staff authors are not included."
                    .to_string(),
            contact:
                "Contact the vZDC Air Traffic Manager / Data Protection contact to exercise \
                 any of the above rights."
                    .to_string(),
        }
    }
}
