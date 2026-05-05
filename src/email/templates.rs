use serde_json::{Value, json};

use crate::errors::ApiError;

use super::suppression::build_unsubscribe_link;

#[derive(Debug, Clone)]
pub struct RenderedEmail {
    pub subject: String,
    pub html: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct TemplateDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub is_transactional: bool,
    pub allow_arbitrary_addresses: bool,
    pub respect_user_event_pref: bool,
    pub payload_schema: fn() -> Value,
    pub renderer: fn(&Value, Option<&str>) -> Result<RenderedEmail, ApiError>,
}

pub fn registry() -> &'static [TemplateDefinition] {
    &[
        TemplateDefinition {
            id: "announcements.generic",
            name: "Generic Announcement",
            category: "announcements",
            description: "Generic formatted announcement email",
            is_transactional: false,
            allow_arbitrary_addresses: true,
            respect_user_event_pref: false,
            payload_schema: announcement_schema,
            renderer: render_announcement,
        },
        TemplateDefinition {
            id: "events.position_published",
            name: "Event Position Published",
            category: "event_notifications",
            description: "Event position publication notice",
            is_transactional: false,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: true,
            payload_schema: event_position_published_schema,
            renderer: render_event_position_published,
        },
        TemplateDefinition {
            id: "events.reminder",
            name: "Event Reminder",
            category: "event_notifications",
            description: "Reminder for an upcoming event",
            is_transactional: false,
            allow_arbitrary_addresses: false,
            respect_user_event_pref: true,
            payload_schema: event_reminder_schema,
            renderer: render_event_reminder,
        },
        TemplateDefinition {
            id: "system.test_email",
            name: "System Test Email",
            category: "transactional",
            description: "Simple diagnostic email for SES connectivity",
            is_transactional: true,
            allow_arbitrary_addresses: true,
            respect_user_event_pref: false,
            payload_schema: system_test_schema,
            renderer: render_system_test,
        },
    ]
}

pub fn find_template(template_id: &str) -> Option<&'static TemplateDefinition> {
    registry()
        .iter()
        .find(|template| template.id == template_id)
}

fn announcement_schema() -> Value {
    json!({
        "type": "object",
        "required": ["headline", "body_markdown"],
        "properties": {
            "headline": { "type": "string" },
            "body_markdown": { "type": "string" },
            "preheader": { "type": "string" },
            "cta_label": { "type": "string" },
            "cta_url": { "type": "string" }
        }
    })
}

fn event_position_published_schema() -> Value {
    json!({
        "type": "object",
        "required": ["event_title", "starts_at", "details_url"],
        "properties": {
            "event_title": { "type": "string" },
            "starts_at": { "type": "string" },
            "details_url": { "type": "string" },
            "preheader": { "type": "string" }
        }
    })
}

fn event_reminder_schema() -> Value {
    json!({
        "type": "object",
        "required": ["event_title", "starts_at", "details_url"],
        "properties": {
            "event_title": { "type": "string" },
            "starts_at": { "type": "string" },
            "details_url": { "type": "string" },
            "location": { "type": "string" },
            "preheader": { "type": "string" }
        }
    })
}

fn system_test_schema() -> Value {
    json!({
        "type": "object",
        "required": ["message"],
        "properties": {
            "message": { "type": "string" },
            "requested_by": { "type": "string" }
        }
    })
}

fn render_announcement(
    payload: &Value,
    unsubscribe_link: Option<&str>,
) -> Result<RenderedEmail, ApiError> {
    let headline = required_string(payload, "headline")?;
    let body = required_string(payload, "body_markdown")?;
    let preheader = optional_string(payload, "preheader").unwrap_or_else(|| headline.clone());
    let cta_label = optional_string(payload, "cta_label");
    let cta_url = optional_string(payload, "cta_url");
    Ok(render_layout(
        &headline,
        &preheader,
        Some(&headline),
        &markdownish_to_html(&body),
        &body,
        cta_label.as_deref().zip(cta_url.as_deref()),
        unsubscribe_link,
    ))
}

