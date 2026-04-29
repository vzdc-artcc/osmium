use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct PublicationCategory {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Publication {
    pub id: String,
    pub category_id: String,
    pub category_key: String,
    pub category_name: String,
    pub title: String,
    pub description: Option<String>,
    pub effective_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub file_id: String,
    pub cdn_url: String,
    pub file_filename: String,
    pub file_content_type: String,
    pub file_size_bytes: i64,
    pub is_public: bool,
    pub sort_order: i32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePublicationRequest {
    pub category_id: String,
    pub title: String,
    pub description: Option<String>,
    pub effective_at: chrono::DateTime<chrono::Utc>,
    pub file_id: String,
    pub is_public: bool,
    pub sort_order: Option<i32>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdatePublicationRequest {
    pub category_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub effective_at: Option<chrono::DateTime<chrono::Utc>>,
    pub file_id: Option<String>,
    pub is_public: Option<bool>,
    pub sort_order: Option<i32>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePublicationCategoryRequest {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdatePublicationCategoryRequest {
    pub key: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
}
