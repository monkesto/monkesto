use crate::theme::theme_with_head;
use maud::Markup;
use maud::html;

pub fn layout(nav_title: Option<Markup>, content: Markup) -> Markup {
    theme_with_head(
        Some("Monkesto"),
        html! {},
        html! {
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
                                @if let Some(title_markup) = nav_title {
                                    (title_markup)
                                }
                                a
                                    href="/me"
                                    class="text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 px-2 py-1" {
                                    "Profile"
                                }
                                form action="/signout" method="post" {
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
        },
    )
}