fn render_event_position_published(
    payload: &Value,
    unsubscribe_link: Option<&str>,
) -> Result<RenderedEmail, ApiError> {
    let title = required_string(payload, "event_title")?;
    let starts_at = required_string(payload, "starts_at")?;
    let details_url = required_string(payload, "details_url")?;
    let subject = format!("Event positions published: {title}");
    let body_text = format!(
        "Positions have been published for {title}.\n\nStarts: {starts_at}\nView details: {details_url}"
    );
    let body_html = format!(
        "<p>Positions have been published for <strong>{}</strong>.</p><div class=\"callout\"><p><strong>Starts:</strong> {}</p></div><p>Open the event page for staffing details.</p>",
        escape_html(&title),
        escape_html(&starts_at),
    );
    Ok(render_layout(
        &subject,
        &optional_string(payload, "preheader").unwrap_or(subject.clone()),
        Some(&title),
        &body_html,
        &body_text,
        Some(("View event", details_url.as_str())),
        unsubscribe_link,
    ))
}

fn render_event_reminder(
    payload: &Value,
    unsubscribe_link: Option<&str>,
) -> Result<RenderedEmail, ApiError> {
    let title = required_string(payload, "event_title")?;
    let starts_at = required_string(payload, "starts_at")?;
    let details_url = required_string(payload, "details_url")?;
    let location = optional_string(payload, "location");
    let subject = format!("Reminder: {title}");
    let mut text = format!("This is a reminder for {title}.\n\nStarts: {starts_at}");
    if let Some(location) = location.as_deref() {
        text.push_str(&format!("\nLocation: {location}"));
    }
    text.push_str(&format!("\nView details: {details_url}"));
    let body_html = format!(
        "<p>This is a reminder for <strong>{}</strong>.</p><div class=\"callout\"><p><strong>Starts:</strong> {}</p>{}</div>",
        escape_html(&title),
        escape_html(&starts_at),
        location
            .as_deref()
            .map(|value| format!("<p><strong>Location:</strong> {}</p>", escape_html(value)))
            .unwrap_or_default(),
    );
    Ok(render_layout(
        &subject,
        &optional_string(payload, "preheader").unwrap_or(subject.clone()),
        Some(&title),
        &body_html,
        &text,
        Some(("Open details", details_url.as_str())),
        unsubscribe_link,
    ))
}

fn render_system_test(
    payload: &Value,
    _unsubscribe_link: Option<&str>,
) -> Result<RenderedEmail, ApiError> {
    let message = required_string(payload, "message")?;
    let requested_by = optional_string(payload, "requested_by");
    let subject = "Osmium email transport test".to_string();
    let text = if let Some(requested_by) = requested_by.as_deref() {
        format!("{message}\n\nRequested by: {requested_by}")
    } else {
        message.clone()
    };
    let mut body_html = format!("<p>{}</p>", escape_html(&message));
    if let Some(requested_by) = requested_by.as_deref() {
        body_html.push_str(&format!(
            "<div class=\"callout\"><p><strong>Requested by:</strong> {}</p></div>",
            escape_html(requested_by)
        ));
    }
    Ok(render_layout(
        &subject,
        "Diagnostic SES connectivity email",
        Some("Transport test"),
        &body_html,
        &text,
        None,
        None,
    ))
}

fn required_string(payload: &Value, key: &str) -> Result<String, ApiError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(ApiError::BadRequest)
}

