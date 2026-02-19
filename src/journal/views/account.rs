use crate::appstate::AppState;
use crate::auth::user;
use crate::auth::user::User;
use crate::ident::Ident;
use crate::ident::JournalId;
use crate::journal::JournalNameOrUnknown;
use crate::journal::layout::layout;
use crate::known_errors::KnownErrors;
use crate::known_errors::UrlError;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::Redirect;
use axum_login::AuthSession;
use axum_login::AuthUser;
use axum_login::AuthnBackend;
use maud::Markup;
use maud::html;
use std::str::FromStr;

struct AccountItem {
    pub id: Ident,
    pub name: String,
    pub balance: i64, // in cents
}

pub async fn account_list_page<S, T>(
    State(state): State<S>,
    session: AuthSession<T>,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect>
where
    S: AppState,
    T: AuthnBackend<User = User>,
{
    let user = user::get_user(session)?;

    let journal_id_res = JournalId::from_str(&id);

    let content = html! {
        @if let Ok(journal_id) = journal_id_res {
            @match state.account_get_all_in_journal(journal_id, user.id()).await {
                Ok(accounts) => {
                    @for (acc_id, acc) in accounts {
                        a
                        href=(format!("/journal/{}/account/{}", id, acc_id.to_string()))
                        class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                            div
                            class="flex justify-between items-center" {
                                h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                    (acc.name)
                                }
                                @let balance = acc.balance.abs();
                                div class="text-right" {
                                    div class="text-lg font-medium text-gray-900 dark:text-white" {
                                        (format!("${}.{:02} {}", balance / 100, balance % 100, if acc.balance < 0 { "Dr" } else { "Cr" }))
                                    }
                                }
                            }
                        }
                    }
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
            form action=(format!("/journal/{}/createaccount", id)) method="post" class="space-y-6" {
                div {
                    label
                    for="account_name"
                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                        "Create New Account"
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
