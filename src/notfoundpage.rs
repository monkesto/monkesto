use maud::{html, Markup};

use crate::maud_header;

pub async fn not_found_page() -> Markup {
    maud_header::header(html! {
        p {
            "Page not found"
        }
    })
}
