use maud::{Markup, html};

use crate::theme;

pub async fn not_found_page() -> Markup {
    theme::theme(html! {
        p {
            "Page not found"
        }
    })
}
