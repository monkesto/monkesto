use crate::AppState;
use crate::auth::axum_login::AuthSession;
use crate::auth::user;
use crate::cuid::Cuid;
use crate::journal::Permissions;
use crate::journal::layout::maud_layout;
use crate::known_errors::{KnownErrors, UrlError};
use axum::extract::{Path, Query, State};
use axum::response::Redirect;
use maud::{Markup, html};
use std::str::FromStr;

pub async fn people_list_page(
    State(state): State<AppState>,
    session: AuthSession,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user_id = user::get_id(session)?;

    let journal_state_res = match Cuid::from_str(&id) {
        Ok(s) => state.journal_store.get_journal(&s).await,
        Err(e) => Err(e),
    };

    let content = html! {
        @if let Ok(journal_state) = &journal_state_res && journal_state.tenants.get(&user_id).is_some_and(|p| p.tenant_permissions.contains(Permissions::READ)) {
            @for (tenant_id, _) in journal_state.tenants.clone() {
                a
                href=(format!("/journal/{}/person/{}", id, tenant_id))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                    h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                        @match state.user_store.get_email(&user_id).await {
                            Ok(email) => (email),
                            Err(e) => (format!("An error occurred while fetching username: {}", e)),
                        }
                    }
                }
            }
        }
        @else {
            div class="flex justify-center items-center h-full" {
                p class="text-gray-500 dark:text-gray-400" {
                    "Invalid journal id"
                }
            }
        }

        hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600";

        div class="mt-10" {
            form method="post" action=(format!("/journal/{}/invite", id)) class="space-y-6"  {
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

    Ok(maud_layout(
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
