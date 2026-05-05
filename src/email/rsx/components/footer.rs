use maud::{html, Markup};

pub fn email_footer(unsubscribe_link: Option<&str>) -> Markup {
    html! {
        tr {
            td class="footer" {
                p { "Sent by vZDC." }
                @if let Some(url) = unsubscribe_link {
                    p class="footer-link" {
                        a href=(url) { "Unsubscribe from this category" }
                    }
                }
            }
        }
    }
}
