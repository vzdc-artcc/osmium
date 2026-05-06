use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingAssignment {
    pub id: String,
    pub student_id: String,
    pub primary_trainer_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingAssignmentRequest {
    pub id: String,
    pub student_id: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decided_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainerReleaseRequest {
    pub id: String,
    pub student_id: String,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decided_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingAssignmentListResponse {
    pub items: Vec<TrainingAssignment>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OtsRecommendationListResponse {
    pub items: Vec<OtsRecommendationSummary>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingLessonListResponse {
    pub items: Vec<TrainingLesson>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingAssignmentRequestListResponse {
    pub items: Vec<TrainingAssignmentRequest>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainerReleaseRequestListResponse {
    pub items: Vec<TrainerReleaseRequest>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingAssignmentRequest {
    pub student_id: String,
    pub primary_trainer_id: String,
    pub other_trainer_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingAssignmentRequestRequest {}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DecideTrainingAssignmentRequestRequest {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainerReleaseRequestRequest {}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DecideTrainerReleaseRequestRequest {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateOtsRecommendationRequest {
    pub student_id: String,
    pub notes: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateOtsRecommendationRequest {
    pub assigned_instructor_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct ListTrainingSessionsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub sort_field: Option<String>,
    pub sort_order: Option<String>,
    pub filter_field: Option<String>,
    pub filter_operator: Option<String>,
    pub filter_value: Option<String>,
    pub student_id: Option<String>,
    pub instructor_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, IntoParams, ToSchema)]
pub struct ListTrainingAppointmentsQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub sort_field: Option<String>,
    pub sort_order: Option<String>,
    pub trainer_id: Option<String>,
    pub student_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingLessonRequest {
    pub identifier: String,
    pub location: i32,
    pub name: String,
    pub description: String,
    pub position: String,
    pub facility: String,
    pub duration: i32,
    pub trainee_preparation: Option<String>,
    pub instructor_only: bool,
    pub notify_instructor_on_pass: bool,
    pub release_request_on_pass: bool,
    pub performance_indicator_template_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateTrainingLessonRequest {
    pub identifier: String,
    pub location: i32,
    pub name: String,
    pub description: String,
    pub position: String,
    pub facility: String,
    pub duration: i32,
    pub trainee_preparation: Option<String>,
    pub instructor_only: bool,
    pub notify_instructor_on_pass: bool,
    pub release_request_on_pass: bool,
    pub performance_indicator_template_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingSessionRequest {
    pub student_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    pub additional_comments: Option<String>,
    pub trainer_comments: Option<String>,
    pub enable_markdown: Option<bool>,
    pub tickets: Vec<CreateTrainingTicketRequest>,
    pub performance_indicator: Option<CreateTrainingSessionPerformanceIndicatorRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateTrainingSessionRequest {
    pub student_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    pub additional_comments: Option<String>,
    pub trainer_comments: Option<String>,
    pub enable_markdown: Option<bool>,
    pub tickets: Vec<CreateTrainingTicketRequest>,
    pub performance_indicator: Option<CreateTrainingSessionPerformanceIndicatorRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingAppointmentRequest {
    pub student_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub lesson_ids: Vec<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateTrainingAppointmentRequest {
    pub student_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub lesson_ids: Vec<String>,
    pub environment: Option<String>,
    pub double_booking: Option<bool>,
    pub preparation_completed: Option<bool>,
    pub warning_email_sent: Option<bool>,
    pub atc_booking_id: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingTicketRequest {
    pub lesson_id: String,
    pub passed: bool,
    pub scores: Vec<CreateRubricScoreRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateRubricScoreRequest {
    pub criteria_id: String,
    pub cell_id: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingSessionPerformanceIndicatorRequest {
    pub categories: Vec<CreateTrainingSessionPerformanceIndicatorCategoryRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingSessionPerformanceIndicatorCategoryRequest {
    pub name: String,
    pub order: i32,
    pub criteria: Vec<CreateTrainingSessionPerformanceIndicatorCriteriaRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingSessionPerformanceIndicatorCriteriaRequest {
    pub name: String,
    pub order: i32,
    pub marker: String,
    pub comments: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiMessage {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingLesson {
    pub id: String,
    pub identifier: String,
    pub location: i32,
    pub name: String,
    pub description: String,
    pub position: String,
    pub facility: String,
    pub rubric_id: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub instructor_only: bool,
    pub notify_instructor_on_pass: bool,
    pub release_request_on_pass: bool,
    pub duration: i32,
    pub trainee_preparation: Option<String>,
    pub performance_indicator_template_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingSessionListItem {
    pub id: String,
    pub student_id: String,
    pub instructor_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    pub additional_comments: Option<String>,
    pub trainer_comments: Option<String>,
    pub vatusa_id: Option<String>,
    pub enable_markdown: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub instructor_cid: i64,
    pub instructor_name: String,
    pub ticket_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingSessionDetail {
    pub id: String,
    pub student_id: String,
    pub instructor_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    pub additional_comments: Option<String>,
    pub trainer_comments: Option<String>,
    pub vatusa_id: Option<String>,
    pub enable_markdown: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub instructor_cid: i64,
    pub instructor_name: String,
    pub tickets: Vec<TrainingTicketDetail>,
    pub performance_indicator: Option<TrainingSessionPerformanceIndicatorDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingAppointmentLessonSummary {
    pub id: String,
    pub identifier: String,
    pub name: String,
    pub location: i32,
    pub duration: i32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingAppointmentListResponse {
    pub items: Vec<TrainingAppointmentListItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingSessionListResponse {
    pub items: Vec<TrainingSessionListItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingAppointmentListItem {
    pub id: String,
    pub student_id: String,
    pub trainer_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub environment: Option<String>,
    pub double_booking: bool,
    pub preparation_completed: bool,
    pub warning_email_sent: bool,
    pub atc_booking_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub trainer_cid: i64,
    pub trainer_name: String,
    pub lesson_count: i64,
    pub estimated_duration_minutes: Option<i64>,
    pub estimated_end: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingAppointmentDetail {
    pub id: String,
    pub student_id: String,
    pub trainer_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub environment: Option<String>,
    pub double_booking: bool,
    pub preparation_completed: bool,
    pub warning_email_sent: bool,
    pub atc_booking_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub trainer_cid: i64,
    pub trainer_name: String,
    pub estimated_duration_minutes: Option<i64>,
    pub estimated_end: Option<chrono::DateTime<chrono::Utc>>,
    pub lessons: Vec<TrainingAppointmentLessonSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingTicketDetail {
    pub id: String,
    pub session_id: String,
    pub lesson_id: String,
    pub passed: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub scores: Vec<RubricScoreDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct RubricScoreDetail {
    pub id: String,
    pub criteria_id: String,
    pub cell_id: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingSessionPerformanceIndicatorDetail {
    pub id: String,
    pub categories: Vec<TrainingSessionPerformanceIndicatorCategoryDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingSessionPerformanceIndicatorCategoryDetail {
    pub id: String,
    pub name: String,
    pub order: i32,
    pub criteria: Vec<TrainingSessionPerformanceIndicatorCriteriaDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingSessionPerformanceIndicatorCriteriaDetail {
    pub id: String,
    pub name: String,
    pub order: i32,
    pub marker: Option<String>,
    pub comments: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct LessonRosterChangeSummary {
    pub id: String,
    pub lesson_id: String,
    pub certification_type_id: String,
    pub certification_option: String,
    pub dossier_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct OtsRecommendationSummary {
    pub id: String,
    pub student_id: String,
    pub assigned_instructor_id: Option<String>,
    pub notes: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateOrUpdateTrainingSessionResult {
    pub session: Option<TrainingSessionDetail>,
    pub release: Option<TrainerReleaseRequest>,
    pub roster_updates: Vec<LessonRosterChangeSummary>,
    pub ots_recommendation: Option<OtsRecommendationSummary>,
    pub errors: Vec<ApiMessage>,
}
