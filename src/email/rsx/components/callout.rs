use maud::{html, Markup};

pub fn callout(content: Markup) -> Markup {
    html! {
        div class="callout" { (content) }
    }
}
