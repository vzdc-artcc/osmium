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
    rsx_template.render(payload, &theme, link.as_deref())
}
