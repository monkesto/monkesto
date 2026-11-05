use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::journal::JournalId;
use crate::journal::error::JournalError;
use crate::journal::layout::layout;
use crate::{BackendType, StateType};
use axum::extract::{Path, State};
use axum_login::AuthSession;
use maud::{Markup, html};
use std::str::FromStr;

pub async fn donor_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
) -> Result<Markup, JournalError> {
    let user = get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&id)?;

    let content = html! {
        @match state.journal_service.list_journal_donors(journal_id, authority).await {
                Ok(journal_donors) => {
                     @for (donor, _, _) in journal_donors {
                        a
                        href=(format!("/journal/{}/donor/{}", journal_id, donor.id))
                        class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                            div class="flex justify-between items-center" {
                                h3 class="text-lg font-semibold text-gray-900 dark:text-white" { (donor.name) }

                                div class="text-right" {
                                    div class="text-lg font-medium text-gray-900 dark:text-white" {
                                        "Donated " (Dollars(donor.total_donated as i64))
                                    }
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    p {
                        "failed to get the donors for " (journal_id) ": " (e)
                    }
                }
            }

        hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600";

        div class="mt-10" {
            form action=(format!("/journal/{}/createdonor", id)) method="post" class="space-y-4" {
                h3 class="text-base font-semibold text-gray-900 dark:text-gray-100" { "Add Donor" }

                div {
                    label
                    for="donor_name"
                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                        "Name"
                    }

                    div class="mt-2" {
                        input
                        id="donor_name"
                        type="text"
                        name="donor_name"
                        required
                        class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                        ;
                    }
                }

                div {
                    button
                    type="submit"
                    class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                        "Add Donor"
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

    let journal_name = state
        .journal_service
        .get_journal(journal_id, authority)
        .await
        .map(|(j, _, _)| j.name.to_string())
        .unwrap_or_else(|e| format!("failed to fetch the journal name: {e}"));

    Ok(layout(
        Some(&journal_name),
        true,
        Some(&id),
        wrapped_content,
    ))
}
