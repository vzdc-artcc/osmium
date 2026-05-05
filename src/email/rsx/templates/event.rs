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

pub struct EventPositionPublishedTemplate;

impl RsxTemplate for EventPositionPublishedTemplate {
    fn id(&self) -> &'static str {
        "events.position_published"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let title = required_string(payload, "event_title")?;
        let starts_at = required_string(payload, "starts_at")?;
        let details_url = required_string(payload, "details_url")?;
        let preheader = optional_string(payload, "preheader");

        let subject = format!("Event positions published: {title}");

        let body = html! {
            p {
                "Positions have been published for "
                strong { (title) }
                "."
            }
            (callout(html! {
                p { strong { "Starts:" } " " (starts_at) }
            }))
            p { "Open the event page for staffing details." }
        };

        let html = EmailLayout::new(&subject)
            .preheader(preheader.as_deref().unwrap_or(&subject))
            .heading(&title)
            .unsubscribe_link(unsubscribe_link)
            .render(body, Some(("View event", &details_url)))
            .into_string();

        let text = TextBuilder::new()
            .line(&format!("Positions have been published for {title}."))
            .blank()
            .line(&format!("Starts: {starts_at}"))
            .link("View details", &details_url)
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}

pub struct EventReminderTemplate;

impl RsxTemplate for EventReminderTemplate {
    fn id(&self) -> &'static str {
        "events.reminder"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let title = required_string(payload, "event_title")?;
        let starts_at = required_string(payload, "starts_at")?;
        let details_url = required_string(payload, "details_url")?;
        let location = optional_string(payload, "location");
        let preheader = optional_string(payload, "preheader");

        let subject = format!("Reminder: {title}");

        let body = html! {
            p {
                "This is a reminder for "
                strong { (title) }
                "."
            }
            (callout(html! {
                p { strong { "Starts:" } " " (starts_at) }
                @if let Some(ref loc) = location {
                    p { strong { "Location:" } " " (loc) }
                }
            }))
        };

        let html = EmailLayout::new(&subject)
            .preheader(preheader.as_deref().unwrap_or(&subject))
            .heading(&title)
            .unsubscribe_link(unsubscribe_link)
            .render(body, Some(("Open details", &details_url)))
            .into_string();

        let mut text = TextBuilder::new()
            .line(&format!("This is a reminder for {title}."))
            .blank()
            .line(&format!("Starts: {starts_at}"));

        if let Some(ref loc) = location {
            text = text.line(&format!("Location: {loc}"));
        }

        let text = text
            .link("View details", &details_url)
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}
