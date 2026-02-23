use crate::BackendType;
use crate::StateType;
use crate::auth::user::{self};
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
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let content = html! {
        div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4" {
            @match state.journal_service.journal_list(user.id).await {
                Ok(journals) => {
                    @for (journal_id, journal_state) in journals {
                        a
                        href=(format! ("/journal/{}", journal_id))
                        class="self-start p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                            h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                (journal_state.name)
                            }

                            div class="mt-2 text-sm text-gray-600 dark:text-gray-400" {
                                "Created by "

                                @match state.user_service.user_get_email(journal_state.creator).await {
                                    Ok(Some(email)) => (email),
                                    Ok(None) => {"unknown user"},
                                    Err(e) => (format!("failed to fetch email: {:?}", e)),
                                }

                                " on "
                                (journal_state.created_at
                                    .with_timezone(&chrono_tz::America::Chicago)
                                    .format("%Y-%m-%d %H:%M:%S %Z")
                                )
                            }
                        }
                    }
                }

                Err(e) => {
                    div class="flex justify-center items-center h-full" {
                        p class="text-gray-500 dark:text-gray-400" {
                            (format!("Failed to fetch journals: {:?}", e))
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
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let journal_state_res = match JournalId::from_str(&id) {
        Ok(s) => state.journal_service.journal_get(s, user.id).await,
        Err(e) => Err(e),
    };

    let content = html! {
        div class="flex flex-col gap-6" {
            @match &journal_state_res {
                Ok(Some(journal_state)) => {
                    div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4" {
                        a
                        href=(format!("/journal/{}/transaction", &id))
                        class="self-start p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                            h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                "Transactions"
                            }
                        }

                        a
                        href=(format!("/journal/{}/account", &id))
                        class="self-start p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                            h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                "Accounts"
                            }
                        }

                        a
                        href=(format!("/journal/{}/person", &id))
                        class="self-start p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                            h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                "People"
                            }
                        }

                        a
                        href=(format!("/journal/{}/subjournals", &id))
                        class="self-start p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                            h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                "Subjournals"
                            }
                        }
                    }

                    div class="p-4 bg-gray-50 dark:bg-gray-800 rounded-lg" {
                        div class="space-y-2" {
                            div class="text-sm text-gray-600 dark:text-gray-400" {
                                "Created by "

                                 @match state.user_service.user_get_email(journal_state.creator).await {
                                    Ok(Some(email)) => (email),
                                    Ok(None) => {"unknown user"},
                                    Err(e) => (format!("failed to fetch email: {:?}", e)),
                                }

                                " on "
                                (journal_state.created_at
                                    .with_timezone(&chrono_tz::America::Chicago)
                                    .format("%Y-%m-%d %H:%M:%S %Z")
                                )
                            }
                        }
                    }
                }

                Ok(None) => {
                    div class="flex justify-center items-center h-full" {
                        p class="text-gray-500 dark:text-gray-400" {
                            "Unknown journal"
                        }
                    }
                }

                Err(e) => {
                    div class="flex justify-center items-center h-full" {
                        p class="text-gray-500 dark:text-gray-400" {
                            (format!("Failed to fetch journal: {:?}", e))
                        }
                    }
                }

            }
        }
    };

    Ok(layout(
        Some(&journal_state_res.map(|s| s.map(|s| s.name)).or_unknown()),
        true,
        Some(&id),
        content,
    ))
}

pub async fn sub_journal_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let journal_state_res = match JournalId::from_str(&id) {
        Ok(s) => state.journal_service.journal_get(s, user.id).await,
        Err(e) => Err(e),
    };

    let content = html! {
        div class="flex flex-col gap-6" {
            @match &journal_state_res {
                Ok(Some(_journal_state)) => {
                    // TODO: fetch and display actual subjournals when the data model supports parent_journal_id
                    p class="text-sm text-gray-500 dark:text-gray-400" {
                        "No subjournals yet."
                    }

                    hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600";

                    div class="mt-10" {
                        div class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl p-6" {
                            h3 class="text-lg font-semibold text-gray-900 dark:text-white mb-6" {
                                "Create Subjournal"
                            }

                            // TODO: wire up to POST /journal/{id}/createsubjournal once backend logic is implemented
                            form method="post" action=(format!("/journal/{}/createsubjournal", id)) class="space-y-4" {
                                div {
                                    label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                        "Name"
                                    }
                                    input
                                        type="text"
                                        name="subjournal_name"
                                        placeholder="Subjournal name"
                                        required
                                        class="block w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400";
                                }

                                div class="flex justify-between items-center pt-4 border-t border-gray-200 dark:border-gray-600" {
                                    p class="text-sm text-gray-500 dark:text-gray-400" {
                                        "Subjournals let you scope transactions to a subset of this journal."
                                    }
                                    button
                                        type="submit"
                                        class="px-6 py-2 bg-indigo-600 text-white font-medium rounded-md hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-400 dark:ring-offset-gray-800" {
                                        "Create Subjournal"
                                    }
                                }
                            }
                        }
                    }
                }

                Ok(None) => {
                    div class="flex justify-center items-center h-full" {
                        p class="text-gray-500 dark:text-gray-400" {
                            "Unknown journal"
                        }
                    }
                }

                Err(e) => {
                    div class="flex justify-center items-center h-full" {
                        p class="text-gray-500 dark:text-gray-400" {
                            (format!("Failed to fetch journal: {:?}", e))
                        }
                    }
                }
            }
        }
    };

    let wrapped_content = html! {
        div class="flex flex-col gap-6 mx-auto w-full max-w-4xl" {
            (content)
        }
    };

    Ok(layout(
        Some(&journal_state_res.map(|s| s.map(|s| s.name)).or_unknown()),
        true,
        Some(&id),
        wrapped_content,
    ))
}
