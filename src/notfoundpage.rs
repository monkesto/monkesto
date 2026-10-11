use axum::http::StatusCode;
use axum::response::IntoResponse;
use maud::html;

use crate::theme;

pub async fn not_found_page() -> impl IntoResponse {
    let markup = theme::theme(html! {
        p {
            "Page not found"
        }
    });

    (StatusCode::NOT_FOUND, markup)
}
