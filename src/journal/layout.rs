use crate::auth::layout as app_layout;
use maud::{Markup, html};

pub fn layout(
    page_title: Option<&str>,
    show_switch_link: bool,
    journal_id: Option<&str>,
    content: Markup,
) -> Markup {
    let nav_title = match (page_title, show_switch_link, journal_id) {
        (Some(title), switch_link, journal_id_opt) => Some(html! {
            div class="flex flex-col items-end justify-center" {
                @if let Some(id) = journal_id_opt {
                    a
                        href=(format!("/journal/{}", id))
                        class="text-sm font-medium text-gray-700 dark:text-gray-300 hover:text-gray-900 dark:hover:text-white" {
                        (title)
                    }
                } @else {
                    span class="text-sm font-medium text-gray-700 dark:text-gray-300" {
                        (title)
                    }
                }
                @if switch_link {
                    a
                        href="/journal"
                        class="text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200" {
                        "Switch"
                    }
                }
            }
        }),
        _ => None,
    };

    app_layout(nav_title, content)
}
