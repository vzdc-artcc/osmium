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

pub struct LoaApprovedTemplate;

impl RsxTemplate for LoaApprovedTemplate {
    fn id(&self) -> &'static str {
        "loa.approved"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let controller_name = required_string(payload, "controller_name")?;
        let loa_start = required_string(payload, "loa_start")?;
        let loa_end = required_string(payload, "loa_end")?;

        let subject = "Your LOA has been approved".to_string();

        let body = html! {
            p { "Your Leave of Absence request has been approved." }
            (callout(html! {
                p { strong { "Start:" } " " (loa_start) }
                p { strong { "End:" } " " (loa_end) }
            }))
            p {
                "Please note that your LOA will be automatically canceled if you control "
                "during your leave period. If you need to extend or modify your LOA, please "
                "contact the staff."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader(&format!("LOA approved for {controller_name}"))
            .heading("LOA Approved")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let text = TextBuilder::new()
            .line("Your Leave of Absence request has been approved.")
            .blank()
            .line(&format!("Start: {loa_start}"))
            .line(&format!("End: {loa_end}"))
            .blank()
            .line("Please note that your LOA will be automatically canceled if you control during your leave period.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}

pub struct LoaDeniedTemplate;

impl RsxTemplate for LoaDeniedTemplate {
    fn id(&self) -> &'static str {
        "loa.denied"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let _controller_name = required_string(payload, "controller_name")?;
        let reason = optional_string(payload, "reason");

        let subject = "Your LOA has been denied".to_string();

        let body = html! {
            p { "Your Leave of Absence request has been denied." }
            @if let Some(ref r) = reason {
                (callout(html! {
                    p { strong { "Reason:" } " " (r) }
                }))
            }
            p {
                "If you have questions about this decision, please contact the staff."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader("LOA request denied")
            .heading("LOA Denied")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let mut text = TextBuilder::new()
            .line("Your Leave of Absence request has been denied.");

        if let Some(ref r) = reason {
            text = text.blank().line(&format!("Reason: {r}"));
        }

        let text = text
            .blank()
            .line("If you have questions about this decision, please contact the staff.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}

pub struct LoaDeletedTemplate;

impl RsxTemplate for LoaDeletedTemplate {
    fn id(&self) -> &'static str {
        "loa.deleted"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let _controller_name = required_string(payload, "controller_name")?;
        let reason = optional_string(payload, "reason");

        let subject = "Your LOA has been deleted".to_string();

        let body = html! {
            p { "Your Leave of Absence has been deleted by staff." }
            @if let Some(ref r) = reason {
                (callout(html! {
                    p { strong { "Reason:" } " " (r) }
                }))
            }
            p {
                "You are now expected to meet normal activity requirements. "
                "If you have questions, please contact the staff."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader("LOA deleted")
            .heading("LOA Deleted")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let mut text = TextBuilder::new()
            .line("Your Leave of Absence has been deleted by staff.");

        if let Some(ref r) = reason {
            text = text.blank().line(&format!("Reason: {r}"));
        }

        let text = text
            .blank()
            .line("You are now expected to meet normal activity requirements.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}

pub struct LoaExpiredTemplate;

impl RsxTemplate for LoaExpiredTemplate {
    fn id(&self) -> &'static str {
        "loa.expired"
    }

    fn render(&self, payload: &Value, unsubscribe_link: Option<&str>) -> Result<RenderedEmail, ApiError> {
        let _controller_name = required_string(payload, "controller_name")?;

        let subject = "Your LOA has expired".to_string();

        let body = html! {
            p { "Your Leave of Absence has expired." }
            p {
                "You are now expected to meet normal activity requirements. "
                "If you need additional time, please submit a new LOA request."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader("LOA expired")
            .heading("LOA Expired")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let text = TextBuilder::new()
            .line("Your Leave of Absence has expired.")
            .blank()
            .line("You are now expected to meet normal activity requirements.")
            .line("If you need additional time, please submit a new LOA request.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail { subject, html, text })
    }
}
