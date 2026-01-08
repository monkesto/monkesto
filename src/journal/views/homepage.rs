use crate::auth::axum_login::AuthSession;
use crate::auth::user;
use crate::cuid::Cuid;
use crate::journal::layout::maud_layout;
use crate::journal::queries::{get_associated_journals, get_journal_owner};
use crate::known_errors::{KnownErrors, RedirectOnError, UrlError};
use axum::Extension;
use axum::extract::{Path, Query};
use axum::response::Redirect;
use maud::{Markup, html};
use sqlx::PgPool;
use std::str::FromStr;

#[allow(dead_code)]
pub struct Journal {
    pub id: Cuid,
    pub name: String,
    pub creator_username: String,
    pub created_at: String,
}

pub async fn journal_list(
    Extension(pool): Extension<PgPool>,
    session: AuthSession,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user_id = user::get_id(session)?;

    let journals_result = get_associated_journals(&user_id, &pool).await;

    let content = html! {
        @if let Ok(journals) = journals_result {
            @for (id, journal) in journals {
                a
                href=(format! ("/journal/{}", id))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                    h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                        (journal.get_name())
                    }
                }
            }
        }
        @else if let Err(e) = journals_result {
            p {
                 (format!("An error occurred while fetching journals: {:?}", e))
            }
        }

        div class="mt-10" {
            form action="/createjournal" method="post" class="space-y-6" {
                div {
                    label
                    for="journal_name"
                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                        "Create New Journal"
                    }

                    div class="mt-2" {
                        input
                        id="journal_name"
                        type="text"
                        name="journal_name"
                        required
                        class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                        ;
                    }
                }

                div {
                    button
                    type="submit"
                    class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500"{
                        "Create Journal"
                    }
                }
            }
        }

        @if let Some(e) = err.err {
            p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                (format! ("error: {:?}", KnownErrors::decode(&e)))
            }
        }
    };

    Ok(maud_layout(None, false, None, content))
}

pub async fn journal_detail(
    Extension(pool): Extension<PgPool>,
    session: AuthSession,
    Path(id): Path<String>,
) -> Result<Markup, Redirect> {
    let user_id = session
        .user
        .ok_or(KnownErrors::NotLoggedIn)
        .or_redirect("/login")?
        .id;

    let journal_id = Cuid::from_str(&id);

    let journals = get_associated_journals(&user_id, &pool).await;

    let journal_res = journals
        .ok()
        .and_then(|journals| journals.get(&journal_id.unwrap_or_default()).cloned());

    let content = html! {
        @if let Some(journal) = &journal_res {
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
                        @match get_journal_owner(&id, &pool).await {
                            Err(e) => (format!("error: {:?}", e)),
                            Ok(None) => "unknown user",
                            Ok(Some(s)) => (s),
                        }
                        " on "
                        (journal
                            .get_created_at()
                            .with_timezone(&chrono_tz::America::Chicago)
                            .format("%Y-%m-%d %H:%M:%S %Z")
                        )
                    }
                }
            }
        }
    };

    Ok(maud_layout(
        Some(&journal_res.map_or_else(|| "unknown journal".to_string(), |j| j.get_name())),
        true,
        Some(&id),
        content,
    ))
}
