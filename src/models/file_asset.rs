use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FileAsset {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub etag: String,
    pub is_public: bool,
    pub uploaded_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UploadFileQuery {
    pub filename: Option<String>,
    pub public: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListFilesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFileMetadataRequest {
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub is_public: Option<bool>,
}

