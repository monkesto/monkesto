use super::homepage::Journal;
use crate::cuid::Cuid;
use crate::layout::Layout;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

struct Person {
    pub id: Cuid,
    pub username: String,
}

fn people() -> Vec<Person> {
    vec![
        Person {
            id: Cuid::from_str("aaaaaaaaab").expect("Invalid CUID"),
            username: "johndoe".to_string(),
        },
        Person {
            id: Cuid::from_str("aaaaaaaaac").expect("Invalid CUID"),
            username: "janesmith".to_string(),
        },
        Person {
            id: Cuid::from_str("aaaaaaaaad").expect("Invalid CUID"),
            username: "bobjohnson".to_string(),
        },
    ]
}

fn journals() -> Vec<Journal> {
    vec![
        Journal {
            id: Cuid::from_str("aaaaaaaaab").expect("Invalid CUID"),
            name: "Personal".to_string(),
            creator_username: "johndoe".to_string(),
            created_at: "2024-01-15 09:30:45".to_string(),
        },
        Journal {
            id: Cuid::from_str("aaaaaaaaac").expect("Invalid CUID"),
            name: "Business".to_string(),
            creator_username: "janesmith".to_string(),
            created_at: "2024-01-20 14:22:18".to_string(),
        },
    ]
}

#[component]
pub fn PeopleListPage() -> impl IntoView {
    let params = use_params_map();
    let journal_id = move || params.get().get("id").unwrap_or_default().to_string();

    let journal_name = move || {
        journals()
            .into_iter()
            .find(|j| j.id.to_string() == journal_id())
            .map(|j| j.name)
            .unwrap_or_else(|| "Unknown Journal".to_string())
    };

    view! {
        <Layout page_title=journal_name() show_switch_link=true journal_id=journal_id()>
            {people()
                .into_iter()
                .map(|person| {
                    view! {
                        <a
                            href=format!("/journal/{}/person/{}", journal_id(), person.id)
                            class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                        >
                            <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                                {person.username}
                            </h3>
                        </a>
                    }
                })
                .collect_view()}
            <hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600" />
            <div class="mt-10">
                <form class="space-y-6">
                    <div>
                        <label
                            for="username"
                            class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100"
                        >
                            "Invite Person"
                        </label>
                        <div class="mt-2">
                            <input
                                id="username"
                                type="text"
                                name="username"
                                required
                                placeholder="Enter username to invite"
                                class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                            />
                        </div>
                    </div>
                    <div>
                        <button
                            type="submit"
                            class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500"
                        >
                            "Send Invite"
                        </button>
                    </div>
                </form>
            </div>
        </Layout>
    }
}
