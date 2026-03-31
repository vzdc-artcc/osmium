use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeedbackItem {
    pub id: String,
    pub submitter_user_id: String,
    pub target_user_id: String,
    pub pilot_callsign: String,
    pub controller_position: String,
    pub rating: i32,
    pub comments: Option<String>,
    pub staff_comments: Option<String>,
    pub status: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decided_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateFeedbackRequest {
    pub target_cid: i64,
    pub pilot_callsign: String,
    pub controller_position: String,
    pub rating: i32,
    pub comments: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DecideFeedbackRequest {
    pub status: String,
    pub staff_comments: Option<String>,
}

