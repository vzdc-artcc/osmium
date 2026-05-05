use maud::html;
use serde_json::Value;

use crate::email::rsx::components::{callout, EmailLayout};
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

pub struct NewFeedbackTemplate;

impl RsxTemplate for NewFeedbackTemplate {
    fn id(&self) -> &'static str {
        "feedback.new"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let controller_name = required_string(payload, "controller_name")?;
        let position = optional_string(payload, "position");
        let rating_text = optional_string(payload, "rating");
        let details_url = optional_string(payload, "details_url");

        let subject = "New feedback received".to_string();

        let body = html! {
            p {
                "You have received new feedback for your controlling."
            }
            (callout(html! {
                @if let Some(ref pos) = position {
                    p { strong { "Position:" } " " (pos) }
                }
                @if let Some(ref rating) = rating_text {
                    p { strong { "Rating:" } " " (rating) }
                }
            }))
            p {
                "This feedback has been shared with you to help you improve. "
                "If you have questions, please contact your training staff."
            }
        };

        let cta = details_url.as_deref().map(|url| ("View feedback", url));

        let html = EmailLayout::new(&subject)
            .preheader(&format!("{controller_name} received new feedback"))
            .heading("New Feedback")
            .unsubscribe_link(unsubscribe_link)
            .render(body, cta)
            .into_string();

        let mut text = TextBuilder::new()
            .line("You have received new feedback for your controlling.");

        if let Some(ref pos) = position {
            text = text.line(&format!("Position: {pos}"));
        }

        if let Some(ref rating) = rating_text {
            text = text.line(&format!("Rating: {rating}"));
        }

        if let Some(url) = details_url.as_deref() {
            text = text.blank().link("View feedback", url);
        }

        let text = text.optional_unsubscribe(unsubscribe_link).build();

        Ok(RenderedEmail { subject, html, text })
    }
}
