use maud::html;
use serde_json::Value;

use crate::email::branding::EmailTheme;
use crate::email::rsx::components::EmailLayout;
use crate::email::rsx::text::TextBuilder;
use crate::email::rsx::validate::required_string;
use crate::email::templates::RenderedEmail;
use crate::errors::ApiError;

use super::RsxTemplate;

pub struct RosterRemovedTemplate;

impl RsxTemplate for RosterRemovedTemplate {
    fn id(&self) -> &'static str {
        "roster.removed"
    }

    fn render(
        &self,
        payload: &Value,
        theme: &EmailTheme,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let controller_name = required_string(payload, "controller_name")?;
        let reason = required_string(payload, "reason")?;

        let subject = "Roster Status Update".to_string();

        let body = html! {
            p {
                "Your roster status has been updated to " strong { "NONE" } "."
            }
            p {
                strong { "Reason: " } (reason)
            }
            p {
                "If you have questions about this change, please contact facility staff."
            }
        };

        let html = EmailLayout::new(&subject, theme)
            .preheader(&format!("{controller_name} removed from the roster"))
            .heading("Roster Status Update")
            .unsubscribe_link(unsubscribe_link)
            .render(body, None)
            .into_string();

        let text = TextBuilder::new()
            .line("Your roster status has been updated to NONE.")
            .blank()
            .line(&format!("Reason: {reason}"))
            .blank()
            .line("If you have questions about this change, please contact facility staff.")
            .optional_unsubscribe(unsubscribe_link)
            .build();

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}
