use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{
    IntoParams, PartialSchema, ToSchema,
    openapi::{
        RefOr,
        schema::{AdditionalProperties, ObjectBuilder, Schema, SchemaType, Type},
    },
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchMeRequest {
    pub preferred_name: Option<Option<String>>,
    pub timezone: Option<String>,
    pub bio: Option<Option<String>>,
    pub operating_initials: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTeamSpeakUidRequest {
    pub uid: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RosterUserRow {
    pub id: String,
    pub cid: i64,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub preferred_name: Option<String>,
    pub artcc: Option<String>,
    pub rating: Option<String>,
    pub division: Option<String>,
    pub status: Option<String>,
    pub controller_status: Option<String>,
    pub membership_status: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub join_date: Option<DateTime<Utc>>,
    pub home_facility: Option<String>,
    pub visitor_home_facility: Option<String>,
    pub is_active: Option<bool>,
    pub hidden_from_roster: bool,
    pub operating_initials: Option<String>,
    pub role_names: Vec<String>,
    pub bio: Option<String>,
    pub timezone: Option<String>,
    pub avatar_asset_id: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ListUsersQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// When true, only include users who currently hold an active controller
    /// status (HOME or VISITOR), matching the live site's roster-picker filter.
    pub controllers_only: Option<bool>,
    /// When set, only include users who currently hold this exact role name
    /// (e.g. "INS" for instructor, "MTR" for mentor) among all of their
    /// roles — checked against the full role set, not just the single
    /// highest-priority "primary role" the listing otherwise displays, so a
    /// user who is both STAFF and INS is still matched by `role=INS`.
    pub role: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct AdminUserListItem {
    pub id: String,
    pub cid: i64,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub artcc: Option<String>,
    pub rating: Option<String>,
    pub division: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserBasicInfo {
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct MeProfileBody {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub preferred_name: Option<String>,
    pub bio: Option<String>,
    pub timezone: String,
    pub operating_initials: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct TeamSpeakUidBody {
    pub id: String,
    pub uid: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub linked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TeamSpeakLookupRequest {
    /// The TeamSpeak client UID to resolve to a controller.
    pub uid: String,
}

/// Controller identity + live position for a TeamSpeak UID. Backs the
/// TeamSpeak server's presence lookup (formerly the website's Prisma-backed
/// `/api/teamspeak` route). `online_position` is null when the controller is
/// not currently connected.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct TeamSpeakLookupResponse {
    pub cid: i64,
    pub controller_status: Option<String>,
    pub rating: Option<String>,
    pub online_position: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MeBody {
    pub id: String,
    pub cid: i64,
    pub email: String,
    pub display_name: String,
    pub rating: Option<String>,
    pub controller_status: Option<String>,
    pub server_admin: bool,
    pub role_names: Vec<String>,
    pub permissions: serde_json::Value,
    pub profile: MeProfileBody,
    pub flags: UserFlagsBody,
    pub teamspeak_uids: Vec<TeamSpeakUidBody>,
    /// Present only while this session is impersonating another user (spec 012).
    /// Carries just enough real-actor identity for a global "acting as …" banner
    /// and a stop control — no more (security checklist #10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impersonation: Option<ImpersonationBanner>,
}

/// Minimal real-admin identity surfaced on `/me` while impersonating.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ImpersonationBanner {
    pub impersonator_cid: i64,
    pub impersonator_display_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserPrivateInfo {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub preferred_name: Option<String>,
    pub artcc: Option<String>,
    pub division: Option<String>,
    pub status: Option<String>,
    pub controller_status: Option<String>,
    pub membership_status: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub join_date: Option<DateTime<Utc>>,
    pub home_facility: Option<String>,
    pub visitor_home_facility: Option<String>,
    pub is_active: Option<bool>,
    pub operating_initials: Option<String>,
    /// The user's full set of assigned role names (e.g. `["STAFF", "INS"]`),
    /// unlike `role` above which is just the single highest-priority
    /// "primary role" used for display. Needed by pickers that need to
    /// check role membership (e.g. "is this user an instructor or mentor")
    /// without an extra per-user request.
    pub role_names: Vec<String>,
    pub bio: Option<String>,
    pub timezone: Option<String>,
    pub avatar_asset_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserListItem {
    pub basic: UserBasicInfo,
    pub full: Option<UserPrivateInfo>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserListResponse {
    pub items: Vec<UserListItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Serialize, ToSchema)]
pub struct UserDetailsResponse {
    pub basic: UserBasicInfo,
    pub full: Option<UserFullInfo>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ManualVatusaRefreshOutcome {
    Home,
    Visitor,
    OffRoster,
}

#[derive(Serialize, ToSchema)]
pub struct ManualVatusaRefreshResult {
    pub cid: i64,
    pub membership_outcome: ManualVatusaRefreshOutcome,
    pub detail_refreshed: bool,
    pub membership_updated: bool,
    pub message: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ManualVatusaRefreshResponse {
    pub user: UserDetailsResponse,
    pub refresh_result: ManualVatusaRefreshResult,
}

#[derive(Serialize, ToSchema)]
pub struct UserFullInfo {
    pub profile: UserPrivateInfo,
    pub roles: Vec<String>,
    pub permissions: serde_json::Value,
    pub stats: UserStats,
}

#[derive(Serialize, ToSchema)]
pub struct UserOverviewBody {
    pub user: AdminUserListItem,
    pub roles: Vec<String>,
    pub permissions: serde_json::Value,
    pub stats: UserStats,
}

#[derive(Serialize, sqlx::FromRow, ToSchema)]
pub struct UserStats {
    pub active_sessions: i64,
    pub assigned_event_positions: i64,
    pub training_assignments_as_student: i64,
    pub training_assignments_as_primary_trainer: i64,
    pub training_assignments_as_other_trainer: i64,
    pub training_assignment_requests: i64,
    pub training_assignment_interests: i64,
    pub trainer_release_requests: i64,
}

#[derive(Deserialize, ToSchema)]
pub struct VisitArtccRequest {
    pub artcc: String,
    pub rating: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct VisitArtccResponse {
    pub cid: i64,
    pub artcc: String,
    pub rating: Option<String>,
    pub status: String,
    pub roster_added: bool,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct UserFeedbackQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserFeedbackListResponse {
    pub items: Vec<crate::models::FeedbackItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Deserialize, ToSchema)]
pub struct SetControllerStatusRequest {
    pub controller_status: String,
    pub artcc: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SetControllerStatusBody {
    pub cid: i64,
    pub controller_status: String,
    pub artcc: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct UserFlagsBody {
    pub no_request_loas: bool,
    pub no_request_training_assignments: bool,
    pub no_request_trainer_release: bool,
    pub no_force_progression_finish: bool,
    pub no_event_signup: bool,
    pub no_edit_profile: bool,
    pub excluded_from_roster_sync: bool,
    pub hidden_from_roster: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateUserFlagsRequest {
    pub no_request_loas: bool,
    pub no_request_training_assignments: bool,
    pub no_request_trainer_release: bool,
    pub no_force_progression_finish: bool,
    pub no_event_signup: bool,
    pub no_edit_profile: bool,
    pub excluded_from_roster_sync: bool,
    pub hidden_from_roster: bool,
    pub reason: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateOperatingInitialsRequest {
    pub operating_initials: String,
}

/// Admin edit of another controller's profile (preferred name / bio / timezone).
/// Operating initials stay on the dedicated operating-initials endpoint; flags
/// stay on the flags endpoint; the event-notification opt-in is a personal
/// self-service preference (`PATCH /me`) and is intentionally not touched here.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AdminUpdateProfileRequest {
    pub preferred_name: Option<String>,
    pub bio: Option<String>,
    pub timezone: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateOperatingInitialsResponse {
    pub cid: i64,
    pub operating_initials: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VisitorApplicationItem {
    pub id: String,
    pub user_id: String,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
    pub home_facility: String,
    pub why_visit: String,
    pub status: String,
    pub reason_for_denial: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub submitted_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub decided_at: Option<DateTime<Utc>>,
    pub decided_by_actor_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateVisitorApplicationRequest {
    pub home_facility: String,
    pub why_visit: String,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ListVisitorApplicationsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
    pub cid: Option<i64>,
    /// Contains-match against the applicant's display name.
    pub display_name: Option<String>,
    /// Contains-match against the applicant's stated home facility.
    pub home_facility: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VisitorApplicationListResponse {
    pub items: Vec<VisitorApplicationItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminUserListResponse {
    pub items: Vec<AdminUserListItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

/// One of a user's active auth sessions, for the admin session manager. Deliberately
/// omits the raw `session_token` — it is never exposed.
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct UserSessionItem {
    pub id: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: DateTime<Utc>,
    /// True when this session is currently impersonating (i.e. an admin is acting as
    /// this user through it); `impersonator_cid` names the real admin.
    pub impersonated: bool,
    pub impersonator_cid: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserSessionListResponse {
    pub items: Vec<UserSessionItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideVisitorApplicationRequest {
    pub status: String,
    pub reason_for_denial: Option<String>,
}

impl ToSchema for PatchMeRequest {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("PatchMeRequest")
    }
}

impl PartialSchema for PatchMeRequest {
    fn schema() -> RefOr<Schema> {
        let nullable_string: SchemaType = [Type::String, Type::Null].into_iter().collect();

        ObjectBuilder::new()
            .description(Some(
                "Self-service profile update payload. Only profile fields are accepted here. \
                 Roles, permissions, and access overrides must be changed through \
                 `POST /api/v1/admin/users/{cid}/access`.",
            ))
            .additional_properties(Some(AdditionalProperties::FreeForm(false)))
            .property(
                "preferred_name",
                ObjectBuilder::new()
                    .schema_type(nullable_string.clone())
                    .description(Some(
                        "Preferred display label for the user. Use null to clear.",
                    )),
            )
            .property(
                "timezone",
                ObjectBuilder::new()
                    .schema_type(nullable_string)
                    .description(Some("IANA timezone name such as `America/Chicago`.")),
            )
            .property(
                "bio",
                ObjectBuilder::new()
                    .schema_type(
                        [Type::String, Type::Null]
                            .into_iter()
                            .collect::<SchemaType>(),
                    )
                    .description(Some("Profile bio. Use null to clear.")),
            )
            .into()
    }
}

impl ToSchema for CreateTeamSpeakUidRequest {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("CreateTeamSpeakUidRequest")
    }
}

impl PartialSchema for CreateTeamSpeakUidRequest {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .description(Some(
                "Self-service TeamSpeak UID creation payload. This route only accepts the raw UID.",
            ))
            .additional_properties(Some(AdditionalProperties::FreeForm(false)))
            .property(
                "uid",
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .description(Some(
                        "TeamSpeak unique identifier to link to the current user.",
                    )),
            )
            .required("uid")
            .into()
    }
}

pub const STAFF_POSITIONS: [&str; 15] = [
    "ATM", "DATM", "TA", "EC", "WM", "FE", "ATA", "AEC", "AWM", "AFE", "EP", "TMU", "FC", "INS",
    "MTR",
];

/// Subset of STAFF_POSITIONS that VATUSA's roster API actually reports.
/// The rest (AEC/AWM/AFE/EP/TMU/FC) have no VATUSA equivalent and are
/// always manually assigned — roster sync never touches them.
pub const VATUSA_SYNCED_STAFF_POSITIONS: [&str; 8] =
    ["ATM", "DATM", "TA", "EC", "WM", "FE", "INS", "MTR"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct StaffPositionItem {
    pub position: String,
    pub source: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StaffPositionsResponse {
    pub cid: i64,
    pub positions: Vec<StaffPositionItem>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct StaffPositionHolder {
    pub cid: i64,
    pub display_name: String,
    pub rating: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StaffPositionHoldersResponse {
    pub position: String,
    pub holders: Vec<StaffPositionHolder>,
}

#[cfg(test)]
mod tests {
    use super::PatchMeRequest;

    #[test]
    fn patch_me_rejects_unknown_fields_like_permissions() {
        let payload = serde_json::json!({
            "preferred_name": "Jay",
            "permissions": {
                "users": ["update"]
            }
        });

        assert!(serde_json::from_value::<PatchMeRequest>(payload).is_err());
    }
}
