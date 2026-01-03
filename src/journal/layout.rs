use maud::{Markup, html};
use crate::maud_header;

pub fn maud_layout(
    page_title: Option<&str>,
    show_switch_link: bool,
    journal_id: Option<&str>,
    content: Markup,
) -> Markup {
    let main_html = html! {
        div class="min-h-full" {
            // Global Navigation Bar
            nav class="bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700" {
                div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8" {
                    div class="flex justify-between h-16" {
                        div class="flex items-center" {
                            img src="/logo.svg" alt="Monkesto" class="h-8 w-auto";
                            span class="ml-4 text-xl font-bold text-gray-900 dark:text-white" {
                                "Monkesto"
                            }
                        }
                        div class="flex items-center gap-4" {
                            @if let Some(title) = page_title {
                                div class="flex flex-col items-end justify-center" {
                                    @if let Some(id) = journal_id {
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
                                    @if show_switch_link {
                                        a
                                            href="/journal"
                                            class="text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200" {
                                            "Switch"
                                        }
                                    }
                                }
                            }
                            form action="/logout" method="post" {
                                button
                                    class="text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 px-2 py-1"
                                    type="submit" {
                                    "Sign out"
                                }
                            }
                        }
                    }
                }
            }

            // Main Content
            div class="flex-1 p-6" {
                div class="max-w-7xl mx-auto" {
                    div class="flex flex-col gap-6 sm:mx-auto sm:w-full sm:max-w-sm" {
                        (content)
                    }
                }
            }
        }
    };

    maud_header::header(main_html)
}
