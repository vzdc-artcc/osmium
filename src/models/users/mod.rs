use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{
    IntoParams, PartialSchema, ToSchema,
    openapi::{
        RefOr,
        schema::{AdditionalProperties, ObjectBuilder, Schema, SchemaType, Type},
    },
};

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ListUsersQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchMeRequest {
    pub preferred_name: Option<Option<String>>,
    pub timezone: Option<String>,
    pub bio: Option<Option<String>>,
    pub receive_event_notifications: Option<bool>,
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
    pub artcc: Option<String>,
    pub rating: Option<String>,
    pub division: Option<String>,
    pub status: Option<String>,
    pub controller_status: Option<String>,
    pub membership_status: Option<String>,
    pub join_date: Option<DateTime<Utc>>,
    pub home_facility: Option<String>,
    pub visitor_home_facility: Option<String>,
    pub is_active: Option<bool>,
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
    pub receive_event_notifications: bool,
    pub operating_initials: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct TeamSpeakUidBody {
    pub id: String,
    pub uid: String,
    pub linked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MeBody {
    pub id: String,
    pub cid: i64,
    pub email: String,
    pub display_name: String,
    pub rating: Option<String>,
    pub server_admin: bool,
    pub permissions: serde_json::Value,
    pub profile: MeProfileBody,
    pub teamspeak_uids: Vec<TeamSpeakUidBody>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserPrivateInfo {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub artcc: Option<String>,
    pub division: Option<String>,
    pub status: Option<String>,
    pub controller_status: Option<String>,
    pub membership_status: Option<String>,
    pub join_date: Option<DateTime<Utc>>,
    pub home_facility: Option<String>,
    pub visitor_home_facility: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserListItem {
    pub basic: UserBasicInfo,
    pub full: Option<UserPrivateInfo>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserListResponse {
    pub items: Vec<UserListItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
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
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
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
pub struct VisitorApplicationItem {
    pub id: String,
    pub user_id: String,
    pub cid: Option<i64>,
    pub display_name: Option<String>,
    pub home_facility: String,
    pub why_visit: String,
    pub status: String,
    pub reason_for_denial: Option<String>,
    pub submitted_at: DateTime<Utc>,
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
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VisitorApplicationListResponse {
    pub items: Vec<VisitorApplicationItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminUserListResponse {
    pub items: Vec<AdminUserListItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
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
        let nullable_bool: SchemaType = [Type::Boolean, Type::Null].into_iter().collect();

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
            .property(
                "receive_event_notifications",
                ObjectBuilder::new()
                    .schema_type(nullable_bool)
                    .description(Some("Whether the user wants new event notifications.")),
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
