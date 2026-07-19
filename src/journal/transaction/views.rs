use crate::BackendType;
use crate::StateType;
use crate::authn::user::UserState;
use crate::authn::{UserId, get_user};
use crate::authority::Actor;
use crate::authority::Authority;
use crate::email::Email;
use crate::journal::JournalId;
use crate::journal::account::AccountId;
use crate::journal::layout;
use crate::journal::service::{AccountState, TransactionState};
use crate::journal::transaction::EntryType;
use crate::monkesto_error::UrlError;
use crate::monkesto_error::{MonkestoError, MonkestoResult};
use crate::time_provider::Timestamp;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::Redirect;
use axum_login::AuthSession;
use maud::Markup;
use maud::html;
use std::collections::HashMap;
use std::str::FromStr;

pub async fn transaction_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));

    let journal_id_res = JournalId::from_str(&id);

    let transactions_res: MonkestoResult<Vec<(TransactionState, Authority, Timestamp)>> =
        match &journal_id_res {
            Ok(id) => state
                .journal_service
                .list_journal_transactions(*id, &user_authority)
                .await
                .map_err(|e| e.into()),
            Err(e) => Err(e.clone().into()),
        };

    let accounts_res: MonkestoResult<HashMap<AccountId, AccountState>> = match &journal_id_res {
        Ok(id) => match state
            .journal_service
            .list_journal_accounts(*id, &user_authority)
            .await
        {
            Ok(accounts) => Ok(accounts
                .into_iter()
                .map(|(state, _, _)| (state.id, state))
                .collect::<HashMap<AccountId, AccountState>>()),
            Err(e) => Err(e.into()),
        },
        Err(e) => Err(e.clone().into()),
    };

    let members_res: MonkestoResult<HashMap<UserId, UserState>> = match &journal_id_res {
        Ok(id) => match state
            .journal_service
            .list_journal_members(*id, &user_authority)
            .await
        {
            Ok(ids) => match state.authn_service.fetch_users(ids.as_slice()).await {
                Ok(members) => Ok(members
                    .into_iter()
                    .map(|m| (m.id, m))
                    .collect::<HashMap<UserId, UserState>>()),
                Err(e) => Err(e.into()),
            },
            Err(e) => Err(e.into()),
        },
        Err(e) => Err(e.clone().into()),
    };

    let mut nonmember_cache: HashMap<UserId, Email> = HashMap::new();

    let content = html! {
        @if let Ok(ref transactions) = transactions_res {
            @for (tx, tx_authority, _) in transactions {
                a
                href=(format!("/journal/{}/transaction/{}", id, tx.id))
                class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                    div class="space-y-3" {
                        div class="space-y-2" {
                            @for entry in tx.entries.iter() {
                                @let entry_amount = format!("${}.{:02}", entry.amount / 100, entry.amount % 100);

                                div class="flex justify-between items-center" {
                                    span class="text-base font-medium text-gray-900 dark:text-white" {
                                        @match &accounts_res {
                                            Ok(accounts) => (accounts.get(&entry.account_id).map(|acct| acct.name.as_ref()).unwrap_or("Unknown Account")),
                                            Err(e) => {"encountered an error while fetching accounts: " (e)}
                                        }
                                    }

                                    span class="text-base text-gray-700 dark:text-gray-300" {
                                        (entry_amount) " " (entry.entry_type)
                                    }
                                }
                            }

                            div class="text-xs text-gray-400 dark:text-gray-500" {
                                @match tx_authority.actor() {
                                    Actor::User(id) => {
                                        @match &members_res {
                                            Ok(members) => {
                                                @if let Some(email) = members.get(id).map(|m| m.email.clone()) {
                                                    (email.to_string())
                                                } @else if let Some(email) = nonmember_cache.get(id)  {
                                                    (email.to_string())
                                                } @else {
                                                    // the user may be the owner or somebody who left the journal after creating the transaction
                                                    @match state.authn_service.fetch_user(*id).await {
                                                        Ok(user) => {
                                                            // maud assumes that you never want to call functions for
                                                            // side effects and makes you assign a value to the result
                                                            @let _ = nonmember_cache.insert(user.id, user.email.clone());
                                                            (user.email.to_string())
                                                        },
                                                        Err(e) => {"failed to fetch user: " (e)}
                                                    }
                                                }
                                            },
                                            Err(e) => {"failed to fetch users: " (e)}
                                        }
                                    },
                                    Actor::System => {"system"},
                                    Actor::Anonymous => {"anonymous"}
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
                            div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg" {
                                div class="space-y-3 md:space-y-0 md:grid md:grid-cols-12 md:gap-3" {
                                    div class="md:col-span-6" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            (if i < 2 {"Account"} else {"Account (Optional)"})
                                        }
                                        select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                        name="account" {
                                            option value="" { "Select account..." }
                                            @if let Ok(accounts) = &accounts_res {
                                                @for (acc_id, acc_state) in accounts {
                                                    option value=(acc_id) { (acc_state.name)}
                                                }
                                            } @else {
                                                option value=("invalid account") { "failed to fetch accounts" }
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
                        (format!("An error occurred: {:?}", MonkestoError::decode(&e)))
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

    let journal_name = match &journal_id_res {
        Ok(id) => {
            match state
                .journal_service
                .get_journal(*id, &user_authority)
                .await
            {
                Ok((journal, _, _)) => journal.name.to_string(),
                Err(e) => format!("failed to fetch the journal: {e}"),
            }
        }
        Err(e) => format!("invalid journal id: {e}"),
    };

    Ok(layout::layout(
        Some(&journal_name),
        true,
        Some(&id),
        wrapped_content,
    ))
}
