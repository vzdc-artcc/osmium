use std::time::Instant;

use axum::{
    body::{Body, to_bytes},
    extract::{MatchedPath, Request, State},
    http::{HeaderMap, HeaderValue, header},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

use crate::auth::context::{CurrentServiceAccount, CurrentUser};

const MAX_PREVIEW_BYTES: usize = 8 * 1024;
const REDACTED: &str = "[REDACTED]";
const SENSITIVE_KEY_FRAGMENTS: &[&str] = &[
    "authorization",
    "cookie",
    "token",
    "secret",
    "password",
    "api_key",
    "apikey",
    "key",
    "session",
    "sig",
    "signature",
    "code",
    "state",
];

pub async fn log_requests(
    State(_state): State<crate::state::AppState>,
    request: Request,
    next: Next,
) -> Response {
    let started_at = Instant::now();
    let request_id = request_id(request.headers());
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let matched_path = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or(&path)
        .to_string();
    let sanitized_query = request.uri().query().map(sanitize_query);
    let content_type = header_value(request.headers(), header::CONTENT_TYPE);
    let content_length = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    let client_ip = client_ip(request.headers()).unwrap_or_else(|| "unknown".to_string());
    let actor = actor_summary(request.extensions());
    let auth_mode = auth_mode(request.extensions());

    let (request, request_body_preview) =
        preview_request_body(request, content_type.as_deref(), content_length).await;
    let mut response = next.run(request).await;

    response.headers_mut().insert(
        header::HeaderName::from_static("x-request-id"),
        HeaderValue::from_str(&request_id).unwrap_or_else(|_| HeaderValue::from_static("invalid")),
    );

    let status = response.status().as_u16();
    let response_content_type = header_value(response.headers(), header::CONTENT_TYPE);
    let response_content_length = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    let (response, response_body_preview) = preview_response_body(
        response,
        response_content_type.as_deref(),
        response_content_length,
    )
    .await;

    let outcome = if status >= 500 {
        "server_error"
    } else if status >= 400 {
        "client_error"
    } else {
        "success"
    };

    tracing::info!(
        request_id = %request_id,
        method = %method,
        matched_path = %matched_path,
        path = %path,
        query = sanitized_query.as_deref().unwrap_or(""),
        status,
        latency_ms = started_at.elapsed().as_millis() as u64,
        content_type = content_type.as_deref().unwrap_or(""),
        content_length = content_length.unwrap_or_default() as u64,
        response_content_type = response_content_type.as_deref().unwrap_or(""),
        response_content_length = response_content_length.unwrap_or_default() as u64,
        client_ip = %client_ip,
        actor_type = actor.actor_type.as_deref().unwrap_or(""),
        actor_id = actor.actor_id.as_deref().unwrap_or(""),
        actor_label = actor.actor_label.as_deref().unwrap_or(""),
        auth_mode = %auth_mode,
        request_body_preview = %request_body_preview,
        response_body_preview = %response_body_preview,
        outcome = %outcome,
        "http_request",
    );

    response
}

struct ActorSummary {
    actor_type: Option<String>,
    actor_id: Option<String>,
    actor_label: Option<String>,
}

async fn preview_request_body(
    request: Request,
    content_type: Option<&str>,
    content_length: Option<usize>,
) -> (Request, String) {
    let Some(content_type) = content_type else {
        return (request, String::new());
    };

    if !is_textual_content_type(content_type) {
        let preview = metadata_preview(content_type, content_length);
        return (request, preview);
    }

    let Some(content_length) = content_length else {
        return (request, String::new());
    };

    if content_length > MAX_PREVIEW_BYTES {
        return (
            request,
            format!("truncated textual body omitted ({} bytes)", content_length),
        );
    }

    let (parts, body) = request.into_parts();
    match to_bytes(body, MAX_PREVIEW_BYTES).await {
        Ok(bytes) => {
            let preview = sanitize_text_payload(&String::from_utf8_lossy(&bytes));
            (
                Request::from_parts(parts, Body::from(bytes)),
                truncate_preview(preview),
            )
        }
        Err(_) => (
            Request::from_parts(parts, Body::empty()),
            "[unavailable]".to_string(),
        ),
    }
}

async fn preview_response_body(
    response: Response,
    content_type: Option<&str>,
    content_length: Option<usize>,
) -> (Response, String) {
    if response.status().is_redirection() {
        let preview = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(sanitize_text_payload)
            .unwrap_or_default();
        return (response, preview);
    }

    let Some(content_type) = content_type else {
        return (response, String::new());
    };

    if !is_textual_content_type(content_type) || is_attachment(response.headers()) {
        let preview = metadata_preview(content_type, content_length);
        return (response, preview);
    }

    let Some(content_length) = content_length else {
        return (response, String::new());
    };

    if content_length > MAX_PREVIEW_BYTES {
        return (
            response,
            format!("truncated textual body omitted ({} bytes)", content_length),
        );
    }

    let (parts, body) = response.into_parts();
    match to_bytes(body, MAX_PREVIEW_BYTES).await {
        Ok(bytes) => {
            let preview = sanitize_text_payload(&String::from_utf8_lossy(&bytes));
            (
                Response::from_parts(parts, Body::from(bytes)),
                truncate_preview(preview),
            )
        }
        Err(_) => (
            Response::from_parts(parts, Body::empty()),
            "[unavailable]".to_string(),
        ),
    }
}

fn metadata_preview(content_type: &str, content_length: Option<usize>) -> String {
    match content_length {
        Some(length) => format!("binary body omitted ({content_type}, {length} bytes)"),
        None => format!("binary body omitted ({content_type})"),
    }
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn header_value(headers: &HeaderMap, key: header::HeaderName) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

fn client_ip(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    {
        if let Some(first) = value.split(',').next() {
            let parsed = first.trim();
            if !parsed.is_empty() {
                return Some(parsed.to_string());
            }
        }
    }

    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn sanitize_query(query: &str) -> String {
    query
        .split('&')
        .map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default();
            if is_sensitive_key(key) {
                format!("{key}={REDACTED}")
            } else {
                format!("{key}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn sanitize_text_payload(payload: &str) -> String {
    let mut sanitized = payload.to_string();
    for fragment in SENSITIVE_KEY_FRAGMENTS {
        sanitized = redact_fragment(&sanitized, fragment);
    }
    sanitized
}

fn redact_fragment(payload: &str, fragment: &str) -> String {
    let patterns = [
        format!("\"{fragment}\":\""),
        format!("\"{fragment}\": \""),
        format!("{fragment}="),
        format!("{fragment}:"),
    ];

    let mut output = payload.to_string();
    for pattern in patterns {
        let pattern_lower = pattern.to_ascii_lowercase();
        let mut cursor = 0;
        let mut rebuilt = String::with_capacity(output.len());
        let lowercase = output.to_ascii_lowercase();

        while let Some(relative_index) = lowercase[cursor..].find(&pattern_lower) {
            let index = cursor + relative_index;
            let start = index + pattern.len();
            let end = output[start..]
                .find(|character: char| ['"', '&', ',', '\n', '\r', ' '].contains(&character))
                .map(|offset| start + offset)
                .unwrap_or(output.len());

            rebuilt.push_str(&output[cursor..start]);
            rebuilt.push_str(REDACTED);
            cursor = end;
        }
        rebuilt.push_str(&output[cursor..]);
        output = rebuilt;
    }
    output
}

fn truncate_preview(preview: String) -> String {
    if preview.len() <= MAX_PREVIEW_BYTES {
        preview
    } else {
        format!("{}...[truncated]", &preview[..MAX_PREVIEW_BYTES])
    }
}

fn actor_summary(extensions: &http::Extensions) -> ActorSummary {
    if let Some(Some(user)) = extensions.get::<Option<CurrentUser>>() {
        return ActorSummary {
            actor_type: Some("user".to_string()),
            actor_id: Some(user.id.clone()),
            actor_label: Some(format!("{} ({})", user.display_name, user.cid)),
        };
    }

    if let Some(Some(service_account)) = extensions.get::<Option<CurrentServiceAccount>>() {
        return ActorSummary {
            actor_type: Some("service_account".to_string()),
            actor_id: Some(service_account.id.clone()),
            actor_label: Some(service_account.name.clone()),
        };
    }

    ActorSummary {
        actor_type: None,
        actor_id: None,
        actor_label: None,
    }
}

fn auth_mode(extensions: &http::Extensions) -> &'static str {
    if matches!(extensions.get::<Option<CurrentUser>>(), Some(Some(_))) {
        "user_session"
    } else if matches!(
        extensions.get::<Option<CurrentServiceAccount>>(),
        Some(Some(_))
    ) {
        "service_account"
    } else {
        "none"
    }
}

fn is_attachment(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("attachment"))
        .unwrap_or(false)
}

fn is_textual_content_type(content_type: &str) -> bool {
    let normalized = content_type.to_ascii_lowercase();
    normalized.starts_with("text/")
        || normalized.contains("application/json")
        || normalized.contains("application/problem+json")
        || normalized.contains("application/x-www-form-urlencoded")
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    SENSITIVE_KEY_FRAGMENTS
        .iter()
        .any(|fragment| normalized.contains(fragment))
}

#[cfg(test)]
mod tests {
    use super::{sanitize_query, sanitize_text_payload};

    #[test]
    fn redacts_sensitive_query_values() {
        assert_eq!(
            sanitize_query("page=1&token=abc&sig=123"),
            "page=1&token=[REDACTED]&sig=[REDACTED]"
        );
    }

    #[test]
    fn redacts_sensitive_text_values() {
        let payload = r#"{"safe":"ok","token":"abc","nested":"session=123"}"#;
        let sanitized = sanitize_text_payload(payload);
        assert!(sanitized.contains(r#""token":"[REDACTED]""#));
        assert!(sanitized.contains("session=[REDACTED]"));
    }
}
