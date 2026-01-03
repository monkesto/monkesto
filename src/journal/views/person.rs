use crate::cuid::Cuid;
use crate::journal::layout::maud_layout;
use crate::known_errors::{KnownErrors, RedirectOnError, UrlError};
use crate::{auth, extensions};
use axum::Extension;
use axum::extract::{Path, Query};
use axum::response::Redirect;
use maud::{Markup, html};
use sqlx::PgPool;
use tower_sessions::Session;

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

pub async fn people_list_page(
    Extension(pool): Extension<PgPool>,
    session: Session,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let session_id = extensions::intialize_session(&session)
        .await
        .or_redirect_clean("/login")?;

    let user_id = auth::get_user_id(&session_id, &pool)
        .await
        .or_redirect_clean("/login")?;

    let journals = crate::journal::queries::get_associated_journals(&user_id, &pool).await;

    let journal_name = journals
        .ok()
        .map(|journals| {
            journals
                .associated
                .into_iter()
                .find(|j| j.get_id().to_string() == id)
                .map(|j| j.get_name())
                .unwrap_or("unknown journal".to_string())
        })
        .unwrap_or("unknown journal".to_string());

    let content = html! {
        @for person in people() {
            a
            href=(format!("/journal/{}/person/{}", id, person.id))
            class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                    (person.username)
                }
            }
        }

        hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600";

        div class="mt-10" {
            form class="space-y-6" {
                div {
                    label
                    for="username"
                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                        "Invite Person"
                    }

                    div class="mt-2" {
                        input
                        id="username"
                        type="text"
                        name="username"
                        required
                        placeholder="Enter username to invite"
                        class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                        ;
                    }
                }

                div {
                    button
                    type="submit"
                    class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                        "Send Invite"
                    }
                }
            }

            @if let Some(e) = err.err {
                p {
                    (format!("An error occurred: {:?}", KnownErrors::decode(&e)))
                }
            }
        }
    };

    Ok(maud_layout(Some(&journal_name), true, Some(&id), content))
}
