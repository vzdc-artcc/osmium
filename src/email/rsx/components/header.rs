use maud::{html, Markup};

pub fn email_header() -> Markup {
    html! {
        tr {
            td class="header" {
                div class="brand" { "vZDC" }
                div class="eyebrow" { "Washington ARTCC" }
            }
        }
    }
}
