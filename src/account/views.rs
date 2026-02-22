use crate::BackendType;
use crate::StateType;
use crate::account::AccountState;
use crate::auth::user;
use crate::ident::AccountId;
use crate::ident::Ident;
use crate::ident::JournalId;
use crate::journal::JournalNameOrUnknown;
use crate::journal::layout::layout;
use crate::known_errors::KnownErrors;
use crate::known_errors::UrlError;
use crate::service::Service;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::Redirect;
use axum_login::AuthSession;
use maud::Markup;
use maud::html;
use std::str::FromStr;

#[expect(dead_code)]
struct AccountItem {
    pub id: Ident,
    pub name: String,
    pub balance: i64, // in cents
}

fn render_account_tree(
    accounts: &[(AccountId, AccountState)],
    parent_id: Option<AccountId>,
    depth: usize,
    journal_id: &str,
) -> Markup {
    let indent_class = match depth {
        0 => "",
        1 => "ml-6",
        2 => "ml-12",
        _ => "ml-16",
    };
    html! {
        @for (acc_id, acc) in accounts.iter().filter(|(_, a)| a.parent_account_id == parent_id) {
            @if depth == 0 {
                a
                href=(format!("/journal/{}/account/{}", journal_id, acc_id))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                    div class="flex justify-between items-center" {
                        h3 class="text-lg font-semibold text-gray-900 dark:text-white" { (acc.name) }
                        @let balance = acc.balance.abs();
                        div class="text-right" {
                            div class="text-lg font-medium text-gray-900 dark:text-white" {
                                (format!("${}.{:02} {}", balance / 100, balance % 100, if acc.balance < 0 { "Dr" } else { "Cr" }))
                            }
                        }
                    }
                }
            } @else {
                div class=(indent_class) {
                    a
                    href=(format!("/journal/{}/account/{}", journal_id, acc_id))
                    class="flex items-center gap-2 p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                        span class="text-gray-400 dark:text-gray-500 select-none" { "↳" }
                        div class="flex justify-between items-center flex-1" {
                            h3 class="text-base font-medium text-gray-800 dark:text-gray-200" { (acc.name) }
                            @let balance = acc.balance.abs();
                            div class="text-right" {
                                div class="text-base font-medium text-gray-900 dark:text-white" {
                                    (format!("${}.{:02} {}", balance / 100, balance % 100, if acc.balance < 0 { "Dr" } else { "Cr" }))
                                }
                            }
                        }
                    }
                }
            }
            (render_account_tree(accounts, Some(*acc_id), depth + 1, journal_id))
        }
    }
}

pub(crate) fn render_account_options(
    accounts: &[(AccountId, AccountState)],
    parent_id: Option<AccountId>,
    depth: usize,
) -> Markup {
    let prefix = "↳ ".repeat(depth);
    html! {
        @for (acc_id, acc) in accounts.iter().filter(|(_, a)| a.parent_account_id == parent_id) {
            option value=(acc_id) { (format!("{}{}", prefix, acc.name)) }
            (render_account_options(accounts, Some(*acc_id), depth + 1))
        }
    }
}

pub async fn account_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let journal_id_res = JournalId::from_str(&id);

    let content = html! {
        @if let Ok(journal_id) = journal_id_res {
            @match state.account_get_all_in_journal(journal_id, user.id).await {
                Ok(accounts) => {
                    (render_account_tree(&accounts, None, 0, &id))
                }

                Err(e) => {
                    div class="flex justify-center items-center h-full" {
                        p class="text-gray-500 dark:text-gray-400" {
                            (format!("Failed to fetch accounts: {:?}", e))
                        }
                    }
                }
            }
        }
        @else {
            div class="flex justify-center items-center h-full" {
                p class="text-gray-500 dark:text-gray-400" {
                    "Invalid journal Id"
                }
            }
        }

        hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600";

        div class="mt-10" {
            form action=(format!("/journal/{}/createaccount", id)) method="post" class="space-y-4" {
                h3 class="text-base font-semibold text-gray-900 dark:text-gray-100" { "Create New Account" }

                div {
                    label
                    for="account_name"
                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                        "Name"
                    }

                    div class="mt-2" {
                        input
                        id="account_name"
                        type="text"
                        name="account_name"
                        required
                        class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                        ;
                    }
                }

                @if let Ok(journal_id) = journal_id_res && let Ok(accounts) = state.account_get_all_in_journal(journal_id, user.id).await {
                    div {
                        label
                        for="parent_account_id"
                        class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                            "Parent Account (optional)"
                        }
                        div class="mt-2" {
                            select
                            id="parent_account_id"
                            name="parent_account_id"
                            class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:focus:outline-indigo-500" {
                                option value="" { "None" }
                                (render_account_options(&accounts, None, 0))
                            }
                        }
                    }
                }

                div {
                    button
                    type="submit"
                    class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                        "Create Account"
                    }
                }
            }
        }


        @if let Some(e) = err.err {
            p {
                (format!("An error occurred: {:?}", KnownErrors::decode(&e)))
            }
        }
    };

    let wrapped_content = html! {
        div class="flex flex-col gap-6 mx-auto w-full max-w-4xl" {
            (content)
        }
    };

    Ok(layout(
        Some(
            &state
                .journal_get_name_from_res(journal_id_res)
                .await
                .or_unknown(),
        ),
        true,
        Some(&id),
        wrapped_content,
    ))
}
