use maud::html;
use serde_json::Value;

use crate::email::rsx::components::{EmailLayout, callout};
use crate::email::rsx::text::TextBuilder;
use crate::email::templates::RenderedEmail;
use crate::errors::ApiError;

use super::RsxTemplate;

fn required_string(payload: &Value, key: &str) -> Result<String, ApiError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .ok_or(ApiError::BadRequest)
}

fn optional_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

pub struct SystemTestTemplate;

impl RsxTemplate for SystemTestTemplate {
    fn id(&self) -> &'static str {
        "system.test_email"
    }

    fn render(
        &self,
        payload: &Value,
        _unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let message = required_string(payload, "message")?;
        let requested_by = optional_string(payload, "requested_by");

        let subject = "Osmium email transport test".to_string();

        let body = html! {
            p { (message) }
            @if let Some(ref by) = requested_by {
                (callout(html! {
                    p { strong { "Requested by:" } " " (by) }
                }))
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader("Diagnostic SES connectivity email")
            .heading("Transport test")
            .render(body, None)
            .into_string();

        let text = TextBuilder::new()
            .line(&message)
            .optional_section("Requested by", requested_by.as_deref())
            .build();

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}
