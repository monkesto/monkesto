use crate::AppState;
use crate::AuthSession;
use crate::auth::user::UserStore;
use crate::auth::user::{self};
use crate::ident::Ident;
use crate::ident::JournalId;
use crate::journal::JournalStore;
use crate::journal::Permissions;
use crate::journal::layout::layout;
use crate::known_errors::KnownErrors;
use crate::known_errors::RedirectOnError;
use crate::known_errors::UrlError;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::Redirect;
use maud::Markup;
use maud::html;
use std::str::FromStr;

#[expect(dead_code)]
pub struct Journal {
    pub id: Ident,
    pub name: String,
    pub creator_username: String,
    pub created_at: String,
}

pub async fn journal_list(
    State(state): State<AppState>,
    session: AuthSession,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let journals = state
        .journal_store
        .get_user_journals(&user.id)
        .await
        .or_redirect("/journal")?;

    let content = html! {
        div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4" {
            @for journal_id in journals {
                @let journal_state = state.journal_store.get_journal(&journal_id).await.ok();
                a
                href=(format! ("/journal/{}", journal_id))
                class="self-start p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                    h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                        (state.journal_store.get_name(&journal_id).await.unwrap_or_else(|_| "unknown journal".to_string()))
                    }
                    @if let Some(journal) = journal_state {
                        div class="mt-2 text-sm text-gray-600 dark:text-gray-400" {
                            "Created by "
                            (state.user_store.get_user_email(&journal.creator).await.map(|e| e.to_string()).unwrap_or_else(|_| "unknown user".to_string()))
                            " on "
                            (journal.created_at
                                .with_timezone(&chrono_tz::America::Chicago)
                                .format("%Y-%m-%d %H:%M:%S %Z")
                            )
                        }
                    }
                }
            }

            form action="/createjournal" method="post" class="self-start rounded-xl transition-colors space-y-4" {
                h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                    "Create New Journal"
                }

                div {
                    input
                    id="journal_name"
                    type="text"
                    name="journal_name"
                    placeholder="Journal name"
                    required
                    class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                    ;
                }

                button
                type="submit"
                class="w-full rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500"{
                    "Create"
                }
            }
        }

        @if let Some(e) = err.err {
            p class="mt-6 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                (format! ("error: {:?}", KnownErrors::decode(&e)))
            }
        }
    };

    Ok(layout(None, false, None, content))
}

pub async fn journal_detail(
    State(state): State<AppState>,
    session: AuthSession,
    Path(id): Path<String>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let journal_state_res = match JournalId::from_str(&id) {
        Ok(s) => state.journal_store.get_journal(&s).await,
        Err(e) => Err(e),
    };

    let content = html! {
        div class="flex flex-col gap-6 mx-auto w-full max-w-4xl" {
            @if let Ok(journal_state) = &journal_state_res && journal_state.get_user_permissions(&user.id).contains(Permissions::READ) {

                a
                href=(format!("/journal/{}/transaction", &id))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                    h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                        "Transactions"
                    }
                }

                a
                href=(format!("/journal/{}/account", &id))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                    h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                        "Accounts"
                    }
                }

                a
                href=(format!("/journal/{}/person", &id))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                    h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                        "People"
                    }
                }

                div class="mt-6 p-4 bg-gray-50 dark:bg-gray-800 rounded-lg" {
                    div class="space-y-2" {
                        div class="text-sm text-gray-600 dark:text-gray-400" {
                            "Created by "
                            (state.user_store.get_user_email(&journal_state.creator).await.map(|e| e.to_string()).unwrap_or_else(|_| "unknown user".to_string()).to_string())
                            " on "
                            (journal_state.created_at
                                .with_timezone(&chrono_tz::America::Chicago)
                                .format("%Y-%m-%d %H:%M:%S %Z")
                            )
                        }
                    }
                }
            }
            @else {
                div class="flex justify-center items-center h-full" {
                    p class="text-gray-500 dark:text-gray-400" {
                        "Invalid journal"
                    }
                }
            }
        }
    };

    Ok(layout(
        Some(
            &journal_state_res
                .map(|s| s.name)
                .unwrap_or_else(|_| "unknown journal".to_string()),
        ),
        true,
        Some(&id),
        content,
    ))
}
