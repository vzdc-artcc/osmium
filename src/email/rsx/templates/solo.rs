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

pub struct SoloAddedTemplate;

impl RsxTemplate for SoloAddedTemplate {
    fn id(&self) -> &'static str {
        "solo.added"
    }

    fn render(
        &self,
        payload: &Value,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let controller_name = required_string(payload, "controller_name")?;
        let position = required_string(payload, "position")?;
        let expires = required_string(payload, "expires")?;

        let subject = format!("Solo certification granted for {position}");

        let body = html! {
            p {
                "Congratulations! You have been granted a solo certification."
            }
            (callout(html! {
                p { strong { "Position:" } " " (position) }
                p { strong { "Expires:" } " " (expires) }
            }))
            p {
                "Please be aware that this solo certification will expire on the date above. "
                "Continue working with your trainer to achieve full certification."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader(&format!("{controller_name} granted solo on {position}"))
            .heading("Solo Certification Granted")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let text = TextBuilder::new()
            .line("Congratulations! You have been granted a solo certification.")
            .blank()
            .line(&format!("Position: {position}"))
            .line(&format!("Expires: {expires}"))
            .blank()
            .line("Please be aware that this solo certification will expire on the date above.")
            .line("Continue working with your trainer to achieve full certification.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}

pub struct SoloDeletedTemplate;

impl RsxTemplate for SoloDeletedTemplate {
    fn id(&self) -> &'static str {
        "solo.deleted"
    }

    fn render(
        &self,
        payload: &Value,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let _controller_name = required_string(payload, "controller_name")?;
        let position = required_string(payload, "position")?;
        let reason = optional_string(payload, "reason");

        let subject = format!("Solo certification removed for {position}");

        let body = html! {
            p {
                "Your solo certification for "
                strong { (position) }
                " has been removed."
            }
            @if let Some(ref r) = reason {
                (callout(html! {
                    p { strong { "Reason:" } " " (r) }
                }))
            }
            p {
                "If you have questions, please contact your training staff."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader(&format!("Solo removed for {position}"))
            .heading("Solo Certification Removed")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let mut text = TextBuilder::new().line(&format!(
            "Your solo certification for {position} has been removed."
        ));

        if let Some(ref r) = reason {
            text = text.blank().line(&format!("Reason: {r}"));
        }

        let text = text
            .blank()
            .line("If you have questions, please contact your training staff.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}

pub struct SoloExpiredTemplate;

impl RsxTemplate for SoloExpiredTemplate {
    fn id(&self) -> &'static str {
        "solo.expired"
    }

    fn render(
        &self,
        payload: &Value,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let _controller_name = required_string(payload, "controller_name")?;
        let position = required_string(payload, "position")?;

        let subject = format!("Solo certification expired for {position}");

        let body = html! {
            p {
                "Your solo certification for "
                strong { (position) }
                " has expired."
            }
            p {
                "Please contact your training staff to discuss next steps for your certification."
            }
        };

        let html = EmailLayout::new(&subject)
            .preheader(&format!("Solo expired for {position}"))
            .heading("Solo Certification Expired")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let text = TextBuilder::new()
            .line(&format!(
                "Your solo certification for {position} has expired."
            ))
            .blank()
            .line(
                "Please contact your training staff to discuss next steps for your certification.",
            )
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}
