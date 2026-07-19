use maud::{Markup, html};

use crate::email::branding::EmailTheme;

pub fn email_footer(theme: &EmailTheme, unsubscribe_link: Option<&str>) -> Markup {
    html! {
        tr {
            td class="footer" {
                p { (theme.branding.footer_text) }
                @if let Some(url) = unsubscribe_link {
                    p class="footer-link" {
                        a href=(url) { "Unsubscribe from this category" }
                    }
                }
            }
        }
    }
}
