use crate::appstate::AppState;
use crate::auth::user;
use crate::ident::JournalId;
use crate::journal::layout;
use crate::journal::transaction::EntryType;
use crate::journal::JournalNameOrUnknown;
use crate::known_errors::KnownErrors;
use crate::known_errors::UrlError;
use crate::BackendType;
use crate::StateType;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::Redirect;
use axum_login::AuthSession;
use maud::html;
use maud::Markup;
use std::str::FromStr;

pub async fn transaction_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;

    let journal_id_res = JournalId::from_str(&id);

    let content = html! {
        @if let Ok(journal_id) = journal_id_res {
            @match state.transaction_get_all_in_journal(journal_id, user.id).await {
                Ok(transactions) => {
                    @for (transaction_id, transaction_state) in transactions {
                        a
                        href=(format!("/journal/{}/transaction/{}", id, transaction_id.to_string()))
                        class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                            div class="space-y-3" {
                                div class="space-y-2" {
                                    @for entry in transaction_state.updates {
                                        @let entry_amount = format!("${}.{:02}", entry.amount / 100, entry.amount % 100);

                                        div class="flex justify-between items-center" {
                                            span class="text-base font-medium text-gray-900 dark:text-white" {
                                                @match state.account_get_name(entry.account_id).await {
                                                    Ok(Some(name)) => (name),
                                                    Ok(None) => "Unknown Account",
                                                    Err(e) => (format! ("Failed to get account name: {}", e)),
                                                }
                                            }

                                            span class="text-base text-gray-700 dark:text-gray-300" {
                                                (entry_amount) " " (entry.entry_type)
                                            }
                                        }
                                    }

                                    div class="text-xs text-gray-400 dark:text-gray-500" {
                                        @match state.user_get_email(transaction_state.author).await {
                                            Ok(Some(email)) => (email),
                                            Ok(None) => "Unknown User",
                                            Err(e) => (format! ("Failed to get user: {}", e)),
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    div class="flex justify-center items-center h-full" {
                        p class="text-gray-500 dark:text-gray-400" {
                            (format!("Failed to fetch transactions: {:?}", e))
                        }
                    }
                }
            }
        }

        hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600";

        div class="mt-10" {
            div class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl p-6" {
                h3 class="text-lg font-semibold text-gray-900 dark:text-white mb-6" {
                    "Create New Transaction"
                }
            }

            @if let Ok(journal_id) = journal_id_res && let Ok(accounts) = state.account_get_all_in_journal(journal_id, user.id).await {
                form method="post" action=(format!("/journal/{}/transaction", id)) class="space-y-6" {
                    @for i in 0..4 {
                        div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg" {
                            div class="space-y-3 md:space-y-0 md:grid md:grid-cols-12 md:gap-3" {
                                div class="md:col-span-6" {
                                    label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                        (if i < 2 {"Account"} else {"Account (Optional)"})
                                    }
                                    select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                    name="account" {
                                        option value="" { "Select account..." }
                                        @for (acc_id, acc_state) in accounts.iter() {
                                            option value=(acc_id) { (acc_state.name) }
                                        }
                                    }
                                }
                                div class="grid grid-cols-4 gap-3 md:col-span-6 md:grid-cols-6" {
                                    div class="col-span-3 md:col-span-4" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            "Amount"
                                        }
                                        input class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]"
                                        type="number"
                                        step="0.01" min="0"
                                        placeholder="0.00"
                                        required[i < 2]
                                        name="amount";
                                    }
                                    div class="col-span-1 md:col-span-2" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            "Type"
                                        }
                                        select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                        name="entry_type" {
                                            option value=(EntryType::Debit) { "Dr" }
                                            option value=(EntryType::Credit) { "Cr" }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div class="flex justify-between items-center pt-4 border-t border-gray-200 dark:border-gray-600" {
                        div class="text-sm text-gray-500 dark:text-gray-400" {
                            "Debits must equal credits"
                        }
                        button class="px-6 py-2 bg-indigo-600 text-white font-medium rounded-md hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-400 dark:ring-offset-gray-800" type="submit" {
                            "Create Transaction"
                        }
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

    Ok(layout::layout(
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
