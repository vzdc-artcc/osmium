use maud::{PreEscaped, html};
use serde_json::Value;

use crate::email::rsx::components::EmailLayout;
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

pub struct BroadcastPostedTemplate;

impl RsxTemplate for BroadcastPostedTemplate {
    fn id(&self) -> &'static str {
        "broadcast.posted"
    }

    fn render(
        &self,
        payload: &Value,
        unsubscribe_link: Option<&str>,
    ) -> Result<RenderedEmail, ApiError> {
        let title = required_string(payload, "title")?;
        let body_markdown = required_string(payload, "body_markdown")?;
        let preheader = optional_string(payload, "preheader").unwrap_or_else(|| title.clone());
        let details_url = optional_string(payload, "details_url");

        let subject = title.clone();
        let body_html = markdown_to_html(&body_markdown);

        let body = html! {
            (PreEscaped(&body_html))
        };

        let cta = details_url.as_deref().map(|url| ("Read more", url));

        let html = EmailLayout::new(&subject)
            .preheader(&preheader)
            .heading(&title)
            .unsubscribe_link(unsubscribe_link)
            .render(body, cta)
            .into_string();

        let mut text = TextBuilder::new().line(&body_markdown);

        if let Some(url) = details_url.as_deref() {
            text = text.blank().link("Read more", url);
        }

        let text = text.optional_unsubscribe(unsubscribe_link).build();

        Ok(RenderedEmail {
            subject,
            html,
            text,
        })
    }
}
