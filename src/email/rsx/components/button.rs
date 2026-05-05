use maud::{html, Markup};

pub fn cta_button(label: &str, url: &str) -> Markup {
    html! {
        p style="margin:24px 0 0;" {
            a class="button" href=(url) { (label) }
        }
    }
}
