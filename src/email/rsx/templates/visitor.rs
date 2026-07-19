use maud::html;
use serde_json::Value;

use crate::email::branding::EmailTheme;
use crate::email::rsx::components::{EmailLayout, callout};
use crate::email::rsx::text::TextBuilder;
use crate::email::rsx::validate::{optional_string, required_string};
use crate::email::templates::RenderedEmail;
use crate::errors::ApiError;

use super::RsxTemplate;

pub struct VisitorAcceptedTemplate;

impl RsxTemplate for VisitorAcceptedTemplate {
    fn id(&self) -> &'static str {
        "visitor.accepted"
    }

    fn render(
        &self,
        payload: &Value,
        theme: &EmailTheme,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let user_name = required_string(payload, "user_name")?;
        let artcc_name =
            optional_string(payload, "artcc_name").unwrap_or_else(|| "the facility".to_string());
        let details_url = optional_string(payload, "details_url");

        let subject = format!("Welcome to {artcc_name}!");

        let body = html! {
            p {
                "Congratulations! Your visitor application to "
                strong { (artcc_name) }
                " has been accepted."
            }
            p {
                "You are now a visiting controller at our facility. Please review "
                "our Standard Operating Procedures and familiarize yourself with "
                "local procedures before controlling."
            }
            p {
                "We look forward to seeing you on frequency!"
            }
        };

        let cta = details_url.as_deref().map(|url| ("View your profile", url));

        let html = EmailLayout::new(&subject, theme)
            .preheader(&format!(
                "{user_name}, your visitor application was accepted"
            ))
            .heading("Application Accepted")
            .unsubscribe_link(unsubscribe_link)
            .render(body, cta)
            .into_string();

        let mut text = TextBuilder::new()
            .line(&format!("Congratulations! Your visitor application to {artcc_name} has been accepted."))
            .blank()
            .line("You are now a visiting controller at our facility. Please review our Standard Operating Procedures and familiarize yourself with local procedures before controlling.")
            .blank()
            .line("We look forward to seeing you on frequency!");

        if let Some(url) = details_url.as_deref() {
            text = text.link("View your profile", url);
        }

        let text = text.optional_unsubscribe(unsubscribe_link).build();

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}

pub struct VisitorRejectedTemplate;

impl RsxTemplate for VisitorRejectedTemplate {
    fn id(&self) -> &'static str {
        "visitor.rejected"
    }

    fn render(
        &self,
        payload: &Value,
        theme: &EmailTheme,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let _user_name = required_string(payload, "user_name")?;
        let artcc_name =
            optional_string(payload, "artcc_name").unwrap_or_else(|| "the facility".to_string());
        let reason = optional_string(payload, "reason");

        let subject = "Visitor application not accepted".to_string();

        let body = html! {
            p {
                "Unfortunately, your visitor application to "
                strong { (artcc_name) }
                " has not been accepted at this time."
            }
            @if let Some(ref r) = reason {
                (callout(html! {
                    p { strong { "Reason:" } " " (r) }
                }))
            }
            p {
                "If you have questions about this decision or would like more information, "
                "please contact the facility staff."
            }
        };

        let html = EmailLayout::new(&subject, theme)
            .preheader("Visitor application decision")
            .heading("Application Not Accepted")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let mut text = TextBuilder::new()
            .line(&format!("Unfortunately, your visitor application to {artcc_name} has not been accepted at this time."));

        if let Some(ref r) = reason {
            text = text.blank().line(&format!("Reason: {r}"));
        }

        let text = text
            .blank()
            .line("If you have questions about this decision or would like more information, please contact the facility staff.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}
