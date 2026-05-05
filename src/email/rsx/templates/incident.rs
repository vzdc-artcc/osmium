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

pub struct IncidentClosedTemplate;

impl RsxTemplate for IncidentClosedTemplate {
    fn id(&self) -> &'static str {
        "incident.closed"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let controller_name = required_string(payload, "controller_name")?;
        let incident_date = optional_string(payload, "incident_date");
        let resolution = optional_string(payload, "resolution");

        let subject = "Incident report closed".to_string();

        let body = html! {
            p {
                "An incident report involving you has been reviewed and closed."
            }
            @if incident_date.is_some() || resolution.is_some() {
                (callout(html! {
                    @if let Some(ref date) = incident_date {
                        p { strong { "Incident Date:" } " " (date) }
                    }
                    @if let Some(ref res) = resolution {
                        p { strong { "Resolution:" } " " (res) }
                    }
                }))
            }
            p {
                "If you have questions about this incident or the resolution, "
                "please contact the appropriate staff member."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader(&format!("Incident closed for {controller_name}"))
            .heading("Incident Closed")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let mut text = TextBuilder::new()
            .line("An incident report involving you has been reviewed and closed.");

        if let Some(ref date) = incident_date {
            text = text.line(&format!("Incident Date: {date}"));
        }

        if let Some(ref res) = resolution {
            text = text.line(&format!("Resolution: {res}"));
        }

        let text = text
            .blank()
            .line("If you have questions about this incident or the resolution, please contact the appropriate staff member.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}
