use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct ListUsersQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
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
}

#[derive(Serialize, sqlx::FromRow, ToSchema)]
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

#[derive(Serialize, ToSchema)]
pub struct UserBasicInfo {
    pub cid: i64,
    pub name: String,
    pub rating: Option<String>,
}

#[derive(Serialize, ToSchema)]
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
}

#[derive(Serialize, ToSchema)]
pub struct UserListItem {
    pub basic: UserBasicInfo,
    pub full: Option<UserPrivateInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct UserDetailsResponse {
    pub basic: UserBasicInfo,
    pub full: Option<UserFullInfo>,
}

#[derive(Serialize, ToSchema)]
pub struct UserFullInfo {
    pub profile: UserPrivateInfo,
    pub roles: Vec<String>,
    pub permissions: BTreeMap<String, Vec<String>>,
    pub stats: UserStats,
}

#[derive(Serialize, ToSchema)]
pub struct UserOverviewBody {
    pub user: AdminUserListItem,
    pub roles: Vec<String>,
    pub permissions: BTreeMap<String, Vec<String>>,
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

#[derive(Deserialize, ToSchema)]
pub struct UserFeedbackQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
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
