use super::homepage::Journal;
use crate::cuid::Cuid;
use crate::journal::layout::maud_layout;
use crate::journal::queries::get_associated_journals;
use crate::known_errors::{KnownErrors, RedirectOnError, UrlError};
use crate::{auth, extensions};
use axum::Extension;
use axum::extract::{Path, Query};
use axum::response::Redirect;
use maud::{Markup, html};
use sqlx::PgPool;
use tower_sessions::Session;

struct AccountItem {
    pub id: Cuid,
    pub name: String,
    pub balance: i64, // in cents
}

fn accounts() -> Vec<AccountItem> {
    vec![
        AccountItem {
            id: Cuid::from_str("aaaaaaaaab").expect("Invalid CUID"),
            name: "Cash".to_string(),
            balance: 25043, // $250.43
        },
        AccountItem {
            id: Cuid::from_str("aaaaaaaaac").expect("Invalid CUID"),
            name: "Checking Account".to_string(),
            balance: 152067, // $1,520.67
        },
        AccountItem {
            id: Cuid::from_str("aaaaaaaaad").expect("Invalid CUID"),
            name: "Savings Account".to_string(),
            balance: 500000, // $5,000.00
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

pub async fn account_list_page(
    Extension(pool): Extension<PgPool>,
    session: Session,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let session_id = extensions::intialize_session(&session)
        .await
        .or_redirect(KnownErrors::SessionIdNotFound, "/login")?;
    let user_id = auth::get_user_id(&session_id, &pool)
        .await
        .or_redirect(KnownErrors::NotLoggedIn, "/login")?;
    let journals = get_associated_journals(&user_id, &pool).await;
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
        @for account in accounts() {
            a
            href=(format!("/journal/{}/account/{}", id, account.id.to_string()))
            class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                div
                class="flex justify-between items-center" {
                    h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                        (account.name)
                    }
                    div class="text-right" {
                        div class="text-lg font-medium text-gray-900 dark:text-white" {
                            (format!("${}.{:02}", account.balance / 100, account.balance % 100))
                        }
                    }
                }
            }
        }

        hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600";

        div class="mt-10" {
            form class="space-y-6" {
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

            @if let Some(e) = err.err {
                p {
                    (format!("An error occurred: {:?}", KnownErrors::decode(&e)))
                }
            }
        }
    };

    Ok(maud_layout(Some(&journal_name), true, Some(&id), content))
}
