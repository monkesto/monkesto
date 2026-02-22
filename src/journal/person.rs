use crate::BackendType;
use crate::StateType;
use crate::auth::user::{self};
use crate::authority::UserId;
use crate::ident::JournalId;
use crate::journal::JournalNameOrUnknown;
use crate::journal::Permissions;
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

// TODO: Fix This! Super messy and hard to work with.
pub async fn person_detail_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((id, person_id)): Path<(String, String)>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let journal_id_res = JournalId::from_str(&id);
    let target_user_id_res = UserId::from_str(&person_id);

    let journal_id = match journal_id_res {
        Ok(jid) => jid,
        Err(_) => {
            return Ok(layout(
                None,
                true,
                None,
                html! {
                    div class="max-w-2xl mx-auto py-8 px-4" {
                        div class="bg-red-50 dark:bg-red-900/30 border-l-4 border-red-400 p-4" {
                            p class="text-sm text-red-700 dark:text-red-200" {
                                "Invalid journal ID"
                            }
                        }
                    }
                },
            ));
        }
    };

    let target_user_id = match target_user_id_res {
        Ok(tuid) => tuid,
        Err(_) => {
            return Ok(layout(
                None,
                true,
                Some(&id),
                html! {
                    div class="max-w-2xl mx-auto py-8 px-4" {
                        div class="bg-red-50 dark:bg-red-900/30 border-l-4 border-red-400 p-4" {
                            p class="text-sm text-red-700 dark:text-red-200" {
                                "Invalid person ID"
                            }
                        }
                    }
                },
            ));
        }
    };

    let journal_state_res = state.journal_get(journal_id, user.id).await;

    let journal_state = match journal_state_res {
        Ok(Some(js)) => js,
        Ok(None) => {
            return Ok(layout(
                None,
                true,
                None,
                html! {
                    div class="max-w-2xl mx-auto py-8 px-4" {
                        div class="bg-red-50 dark:bg-red-900/30 border-l-4 border-red-400 p-4" {
                            p class="text-sm text-red-700 dark:text-red-200" {
                                "Journal not found"
                            }
                        }
                    }
                },
            ));
        }
        Err(e) => {
            return Ok(layout(
                None,
                true,
                None,
                html! {
                    div class="max-w-2xl mx-auto py-8 px-4" {
                        div class="bg-red-50 dark:bg-red-900/30 border-l-4 border-red-400 p-4" {
                            p class="text-sm text-red-700 dark:text-red-200" {
                                (format!("Error loading journal: {}", e))
                            }
                        }
                    }
                },
            ));
        }
    };

    if !journal_state
        .get_user_permissions(user.id)
        .contains(Permissions::READ)
    {
        return Ok(layout(
            Some(&journal_state.name),
            true,
            Some(&id),
            html! {
                div class="max-w-2xl mx-auto py-8 px-4" {
                    div class="bg-red-50 dark:bg-red-900/30 border-l-4 border-red-400 p-4" {
                        p class="text-sm text-red-700 dark:text-red-200" {
                            "You do not have permission to view this journal."
                        }
                    }
                }
            },
        ));
    }

    let tenant_info = journal_state.tenants.get(&target_user_id);
    let is_owner = journal_state.owner == target_user_id;

    let target_email = match state.user_get_email(target_user_id).await {
        Ok(Some(email)) => email,
        Ok(None) => "Unknown User".to_string(),
        Err(e) => format!("Error fetching email: {}", e),
    };

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

    let wrapped_content = html! {
        div class="flex flex-col gap-6 mx-auto w-full max-w-4xl" {
            (content)
        }
    };

    Ok(layout(
        Some(&journal_state.name),
        true,
        Some(&id),
        wrapped_content,
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
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let journal_id_res = JournalId::from_str(&id);

    let content = html! {
        @if let Ok(journal_id) = journal_id_res {
            @match state.journal_get_users(journal_id, user.id).await {
                Ok(users) => {
                    @for user_id in users {
                        a
                        href=(format!("/journal/{}/person/{}", id, user_id))
                        class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                            h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                @match state.user_get_email(user_id).await {
                                    Ok(Some(email)) => (email),
                                    Ok(None) => {"unknown user"},
                                    Err(e) => (format!("failed to fetch email: {:?}", e)),
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    div class="flex justify-center items-center h-full" {
                        p class="text-gray-500 dark:text-gray-400" {
                            (format!("An error occurred while fetching users: {}", e))
                        }
                    }
                }
            }
        } @else {
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

                div class="space-y-4" {
                    p class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                        "Permissions"
                    }
                    (permission_checkbox("read", "Read Access", true))
                    (permission_checkbox("addaccount", "Add Accounts", true))
                    (permission_checkbox("appendtransaction", "Append Transactions", true))
                    (permission_checkbox("invite", "Invite Users", false))
                    (permission_checkbox("delete", "Delete Journal", false))
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
