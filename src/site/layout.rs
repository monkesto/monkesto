use crate::api::main_api::LogOut;
use leptos::prelude::*;

#[component]
pub fn Layout(
    #[prop(optional)] page_title: Option<String>,
    #[prop(optional)] show_switch_link: bool,
    #[prop(optional)] journal_id: Option<String>,
    children: Children,
) -> impl IntoView {
    let logout_action = ServerAction::<LogOut>::new();

    view! {
        <div class="min-h-full">
            // Global Navigation Bar
            <nav class="bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div class="flex justify-between h-16">
                        <div class="flex items-center">
                            <img src="/logo.svg" alt="Monkesto" class="h-8 w-auto" />
                            <span class="ml-4 text-xl font-bold text-gray-900 dark:text-white">
                                "Monkesto"
                            </span>
                        </div>
                        <div class="flex items-center gap-4">
                            {if let Some(title) = page_title {
                                view! {
                                    <div class="flex flex-col items-end justify-center">
                                        {if let Some(id) = journal_id.clone() {
                                            view! {
                                                <a
                                                    href=format!("/journal/{}", id)
                                                    class="text-sm font-medium text-gray-700 dark:text-gray-300 hover:text-gray-900 dark:hover:text-white"
                                                >
                                                    {title.clone()}
                                                </a>
                                            }
                                                .into_any()
                                        } else {
                                            view! {
                                                <span class="text-sm font-medium text-gray-700 dark:text-gray-300">
                                                    {title}
                                                </span>
                                            }
                                                .into_any()
                                        }}
                                        {if show_switch_link {
                                            view! {
                                                <a
                                                    href="/"
                                                    class="text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
                                                >
                                                    "Switch"
                                                </a>
                                            }
                                                .into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }}
                                    </div>
                                }
                                    .into_any()
                            } else {
                                view! { <div></div> }.into_any()
                            }} <ActionForm action=logout_action>
                                <button
                                    class="text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 px-2 py-1"
                                    type="submit"
                                >
                                    "Sign out"
                                </button>
                            </ActionForm>
                        </div>
                    </div>
                </div>
            </nav>

            // Main Content
            <div class="flex-1 p-6">
                <div class="max-w-7xl mx-auto">
                    <div class="flex flex-col gap-6 sm:mx-auto sm:w-full sm:max-w-sm">
                        {children()}
                    </div>
                </div>
            </div>
        </div>
    }
}
