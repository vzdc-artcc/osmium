use axum::{Json, extract::Path, response::Html};
use serde_json::json;

use crate::docs;

pub async fn docs_index() -> Html<String> {
    let page = docs::find_doc_page(None, None).expect("docs index page must exist");
    Html(docs::render_markdown_page(page))
}

pub async fn docs_page(Path((section, page)): Path<(String, String)>) -> Html<String> {
    let page = docs::find_doc_page(Some(&section), Some(&page))
        .unwrap_or_else(|| docs::find_doc_page(None, None).expect("docs index page must exist"));
    Html(docs::render_markdown_page(page))
}

pub async fn docs_health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "message": "Documentation service is running"
    }))
}
