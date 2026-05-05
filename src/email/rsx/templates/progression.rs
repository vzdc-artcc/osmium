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

pub struct ProgressionAssignedTemplate;

impl RsxTemplate for ProgressionAssignedTemplate {
    fn id(&self) -> &'static str {
        "progression.assigned"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let controller_name = required_string(payload, "controller_name")?;
        let progression_name = required_string(payload, "progression_name")?;
        let details_url = optional_string(payload, "details_url");

        let subject = format!("Training progression assigned: {progression_name}");

        let body = html! {
            p {
                "You have been assigned to a new training progression."
            }
            (callout(html! {
                p { strong { "Progression:" } " " (progression_name) }
            }))
            p {
                "Please review your training requirements and schedule sessions with your trainer."
            }
        };

        let cta = details_url.as_deref().map(|url| ("View progression", url));

        let html = EmailLayout::new(&subject)
            .preheader(&format!("{controller_name} assigned to {progression_name}"))
            .heading("Progression Assigned")
            .unsubscribe_link(unsubscribe_link)
            .render(body, cta)
            .into_string();

        let mut text = TextBuilder::new()
            .line("You have been assigned to a new training progression.")
            .blank()
            .line(&format!("Progression: {progression_name}"))
            .blank()
            .line("Please review your training requirements and schedule sessions with your trainer.");

        if let Some(url) = details_url.as_deref() {
            text = text.link("View progression", url);
        }

        let text = text.optional_unsubscribe(unsubscribe_link).build();

        Ok(RenderedEmail { subject, html, text })
    }
}

pub struct ProgressionRemovedTemplate;

impl RsxTemplate for ProgressionRemovedTemplate {
    fn id(&self) -> &'static str {
        "progression.removed"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let _controller_name = required_string(payload, "controller_name")?;
        let progression_name = required_string(payload, "progression_name")?;
        let reason = optional_string(payload, "reason");

        let subject = format!("Training progression removed: {progression_name}");

        let body = html! {
            p {
                "You have been removed from a training progression."
            }
            (callout(html! {
                p { strong { "Progression:" } " " (progression_name) }
                @if let Some(ref r) = reason {
                    p { strong { "Reason:" } " " (r) }
                }
            }))
            p {
                "If you have questions about this change, please contact your training staff."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader(&format!("Removed from {progression_name}"))
            .heading("Progression Removed")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let mut text = TextBuilder::new()
            .line("You have been removed from a training progression.")
            .blank()
            .line(&format!("Progression: {progression_name}"));

        if let Some(ref r) = reason {
            text = text.line(&format!("Reason: {r}"));
        }

        let text = text
            .blank()
            .line("If you have questions about this change, please contact your training staff.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}
