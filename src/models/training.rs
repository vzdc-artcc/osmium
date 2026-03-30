use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrainingAssignment {
    pub id: String,
    pub student_id: String,
    pub primary_trainer_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrainingAssignmentRequest {
    pub id: String,
    pub student_id: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decided_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrainerReleaseRequest {
    pub id: String,
    pub student_id: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decided_by: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTrainingAssignmentRequest {
    pub student_id: String,
    pub primary_trainer_id: String,
    pub other_trainer_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTrainingAssignmentRequestRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub struct DecideTrainingAssignmentRequestRequest {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTrainerReleaseRequestRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub struct DecideTrainerReleaseRequestRequest {
    pub status: String,
}

