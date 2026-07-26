use crate::{errors::ApiError, models::EmailBranding};

use super::branding::EmailTheme;
use super::rsx::find_rsx_template;
use super::templates::{RenderedEmail, TemplateDefinition, unsubscribe_link};

#[allow(clippy::too_many_arguments)]
pub fn render_template(
    template: &TemplateDefinition,
    payload: &serde_json::Value,
    branding: &EmailBranding,
    unsubscribe_base_url: Option<&str>,
    unsubscribe_secret: Option<&str>,
    recipient_email: Option<&str>,
    recipient_user_id: Option<&str>,
) -> Result<RenderedEmail, ApiError> {
    let link = if template.is_transactional {
        None
    } else {
        recipient_email.and_then(|email| {
            unsubscribe_link(
                unsubscribe_base_url,
                unsubscribe_secret,
                template.category,
                email,
                recipient_user_id,
            )
        })
    };

    // `unsubscribe_base_url` is really "this deployment's public base URL" —
    // reused here to build an absolute logo URL rather than introducing a
    // second base-URL env var for a single field.
    let logo_url = branding.logo_file_id.as_deref().and_then(|file_id| {
        unsubscribe_base_url.map(|base| format!("{}/cdn/{file_id}", base.trim_end_matches('/')))
    });
    let theme = EmailTheme::new(branding, logo_url);

    let rsx_template = find_rsx_template(template.id).ok_or(ApiError::Internal)?;
    let mut rendered = rsx_template.render(payload, &theme, link.as_deref())?;
    rendered.html = inline_email_css(&rendered.html);
    Ok(rendered)
}

/// Inlines the email's `<style>` class rules onto each element so clients that
/// strip `<style>` (Gmail, Outlook, …) still render the brand colors — otherwise
/// the sent email loses the header/CTA colors even though the browser preview
/// (which honors `<style>`) looks correct. `keep_style_tags` leaves the original
/// block in place too, so the responsive `@media` rule (not inline-able) survives.
fn inline_email_css(html: &str) -> String {
    let options = css_inline::InlineOptions {
        keep_style_tags: true,
        load_remote_stylesheets: false,
        ..Default::default()
    };
    css_inline::CSSInliner::new(options)
        .inline(html)
        .unwrap_or_else(|_| html.to_string())
}
