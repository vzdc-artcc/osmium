use maud::{Markup, html};

pub fn callout(content: Markup) -> Markup {
    html! {
        div class="callout" { (content) }
    }
}
