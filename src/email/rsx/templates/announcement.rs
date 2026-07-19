use maud::{PreEscaped, html};
use serde_json::Value;

use crate::email::branding::EmailTheme;
use crate::email::rsx::components::EmailLayout;
use crate::email::rsx::text::TextBuilder;
use crate::email::rsx::validate::{optional_string, required_string};
use crate::email::templates::RenderedEmail;
use crate::errors::ApiError;

use super::RsxTemplate;

fn markdown_to_html(markdown: &str) -> String {
    markdown
        .split("\n\n")
        .map(|segment| format!("<p>{}</p>", html_escape(segment).replace('\n', "<br>")))
        .collect::<Vec<_>>()
        .join("")
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub struct AnnouncementTemplate;

impl RsxTemplate for AnnouncementTemplate {
    fn id(&self) -> &'static str {
        "announcements.generic"
    }

    fn render(
        &self,
        payload: &Value,
        theme: &EmailTheme,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let headline = required_string(payload, "headline")?;
        let body_markdown = required_string(payload, "body_markdown")?;
        let preheader = optional_string(payload, "preheader").unwrap_or_else(|| headline.clone());
        let cta_label = optional_string(payload, "cta_label");
        let cta_url = optional_string(payload, "cta_url");

        let subject = headline.clone();
        let body_html = markdown_to_html(&body_markdown);

        let body = html! {
            (PreEscaped(&body_html))
        };

        let cta = cta_label.as_deref().zip(cta_url.as_deref());

        let html = EmailLayout::new(&subject, theme)
            .preheader(&preheader)
            .heading(&headline)
            .unsubscribe_link(unsubscribe_link)
            .render(body, cta)
            .into_string();

        let mut text_builder = TextBuilder::new().line(&body_markdown);
        if let Some((label, url)) = cta {
            text_builder = text_builder.blank().link(label, url);
        }
        let text = text_builder.optional_unsubscribe(unsubscribe_link).build();

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}
