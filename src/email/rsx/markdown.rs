use pulldown_cmark::{Options, Parser, html};

/// Renders admin-authored markdown to HTML for email bodies (bold, italics,
/// links, lists, headings, tables, strikethrough). pulldown-cmark escapes text
/// content itself; authors are SERVER_ADMIN, so this is trusted input.
pub fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(markdown, options));
    out
}

#[cfg(test)]
mod tests {
    use super::markdown_to_html;

    #[test]
    fn renders_bold_and_links() {
        let html = markdown_to_html("Effective **immediately**, see [details](https://x.com).");
        assert!(html.contains("<strong>immediately</strong>"));
        assert!(html.contains(r#"<a href="https://x.com">details</a>"#));
    }
}
