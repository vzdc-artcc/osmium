use axum::Json;

use crate::{
    captcha,
    errors::ApiError,
    models::{VerifyCaptchaRequest, VerifyCaptchaResponse},
};

#[utoipa::path(
    post,
    path = "/api/v1/captcha/verify",
    tag = "captcha",
    request_body = VerifyCaptchaRequest,
    responses(
        (status = 200, description = "Captcha verification result", body = VerifyCaptchaResponse),
        (status = 503, description = "Captcha verification unavailable")
    )
)]
pub async fn verify_captcha(
    Json(request): Json<VerifyCaptchaRequest>,
) -> Result<Json<VerifyCaptchaResponse>, ApiError> {
    Ok(Json(captcha::verify_captcha(&request.token).await?))
}
