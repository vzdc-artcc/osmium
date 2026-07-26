use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct AssignmentTrainerSummary {
    pub id: String,
    pub cid: i64,
    pub name: String,
}

/// Query for the training-statistics bundle. `month` is 0-based (0 = January),
/// matching the website's selector; omit it for a full-year view. `cid` scopes
/// every metric to that instructor; omit it for facility-wide stats.
#[derive(Debug, Clone, Deserialize, IntoParams)]
pub struct TrainingStatsQuery {
    pub year: i32,
    pub month: Option<i32>,
    pub cid: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingStatsMostRunLesson {
    pub identifier: Option<String>,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct TrainingStatsTopTrainer {
    pub id: String,
    pub cid: Option<i64>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub preferred_name: Option<String>,
    pub hours: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingStatsMonthlyBucket {
    /// Short month name, e.g. `"Jan"`.
    pub month: String,
    pub sessions: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct TrainingStatsLessonDistribution {
    pub lesson: String,
    pub passed: i64,
    pub failed: i64,
}

/// Aggregated training-session metrics for one scope (facility or trainer,
/// month or year). `top_trainers` is populated only for facility scope;
/// `monthly_sessions` only for a full-year scope. Pass rate is derived
/// client-side from `passed`/`failed`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingStatsBundle {
    pub sessions: i64,
    pub total_hours: f64,
    pub passed: i64,
    pub failed: i64,
    pub most_run_lesson: TrainingStatsMostRunLesson,
    pub top_trainers: Vec<TrainingStatsTopTrainer>,
    pub monthly_sessions: Vec<TrainingStatsMonthlyBucket>,
    pub lesson_distribution: Vec<TrainingStatsLessonDistribution>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingStatsAllTimeHours {
    pub hours: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingAssignment {
    pub id: String,
    pub student_id: String,
    pub student_cid: i64,
    pub student_name: String,
    pub student_controller_status: String,
    pub primary_trainer_id: String,
    pub primary_trainer_cid: i64,
    pub primary_trainer_name: String,
    pub other_trainer_ids: Vec<String>,
    pub other_trainers: Vec<AssignmentTrainerSummary>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateTrainingAssignmentRequest {
    pub primary_trainer_id: Option<String>,
    pub other_trainer_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingAssignmentRequest {
    pub id: String,
    pub student_id: String,
    pub student_cid: i64,
    pub student_name: String,
    pub student_controller_status: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decided_by: Option<String>,
    pub interested_trainers: Vec<AssignmentTrainerSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainerReleaseRequest {
    pub id: String,
    pub student_id: String,
    pub student_cid: i64,
    pub student_name: String,
    pub student_controller_status: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub status: String,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decided_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingAssignmentListResponse {
    pub items: Vec<TrainingAssignment>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OtsRecommendationListResponse {
    pub items: Vec<OtsRecommendationSummary>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingLessonListResponse {
    pub items: Vec<TrainingLesson>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingAssignmentRequestListResponse {
    pub items: Vec<TrainingAssignmentRequest>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainerReleaseRequestListResponse {
    pub items: Vec<TrainerReleaseRequest>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingAssignmentRequest {
    pub student_id: String,
    pub primary_trainer_id: String,
    pub other_trainer_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingAssignmentRequestRequest {
    /// Omit to submit for the current user (the common self-request case). Set to submit a
    /// manual/backdated request on another student's behalf — requires
    /// `training.assignment_requests.create`, not just the self-request permission.
    pub student_id: Option<String>,
    /// Omit to use the current time. Set to backdate a request that was actually made through
    /// another channel (e.g. Discord) before it was logged here.
    pub submitted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DecideTrainingAssignmentRequestRequest {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainerReleaseRequestRequest {
    /// Omit to submit for the current user (the common self-request case). Set to submit a
    /// release request on another student's behalf (e.g. a trainer releasing their own
    /// assigned student) — requires `training.release_requests.create`, not just the
    /// self-request permission.
    pub student_id: Option<String>,
}

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
pub struct CreateLessonRubricCriteriaRequest {
    pub criteria: String,
    pub description: String,
    pub max_points: i32,
    pub passing: i32,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateLessonRubricCriteriaRequest {
    pub criteria: String,
    pub description: String,
    pub max_points: i32,
    pub passing: i32,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateLessonRubricCellRequest {
    pub points: i32,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateLessonRubricCellRequest {
    pub points: i32,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct LessonRubricCellDetail {
    pub id: String,
    pub criteria_id: String,
    pub points: i32,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LessonRubricCriteriaDetail {
    pub id: String,
    pub rubric_id: String,
    pub criteria: String,
    pub description: String,
    pub passing: i32,
    pub max_points: i32,
    pub sort_order: i32,
    pub cells: Vec<LessonRubricCellDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LessonRubricDetail {
    pub id: String,
    pub lesson_id: String,
    pub criteria: Vec<LessonRubricCriteriaDetail>,
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
    #[serde(default)]
    pub additional_trainers: Vec<AdditionalTrainerRequest>,
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
    #[serde(default)]
    pub additional_trainers: Vec<AdditionalTrainerRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTrainingAppointmentRequest {
    pub student_id: String,
    pub start: chrono::DateTime<chrono::Utc>,
    pub lesson_ids: Vec<String>,
    pub environment: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub additional_trainers: Vec<AdditionalTrainerRequest>,
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
    pub notes: Option<String>,
    #[serde(default)]
    pub additional_trainers: Vec<AdditionalTrainerRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdditionalTrainerRequest {
    pub trainer_id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct AdditionalTrainerDetail {
    pub trainer_id: String,
    pub trainer_cid: i64,
    pub trainer_name: String,
    pub description: String,
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
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub instructor_only: bool,
    pub notify_instructor_on_pass: bool,
    pub release_request_on_pass: bool,
    pub duration: i32,
    pub trainee_preparation: Option<String>,
    pub performance_indicator_template_id: Option<String>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingSessionListItem {
    pub id: String,
    pub student_id: String,
    pub instructor_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub start: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub end: chrono::DateTime<chrono::Utc>,
    pub additional_comments: Option<String>,
    pub trainer_comments: Option<String>,
    pub vatusa_id: Option<String>,
    pub enable_markdown: bool,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub instructor_cid: i64,
    pub instructor_name: String,
    pub ticket_count: i64,
    pub tickets: Vec<TrainingSessionTicketSummary>,
    pub additional_trainer_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingSessionTicketSummary {
    pub id: String,
    pub lesson_id: String,
    pub lesson_identifier: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingSessionDetail {
    pub id: String,
    pub student_id: String,
    pub instructor_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub start: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub end: chrono::DateTime<chrono::Utc>,
    pub additional_comments: Option<String>,
    pub trainer_comments: Option<String>,
    pub vatusa_id: Option<String>,
    pub enable_markdown: bool,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub instructor_cid: i64,
    pub instructor_name: String,
    pub tickets: Vec<TrainingTicketDetail>,
    pub performance_indicator: Option<TrainingSessionPerformanceIndicatorDetail>,
    pub additional_trainers: Vec<AdditionalTrainerDetail>,
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
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TrainingSessionListResponse {
    pub items: Vec<TrainingSessionListItem>,
    #[serde(flatten)]
    pub pagination: crate::models::PaginationMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct TrainingAppointmentListItem {
    pub id: String,
    pub student_id: String,
    pub trainer_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub start: chrono::DateTime<chrono::Utc>,
    pub environment: Option<String>,
    pub double_booking: bool,
    pub preparation_completed: bool,
    pub warning_email_sent: bool,
    pub atc_booking_id: Option<String>,
    pub notes: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub trainer_cid: i64,
    pub trainer_name: String,
    pub lesson_count: i64,
    pub lessons: Vec<TrainingAppointmentLessonSummary>,
    pub additional_trainer_count: i64,
    pub additional_trainers: Vec<AdditionalTrainerDetail>,
    pub estimated_duration_minutes: Option<i64>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub estimated_end: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingAppointmentDetail {
    pub id: String,
    pub student_id: String,
    pub trainer_id: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub start: chrono::DateTime<chrono::Utc>,
    pub environment: Option<String>,
    pub double_booking: bool,
    pub preparation_completed: bool,
    pub warning_email_sent: bool,
    pub atc_booking_id: Option<String>,
    pub notes: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub student_cid: i64,
    pub student_name: String,
    pub trainer_cid: i64,
    pub trainer_name: String,
    pub estimated_duration_minutes: Option<i64>,
    #[serde(serialize_with = "crate::time::serialize_optional_datetime")]
    pub estimated_end: Option<chrono::DateTime<chrono::Utc>>,
    pub lessons: Vec<TrainingAppointmentLessonSummary>,
    pub additional_trainers: Vec<AdditionalTrainerDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrainingTicketDetail {
    pub id: String,
    pub session_id: String,
    pub lesson_id: String,
    pub passed: bool,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OtsRecommendationSummary {
    pub id: String,
    pub student_id: String,
    pub student_cid: i64,
    pub student_name: String,
    pub assigned_instructor_id: Option<String>,
    pub assigned_instructor_cid: Option<i64>,
    pub assigned_instructor_name: Option<String>,
    pub notes: String,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "crate::time::serialize_datetime")]
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
