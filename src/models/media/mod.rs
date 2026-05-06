use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct FileAsset {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub etag: String,
    pub is_public: bool,
    pub uploaded_by: String,
    pub owner_user_id: Option<String>,
    pub viewer_roles: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UploadFileQuery {
    pub filename: Option<String>,
    pub public: Option<bool>,
    pub owner_cid: Option<i64>,
    pub viewer_cids: Option<String>,
    pub viewer_roles: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ListFilesQuery {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFileMetadataRequest {
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub is_public: Option<bool>,
    pub owner_cid: Option<i64>,
    pub viewer_cids: Option<Vec<i64>>,
    pub viewer_roles: Option<Vec<String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FileAssetListResponse {
    pub items: Vec<FileAsset>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}