fn optional_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn render_layout(
    subject: &str,
    preheader: &str,
    heading: Option<&str>,
    body_html: &str,
    body_text: &str,
    cta: Option<(&str, &str)>,
    unsubscribe_link: Option<&str>,
) -> RenderedEmail {
    let title = heading.unwrap_or(subject);
    let cta_html = cta
        .map(|(label, url)| {
            format!(
                "<p style=\"margin:24px 0 0;\"><a class=\"button\" href=\"{}\">{}</a></p>",
                escape_html(url),
                escape_html(label),
            )
        })
        .unwrap_or_default();
    let unsubscribe_html = unsubscribe_link
        .map(|url| {
            format!(
                "<p class=\"footer-link\"><a href=\"{}\">Unsubscribe from this category</a></p>",
                escape_html(url)
            )
        })
        .unwrap_or_default();

    let html = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title><style>{}</style></head><body><div class=\"preheader\">{}</div><table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" class=\"bg\"><tr><td align=\"center\"><table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" class=\"shell\"><tr><td class=\"header\"><div class=\"brand\">Osmium</div><div class=\"eyebrow\">Email Platform</div></td></tr><tr><td class=\"panel\"><h1>{}</h1>{}{}{}</td></tr><tr><td class=\"footer\"><p>Sent by Osmium.</p>{}</td></tr></table></td></tr></table></body></html>",
        escape_html(subject),
        STYLE,
        escape_html(preheader),
        escape_html(title),
        body_html,
        cta_html,
        "",
        unsubscribe_html,
    );

    let mut text = body_text.to_string();
    if let Some((label, url)) = cta {
        text.push_str(&format!("\n\n{}: {}", label, url));
    }
    if let Some(url) = unsubscribe_link {
        text.push_str(&format!("\n\nUnsubscribe from this category: {}", url));
    }

    RenderedEmail {
        subject: subject.to_string(),
        html,
        text,
    }
}

const STYLE: &str = "
body{margin:0;padding:0;background:#ede7db;color:#1d2935;font-family:Georgia,'Palatino Linotype',serif}
.bg{background:radial-gradient(circle at top left,#f4ece0 0%,#ede7db 40%,#e5ddd0 100%);padding:24px}
.shell{max-width:640px;margin:0 auto}
.header{background:linear-gradient(135deg,#0d5c63 0%,#184e77 100%);padding:28px 32px;color:#f9fafb;border-radius:18px 18px 0 0}
.brand{font-size:28px;letter-spacing:.04em;text-transform:uppercase}
.eyebrow{font-size:12px;letter-spacing:.18em;text-transform:uppercase;opacity:.82;margin-top:8px}
.panel{background:#fffdf8;padding:36px 32px;border-left:1px solid #d6cfbf;border-right:1px solid #d6cfbf}
.panel h1{margin:0 0 18px;color:#8b3d2e;font-size:30px;line-height:1.2}
.panel p{font-size:16px;line-height:1.7;margin:0 0 16px}
.callout{background:#f4ece0;border-left:4px solid #0d5c63;padding:14px 16px;margin:18px 0}
.button{display:inline-block;padding:12px 18px;background:#8b3d2e;color:#fffdf8 !important;text-decoration:none;border-radius:999px;font-weight:bold}
.footer{background:#f6f1e8;padding:20px 32px;border:1px solid #d6cfbf;border-top:0;border-radius:0 0 18px 18px;color:#586574}
.footer p{margin:0;font-size:13px;line-height:1.6}
.footer-link{margin-top:8px !important}
.preheader{display:none!important;visibility:hidden;opacity:0;color:transparent;height:0;width:0;overflow:hidden}
@media only screen and (max-width:640px){.bg{padding:12px}.header,.panel,.footer{padding-left:22px;padding-right:22px}.panel h1{font-size:26px}}
";

fn markdownish_to_html(markdown: &str) -> String {
    let paragraphs = markdown
        .split("\n\n")
        .map(|segment| format!("<p>{}</p>", escape_html(&segment.replace('\n', "<br>"))))
        .collect::<Vec<_>>();
    paragraphs.join("")
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn unsubscribe_link(
    base_url: Option<&str>,
    secret: Option<&str>,
    category: &str,
    email: &str,
    user_id: Option<&str>,
) -> Option<String> {
    build_unsubscribe_link(base_url?, secret?, category, email, user_id)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::find_template;

    #[test]
    fn registry_contains_expected_templates() {
        for id in [
            "announcements.generic",
            "events.position_published",
            "events.reminder",
            "system.test_email",
        ] {
            assert!(find_template(id).is_some(), "missing template: {id}");
        }
    }

    #[test]
    fn announcement_renders_non_empty_parts() {
        let template = find_template("announcements.generic").unwrap();
        let rendered = (template.renderer)(
            &json!({"headline":"Status update","body_markdown":"Hello team"}),
            None,
        )
        .unwrap();
        assert!(!rendered.subject.is_empty());
        assert!(rendered.html.contains("Status update"));
        assert!(rendered.text.contains("Hello team"));
    }
}
