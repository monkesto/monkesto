use super::layout::Layout;
use crate::api::main_api;
use crate::api::return_types::*;
use crate::event_sourcing::journal::Permissions;
use leptos::prelude::*;

use uuid::Uuid;

#[component]
fn AddAccount(user_id: Uuid, journal_id: Uuid) -> impl IntoView {
    let add_account = ServerAction::<main_api::AddAccount>::new();
    view! {
        <div class="flex flex-col items-center text-center px-10 py-10">
            <h1>"Create a new account"</h1>
            <ActionForm action=add_account>
                <input
                    class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                    type="text"
                    name="account_name"
                    required
                />

                <input type="hidden" name="user_id" value=user_id.to_string() />
                <input type="hidden" name="journal_id" value=journal_id.to_string() />

                <button
                    class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400"
                    type="submit"
                >
                    "Add"
                </button>
            </ActionForm>

            {move || {
                match add_account.value().get() {
                    Some(Err(e)) => {

                        view! {
                            <p>"An error occurred while creating the account: " {e.to_string()}</p>
                        }
                            .into_any()
                    }
                    _ => view! { "" }.into_any(),
                }
            }}

        </div>
    }
}

#[component]
pub fn AccountList(mut accounts: Vec<Account>, journals: Journals, user_id: Uuid) -> impl IntoView {
    let create_journal = ServerAction::<main_api::CreateJournal>::new();

    view! {
        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <ActionForm action=create_journal>
                <input type="hidden" name="user_id" value=user_id.to_string() />
                <input
                    class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                    name="journal_name"
                    type="text"
                    placeholder="journal name"
                    required
                />
                <button
                    type="submit"
                    class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400"
                >
                    "Create Journal!"
                </button>
            </ActionForm>

            {move || {
                match create_journal.value().get() {
                    Some(Err(e)) => {
                        view! {
                            <p>"An error occurred while creating the journal: " {e.to_string()}</p>
                        }
                            .into_any()
                    }
                    _ => {

                        view! { "" }
                            .into_any()
                    }
                }
            }}

            <h1 class="font-bold text-4xl">"Accounts"</h1>
        </div>
        <div class="mx-auto flex min-w-full flex-col items-center">
            <ul>
                {
                    accounts.sort_unstable_by_key(|account| account.name.clone());
                    accounts
                        .into_iter()
                        .map(|account| {
                            view! {
                                <li class="px-1 py-1 font-bold text-2xl">
                                    {account.name}"    "
                                    {format!(
                                        "${}.{:02} {}",
                                        account.balance.abs() / 100,
                                        account.balance.abs() % 100,
                                        if account.balance < 0 { "Dr" } else { "Cr" },
                                    )}
                                </li>
                            }
                        })
                        .collect_view()
                }
            </ul>
        </div>

        {move || {
            if let Some(selected) = journals.selected.clone() {
                match selected {
                    AssociatedJournal::Owned { id, .. } => {
                        view! { <AddAccount user_id=user_id journal_id=id /> }.into_any()
                    }
                    AssociatedJournal::Shared { id, tenant_info, .. } => {
                        if tenant_info.tenant_permissions.contains(Permissions::ADDACCOUNT) {
                            view! { <AddAccount user_id=user_id journal_id=id /> }.into_any()
                        } else {
                            view! { "" }.into_any()
                        }
                    }
                }
            } else {
                view! { "" }.into_any()
            }
        }}
    }
}

struct AccountItem {
    pub id: Uuid,
    pub name: String,
    pub balance: i64, // in cents
}

fn accounts() -> Vec<AccountItem> {
    use std::str::FromStr;
    vec![
        AccountItem {
            id: Uuid::from_str("450e8400-e29b-41d4-a716-446655440000").expect("Invalid UUID"),
            name: "Cash".to_string(),
            balance: 25043, // $250.43
        },
        AccountItem {
            id: Uuid::from_str("450e8400-e29b-41d4-a716-446655440001").expect("Invalid UUID"),
            name: "Checking Account".to_string(),
            balance: 152067, // $1,520.67
        },
        AccountItem {
            id: Uuid::from_str("450e8400-e29b-41d4-a716-446655440002").expect("Invalid UUID"),
            name: "Savings Account".to_string(),
            balance: 500000, // $5,000.00
        },
    ]
}

fn journals() -> Vec<super::journal::Journal> {
    use std::str::FromStr;
    vec![
        super::journal::Journal {
            id: Uuid::from_str("550e8400-e29b-41d4-a716-446655440000").expect("Invalid UUID"),
            name: "Personal".to_string(),
            creator_username: "johndoe".to_string(),
            created_at: "2024-01-15 09:30:45".to_string(),
        },
        super::journal::Journal {
            id: Uuid::from_str("550e8400-e29b-41d4-a716-446655440001").expect("Invalid UUID"),
            name: "Business".to_string(),
            creator_username: "janesmith".to_string(),
            created_at: "2024-01-20 14:22:18".to_string(),
        },
    ]
}

#[component]
pub fn AccountListPage() -> impl IntoView {
    use leptos_router::hooks::use_params_map;

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
            {accounts()
                .into_iter()
                .map(|account| {
                    view! {
                        <a
                            href=format!("/journal/{}/account/{}", journal_id(), account.id)
                            class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                        >
                            <div class="flex justify-between items-center">
                                <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                                    {account.name}
                                </h3>
                                <div class="text-right">
                                    <div class="text-lg font-medium text-gray-900 dark:text-white">
                                        {format!(
                                            "${}.{:02}",
                                            account.balance / 100,
                                            account.balance % 100,
                                        )}
                                    </div>
                                </div>
                            </div>
                        </a>
                    }
                })
                .collect_view()}
            <hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600" />
            <div class="mt-10">
                <form class="space-y-6">
                    <div>
                        <label
                            for="account_name"
                            class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100"
                        >
                            "Create New Account"
                        </label>
                        <div class="mt-2">
                            <input
                                id="account_name"
                                type="text"
                                name="account_name"
                                required
                                class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                            />
                        </div>
                    </div>
                    <div>
                        <button
                            type="submit"
                            class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500"
                        >
                            "Create Account"
                        </button>
                    </div>
                </form>
            </div>
        </Layout>
    }
}
