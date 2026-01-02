use crate::auth;
use crate::cuid::Cuid;
use crate::extensions;
use crate::journal::layout::maud_layout;
use crate::journal::queries::{get_associated_journals, get_journal_owner};
use crate::known_errors::{KnownErrors, RedirectOnError, UrlError};
use axum::Extension;
use axum::extract::{Path, Query};
use axum::response::Redirect;
use maud::{Markup, html};
use sqlx::PgPool;
use tower_sessions::Session;

#[allow(dead_code)]
pub struct Journal {
    pub id: Cuid,
    pub name: String,
    pub creator_username: String,
    pub created_at: String,
}

pub async fn journal_list(
    Extension(pool): Extension<PgPool>,
    session: Session,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let session_id = extensions::intialize_session(&session)
        .await
        .or_redirect(KnownErrors::SessionIdNotFound, "/login")?;

    let user_id = auth::get_user_id(&session_id, &pool)
        .await
        .or_redirect(KnownErrors::NotLoggedIn, "/login")?;

    let journals_result = get_associated_journals(&user_id, &pool).await;

    let content = html! {
        @if let Ok(journals) = journals_result {
            @for journal in journals.associated {
                a
                href=(format! ("/journal/{}", journal.get_id()))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                    h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                        (journal.get_name())
                    }
                }
            }
        }
        @else if let Err(e) = journals_result {
            p {
                "An error occurred while fetching journals: " (e)
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
    session: Session,
    Path(id): Path<String>,
) -> Result<Markup, Redirect> {
    let session_id = extensions::intialize_session(&session)
        .await
        .or_redirect(KnownErrors::SessionIdNotFound, "/login")?;

    let user_id = auth::get_user_id(&session_id, &pool)
        .await
        .or_redirect(KnownErrors::NotLoggedIn, "/login")?;

    let journal_result = get_associated_journals(&user_id, &pool)
        .await
        .ok()
        .and_then(|journals| {
            journals
                .associated
                .into_iter()
                .find(|journal| journal.get_id().to_string() == id)
        });

    let content = html! {
        @if let Some(journal) = &journal_result {
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
                            Err(e) => (format!("error: {}", e)),
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

    let page_title = journal_result
        .map(|j| j.get_name())
        .unwrap_or("unknown journal".to_string());

    Ok(maud_layout(Some(&page_title), true, Some(&id), content))
}
