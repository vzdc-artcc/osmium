use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct VerifyCaptchaRequest {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VerifyCaptchaResponse {
    pub success: bool,
    pub score: Option<f64>,
}
