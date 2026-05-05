use crate::errors::ApiError;

use super::rsx::find_rsx_template;
use super::templates::{RenderedEmail, TemplateDefinition, unsubscribe_link};

pub fn render_template(
    template: &TemplateDefinition,
    payload: &serde_json::Value,
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

    if let Some(rsx_template) = find_rsx_template(template.id) {
        return rsx_template.render(payload, link.as_deref());
    }

    (template.renderer)(payload, link.as_deref())
}
