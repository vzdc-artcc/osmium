use std::{cell::RefCell, future::Future};

use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, header::CONTENT_TYPE, request::Parts},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, SecondsFormat, Utc};
use chrono_tz::Tz;
use serde::Serialize;

use crate::{auth::context::CurrentUser, errors::ApiError};

pub const RESPONSE_TIMEZONE_HEADER: &str = "X-Response-Timezone";

#[derive(Debug, Clone)]
pub enum ResponseTimezoneMode {
    User,
    Zulu,
    Explicit(Tz),
}

#[derive(Debug, Clone)]
pub struct ResponseTimeContext {
    mode: ResponseTimezoneMode,
    user_timezone: Option<String>,
}

impl Default for ResponseTimeContext {
    fn default() -> Self {
        Self {
            mode: ResponseTimezoneMode::User,
            user_timezone: None,
        }
    }
}

impl ResponseTimeContext {
    pub fn from_user(user: Option<&CurrentUser>) -> Self {
        Self {
            mode: ResponseTimezoneMode::User,
            user_timezone: user.map(|user| user.timezone.clone()),
        }
    }

    pub fn parse(header_value: Option<&str>, user: Option<&CurrentUser>) -> Result<Self, ApiError> {
        let mut context = Self::from_user(user);

        let Some(value) = header_value
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(context);
        };

        context.mode = if value.eq_ignore_ascii_case("user") {
            ResponseTimezoneMode::User
        } else if value.eq_ignore_ascii_case("zulu") {
            ResponseTimezoneMode::Zulu
        } else {
            ResponseTimezoneMode::Explicit(value.parse::<Tz>().map_err(|_| ApiError::BadRequest)?)
        };

        Ok(context)
    }

    fn resolved_timezone(&self) -> Option<Tz> {
        match self.mode {
            ResponseTimezoneMode::Zulu => None,
            ResponseTimezoneMode::Explicit(timezone) => Some(timezone),
            ResponseTimezoneMode::User => {
                self.user_timezone
                    .as_deref()
                    .and_then(|value| match value.parse::<Tz>() {
                        Ok(timezone) => Some(timezone),
                        Err(error) => {
                            tracing::warn!(
                                timezone = value,
                                ?error,
                                "failed to parse stored user timezone for response rendering"
                            );
                            None
                        }
                    })
            }
        }
    }

    pub fn format_datetime(&self, value: DateTime<Utc>) -> String {
        if let Some(timezone) = self.resolved_timezone() {
            value
                .with_timezone(&timezone)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        } else {
            value.to_rfc3339_opts(SecondsFormat::Secs, true)
        }
    }
}

thread_local! {
    static SERIALIZATION_CONTEXT: RefCell<Option<ResponseTimeContext>> = const { RefCell::new(None) };
}

pub fn format_response_datetime(value: DateTime<Utc>, context: &ResponseTimeContext) -> String {
    context.format_datetime(value)
}

pub fn format_response_optional_datetime(
    value: Option<DateTime<Utc>>,
    context: &ResponseTimeContext,
) -> Option<String> {
    value.map(|value| context.format_datetime(value))
}

pub fn serialize_datetime<S>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let formatted = with_context(|context| context.format_datetime(*value))
        .unwrap_or_else(|| value.to_rfc3339_opts(SecondsFormat::Secs, true));
    serializer.serialize_str(&formatted)
}

pub fn serialize_optional_datetime<S>(
    value: &Option<DateTime<Utc>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(value) => serialize_datetime(value, serializer),
        None => serializer.serialize_none(),
    }
}

fn with_context<T>(f: impl FnOnce(&ResponseTimeContext) -> T) -> Option<T> {
    SERIALIZATION_CONTEXT.with(|slot| slot.borrow().as_ref().map(f))
}

fn with_serialization_context<T>(context: ResponseTimeContext, f: impl FnOnce() -> T) -> T {
    SERIALIZATION_CONTEXT.with(|slot| {
        let previous = slot.replace(Some(context));
        let result = f();
        slot.replace(previous);
        result
    })
}

#[derive(Debug, Clone)]
pub struct ApiJson<T> {
    value: T,
    context: ResponseTimeContext,
}

impl<T> ApiJson<T> {
    pub fn new(value: T, context: ResponseTimeContext) -> Self {
        Self { value, context }
    }
}

