use serde::Deserialize;

use crate::{errors::ApiError, models::VerifyCaptchaResponse};

const SITEVERIFY_URL: &str = "https://www.google.com/recaptcha/api/siteverify";

#[derive(Debug, Deserialize)]
struct SiteverifyResponse {
    success: bool,
    score: Option<f64>,
}

/// Verifies a client-supplied reCAPTCHA token against Google's siteverify
/// endpoint, using a server-held secret that must never reach the client.
/// Mirrors the live site's `actions/captcha.ts`: a thin proxy, no local
/// state, no binding to the request that follows it.
pub async fn verify_captcha(token: &str) -> Result<VerifyCaptchaResponse, ApiError> {
    let secret = std::env::var("GOOGLE_CAPTCHA_SECRET_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ApiError::ServiceUnavailable)?;

    let token = token.trim();
    if token.is_empty() {
        return Ok(VerifyCaptchaResponse {
            success: false,
            score: None,
        });
    }

    let client = reqwest::Client::new();
    let form = [("secret", secret.as_str()), ("response", token)];

    let response = client
        .post(SITEVERIFY_URL)
        .form(&form)
        .send()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(ApiError::ServiceUnavailable);
    }

    let body: SiteverifyResponse = response
        .json()
        .await
        .map_err(|_| ApiError::ServiceUnavailable)?;

    Ok(VerifyCaptchaResponse {
        success: body.success,
        score: body.score,
    })
}
