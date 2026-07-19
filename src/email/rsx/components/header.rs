use maud::{Markup, html};

use crate::email::branding::EmailTheme;

pub fn email_header(theme: &EmailTheme) -> Markup {
    html! {
        tr {
            td class="header" {
                @if let Some(logo_url) = theme.logo_url.as_deref() {
                    img src=(logo_url) alt=(theme.branding.brand_name);
                } @else {
                    div class="brand" { (theme.branding.brand_name) }
                }
                div class="eyebrow" { (theme.branding.tagline) }
            }
        }
    }
}