impl<T> IntoResponse for ApiJson<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let body = with_serialization_context(self.context, || serde_json::to_vec(&self.value));
        match body {
            Ok(body) => {
                (StatusCode::OK, [(CONTENT_TYPE, "application/json")], body).into_response()
            }
            Err(error) => {
                tracing::error!(?error, "failed to serialize api response");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

impl<T> From<(T, ResponseTimeContext)> for ApiJson<T> {
    fn from((value, context): (T, ResponseTimeContext)) -> Self {
        Self::new(value, context)
    }
}

impl<S> FromRequestParts<S> for ResponseTimeContext
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let header_value = parts
            .headers
            .get(RESPONSE_TIMEZONE_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let current_user = parts
            .extensions
            .get::<Option<CurrentUser>>()
            .cloned()
            .flatten();

        async move { Self::parse(header_value.as_deref(), current_user.as_ref()) }
    }
}

impl<T> From<(Json<T>, ResponseTimeContext)> for ApiJson<T> {
    fn from((Json(value), context): (Json<T>, ResponseTimeContext)) -> Self {
        Self::new(value, context)
    }
}

#[cfg(test)]
mod tests {
    use super::{RESPONSE_TIMEZONE_HEADER, ResponseTimeContext, serialize_datetime};
    use crate::auth::context::CurrentUser;
    use crate::errors::ApiError;
    use chrono::{TimeZone, Utc};
    use serde::Serialize;

    #[derive(Serialize)]
    struct TimestampBody {
        #[serde(serialize_with = "serialize_datetime")]
        value: chrono::DateTime<Utc>,
    }

    fn current_user(timezone: &str) -> CurrentUser {
        CurrentUser {
            id: "user-1".to_string(),
            cid: 123456,
            email: "user@example.com".to_string(),
            display_name: "User".to_string(),
            timezone: timezone.to_string(),
            rating: None,
            primary_role: None,
        }
    }

    #[test]
    fn defaults_to_user_timezone() {
        let context =
            ResponseTimeContext::parse(None, Some(&current_user("America/Chicago"))).unwrap();
        let value = Utc.with_ymd_and_hms(2026, 5, 20, 14, 0, 0).unwrap();

        assert_eq!(context.format_datetime(value), "2026-05-20T09:00:00-05:00");
    }

    #[test]
    fn falls_back_to_utc_without_user() {
        let context = ResponseTimeContext::parse(None, None).unwrap();
        let value = Utc.with_ymd_and_hms(2026, 5, 20, 14, 0, 0).unwrap();

        assert_eq!(context.format_datetime(value), "2026-05-20T14:00:00Z");
    }

    #[test]
    fn supports_zulu_header() {
        let context =
            ResponseTimeContext::parse(Some("zulu"), Some(&current_user("America/Chicago")))
                .unwrap();
        let value = Utc.with_ymd_and_hms(2026, 5, 20, 14, 0, 0).unwrap();

        assert_eq!(context.format_datetime(value), "2026-05-20T14:00:00Z");
    }

    #[test]
    fn supports_explicit_timezone_header() {
        let context = ResponseTimeContext::parse(Some("America/New_York"), None).unwrap();
        let value = Utc.with_ymd_and_hms(2026, 12, 20, 14, 0, 0).unwrap();

        assert_eq!(context.format_datetime(value), "2026-12-20T09:00:00-05:00");
    }

    #[test]
    fn rejects_invalid_header_values() {
        let error = ResponseTimeContext::parse(Some("Mars/Base"), None).unwrap_err();
        assert!(matches!(error, ApiError::BadRequest));
    }

    #[test]
    fn serializer_uses_rfc3339_offsets() {
        let body = TimestampBody {
            value: Utc.with_ymd_and_hms(2026, 5, 20, 14, 0, 0).unwrap(),
        };
        let context =
            ResponseTimeContext::parse(None, Some(&current_user("America/Chicago"))).unwrap();

        let serialized =
            super::with_serialization_context(context, || serde_json::to_value(&body)).unwrap();

        assert_eq!(serialized["value"], "2026-05-20T09:00:00-05:00");
    }

    #[test]
    fn exposes_canonical_header_name() {
        assert_eq!(RESPONSE_TIMEZONE_HEADER, "X-Response-Timezone");
    }
}
