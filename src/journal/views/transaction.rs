use crate::AppState;
use crate::AuthSession;
use crate::ident::JournalId;
use crate::journal::JournalStore;
use crate::journal::layout;
use crate::journal::transaction::EntryType;
use crate::known_errors::KnownErrors;
use crate::known_errors::UrlError;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::Redirect;
use axum_login::AuthnBackend;
use maud::Markup;
use maud::html;
use std::str::FromStr;

#[expect(unused_variables)]
pub async fn transaction_list_page(
    State(state): State<AppState>,
    session: AuthSession,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let journal_state_res = match JournalId::from_str(&id) {
        Ok(s) => state.journal_store.get_journal(&s).await,
        Err(e) => Err(e),
    };

    let content = html! {
        @if let Ok(ref journal_state) = journal_state_res {
            @for transaction_id in journal_state.transactions.iter() {
                a
                href=(format!("/journal/{}/transaction/{}", id, transaction_id.to_string()))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                    div class="space-y-3" {
                        div class="space-y-2" {
                            @match state.journal_store.get_transaction_state(transaction_id).await {
                                Ok(transaction) => {
                                    @for entry in transaction.updates {
                                        @let entry_amount = format!("${}.{:02}", entry.amount / 100, entry.amount % 100);

                                        div class="flex justify-between items-center" {
                                            span class="text-base font-medium text-gray-900 dark:text-white" {
                                                @match journal_state.accounts.get(&entry.account_id) {
                                                    Some(account) => (account.name),
                                                    None => "Unknown Account"
                                                }
                                            }

                                            span class="text-base text-gray-700 dark:text-gray-300" {
                                                (entry_amount) " " (entry.entry_type)
                                            }
                                        }
                                    }

                                    div class="text-xs text-gray-400 dark:text-gray-500" {
                                        @match state.user_store.get_user(&transaction.author).await {
                                            Ok(Some(user)) => (user.email),
                                            Ok(None) => "Unknown User",
                                            Err(e) => (format! ("Failed to get user: {}", e)),
                                        }
                                    }
                                }
                                Err(e) => {
                                    (format!("Failed to get transaction {}: {}", transaction_id, e))
                                }
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

                    form method="post" action=(format!("/journal/{}/transaction", id)) class="space-y-6" {
                        @for i in 0..4 {
                            div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3" {
                                div {
                                    label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                        (if i < 2 {"Account"} else {"Account (Optional)"})
                                    }
                                    select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                    name="account" {
                                        option value="" { "Select account..." }
                                        @for (acc_id, acc_state) in journal_state.accounts.iter() {
                                            option value=(acc_id) { (acc_state.name) }
                                        }
                                    }
                                }
                                div class="grid grid-cols-4 gap-2 sm:gap-3" {
                                    div class="col-span-3" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            "Amount"
                                        }
                                        input class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]"
                                        type="number"
                                        step="0.01" min="0"
                                        placeholder="0.00"
                                        required[i < 2]
                                        name="amount";
                                    }
                                    div {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            "Type"
                                        }
                                        select class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                        name="entry_type" {
                                            option value=(EntryType::Debit) { "Dr" }
                                            option value=(EntryType::Credit) { "Cr" }
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
        }
    };

    Ok(layout::layout(
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
