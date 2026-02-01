use crate::AppState;
use crate::AuthSession;
use crate::ident::{JournalId, UserId};
use crate::journal::JournalStore;
use crate::journal::Permissions;
use crate::journal::layout::maud_layout;
use crate::known_errors::{KnownErrors, RedirectOnError, UrlError};
use crate::webauthn::user::{self, UserStore};
use axum::extract::{Path, Query, State};
use axum::response::Redirect;
use maud::{Markup, html};
use std::str::FromStr;

pub async fn person_detail_page(
    State(state): State<AppState>,
    session: AuthSession,
    Path((id, person_id)): Path<(String, String)>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;
    let journal_id = JournalId::from_str(&id).or_redirect(&format!("/journal/{}/person", id))?;
    let target_user_id =
        UserId::from_str(&person_id).or_redirect(&format!("/journal/{}/person", id))?;

    let journal_state = state
        .journal_store
        .get_journal(&journal_id)
        .await
        .or_redirect(&format!("/journal/{}/person", id))?;

    if !journal_state
        .get_user_permissions(&user.id)
        .contains(Permissions::READ)
    {
        return Err(KnownErrors::PermissionError {
            required_permissions: Permissions::READ,
        }
        .redirect("/journal"));
    }

    let tenant_info = journal_state.tenants.get(&target_user_id);
    let is_owner = journal_state.owner == target_user_id;

    let target_email = state
        .user_store
        .get_user_email(&target_user_id)
        .await
        .unwrap_or_else(|_| "Unknown".to_string());

    let content = html! {
        div class="max-w-2xl mx-auto py-8 px-4" {
            div class="flex justify-between items-center mb-8" {
                h2 class="text-2xl font-bold text-gray-900 dark:text-white" { (target_email) }
                @if is_owner {
                    span class="inline-flex items-center rounded-md bg-indigo-50 dark:bg-indigo-900/30 px-2 py-1 text-xs font-medium text-indigo-700 dark:text-indigo-300 ring-1 ring-inset ring-indigo-700/10 dark:ring-indigo-400/30" { "Owner" }
                }
            }

            @if let Some(info) = tenant_info {
                div class="bg-white dark:bg-gray-800 shadow sm:rounded-lg overflow-hidden border border-gray-200 dark:border-gray-700" {
                    div class="px-4 py-5 sm:p-6" {
                        h3 class="text-base font-semibold text-gray-900 dark:text-white mb-4" { "Permissions" }

                        form method="post" action=(format!("/journal/{}/person/{}/update", id, person_id)) class="space-y-4" {
                            div class="space-y-4" {
                                (permission_checkbox("read", "Read Access", info.tenant_permissions.contains(Permissions::READ)))
                                (permission_checkbox("addaccount", "Add Accounts", info.tenant_permissions.contains(Permissions::ADDACCOUNT)))
                                (permission_checkbox("appendtransaction", "Append Transactions", info.tenant_permissions.contains(Permissions::APPENDTRANSACTION)))
                                (permission_checkbox("invite", "Invite Users", info.tenant_permissions.contains(Permissions::INVITE)))
                                (permission_checkbox("delete", "Delete Journal", info.tenant_permissions.contains(Permissions::DELETE)))
                            }

                            div class="mt-6 flex items-center justify-end gap-x-6" {
                                button
                                type="submit"
                                class="rounded-md bg-indigo-600 px-3 py-2 text-sm font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:hover:bg-indigo-400" {
                                    "Update Permissions"
                                }
                            }
                        }
                    }
                }

                div class="mt-8 bg-white dark:bg-gray-800 shadow sm:rounded-lg overflow-hidden border border-red-200 dark:border-red-900/30" {
                    div class="px-4 py-5 sm:p-6" {
                        h3 class="text-base font-semibold text-red-600 dark:text-red-400 mb-4" { "Danger Zone" }
                        p class="text-sm text-gray-500 dark:text-gray-400 mb-4" { "Removing this user will immediately revoke their access to this journal." }
                        form method="post" action=(format!("/journal/{}/person/{}/remove", id, person_id)) {
                            button
                            type="submit"
                            class="rounded-md bg-red-600 px-3 py-2 text-sm font-semibold text-white shadow-xs hover:bg-red-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-red-600 dark:bg-red-500 dark:hover:bg-red-400" {
                                "Remove User from Journal"
                            }
                        }
                    }
                }
            }
            @else if !is_owner {
                 div class="bg-yellow-50 dark:bg-yellow-900/30 border-l-4 border-yellow-400 p-4" {
                    div class="flex" {
                        div class="ml-3" {
                            p class="text-sm text-yellow-700 dark:text-yellow-200" {
                                "This user is no longer a tenant of this journal."
                            }
                        }
                    }
                }
            }

            @if let Some(e) = err.err {
                div class="mt-6 bg-red-50 dark:bg-red-900/30 border-l-4 border-red-400 p-4" {
                    p class="text-sm text-red-700 dark:text-red-200" {
                        (format!("An error occurred: {:?}", KnownErrors::decode(&e)))
                    }
                }
            }
        }
    };

    Ok(maud_layout(
        Some(&journal_state.name),
        true,
        Some(&id),
        content,
    ))
}

fn permission_checkbox(name: &'static str, label: &'static str, checked: bool) -> Markup {
    html! {
        div class="relative flex items-start" {
            div class="flex h-6 items-center" {
                input
                id=(name)
                name=(name)
                type="checkbox"
                checked[checked]
                class="h-4 w-4 rounded border-gray-300 text-indigo-600 focus:ring-indigo-600 dark:border-gray-700 dark:bg-gray-900 dark:ring-offset-gray-900"
                ;
            }
            div class="ml-3 text-sm/6" {
                label for=(name) class="font-medium text-gray-900 dark:text-white" { (label) }
            }
        }
    }
}

pub async fn people_list_page(
    State(state): State<AppState>,
    session: AuthSession,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let journal_state_res = match JournalId::from_str(&id) {
        Ok(s) => state.journal_store.get_journal(&s).await,
        Err(e) => Err(e),
    };

    let content = html! {
        @if let Ok(journal_state) = &journal_state_res && journal_state.get_user_permissions(&user.id).contains(Permissions::READ) {
            @for (tenant_id, _) in journal_state.tenants.clone() {
                a
                href=(format!("/journal/{}/person/{}", id, tenant_id))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                    h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                        @match state.user_store.get_user_email(&tenant_id).await {
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
                    for="email"
                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                        "Invite Person"
                    }

                    div class="mt-2" {
                        input
                        id="email"
                        type="text"
                        name="email"
                        required
                        placeholder="Enter email to invite"
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
