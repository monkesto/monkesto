use crate::auth::axum_login::AuthSession;
use crate::auth::user;
use crate::cuid::Cuid;
use crate::journal::layout;
use crate::journal::queries::get_associated_journals;
use crate::known_errors::{KnownErrors, UrlError};
use axum::Extension;
use axum::extract::{Path, Query};
use axum::response::Redirect;
use maud::{Markup, html};
use sqlx::PgPool;
use std::fmt::{self, Display};
use std::str::FromStr;

#[derive(Debug, Clone)]
enum EntryType {
    Debit,
    Credit,
}

#[derive(Debug, Clone)]
struct Entry {
    pub account: AccountItem,
    pub amount: i64, // in cents
    pub entry_type: EntryType,
}

impl Display for EntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Debit => write!(f, "Dr"),
            Self::Credit => write!(f, "Cr"),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct AccountItem {
    pub id: Cuid,
    pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct Person {
    pub id: Cuid,
    pub username: String,
}

#[derive(Debug, Clone)]
struct Transaction {
    pub id: Cuid,
    pub author: Person,
    pub entries: Vec<Entry>,
}

fn transactions() -> Vec<Transaction> {
    vec![
        Transaction {
            id: Cuid::from_str("aaaaaaaaab").expect("Invalid Cuid"),
            author: Person {
                id: Cuid::from_str("aaaaaaaaac").expect("Invalid Cuid"),
                username: "johndoe".to_string(),
            },
            entries: vec![
                Entry {
                    account: AccountItem {
                        id: Cuid::from_str("aaaaaaaaab").expect("Invalid Cuid"),
                        name: "Cash".to_string(),
                    },
                    amount: 4567, // $45.67 in cents
                    entry_type: EntryType::Credit,
                },
                Entry {
                    account: AccountItem {
                        id: Cuid::from_str("aaaaaaaaac").expect("Invalid Cuid"),
                        name: "Groceries Expense".to_string(),
                    },
                    amount: 4567, // $45.67 in cents
                    entry_type: EntryType::Debit,
                },
            ],
        },
        Transaction {
            id: Cuid::from_str("aaaaaaaaab").expect("Invalid Cuid"),
            author: Person {
                id: Cuid::from_str("aaaaaaaaac").expect("Invalid Cuid"),
                username: "janesmith".to_string(),
            },
            entries: vec![
                Entry {
                    account: AccountItem {
                        id: Cuid::from_str("aaaaaaaaac").expect("Invalid Cuid"),
                        name: "Checking Account".to_string(),
                    },
                    amount: 3214, // $32.14 in cents
                    entry_type: EntryType::Credit,
                },
                Entry {
                    account: AccountItem {
                        id: Cuid::from_str("aaaaaaaaad").expect("Invalid Cuid"),
                        name: "Fuel Expense".to_string(),
                    },
                    amount: 3214, // $32.14 in cents
                    entry_type: EntryType::Debit,
                },
            ],
        },
        Transaction {
            id: Cuid::from_str("aaaaaaaaac").expect("Invalid Cuid"),
            author: Person {
                id: Cuid::from_str("aaaaaaaaac").expect("Invalid Cuid"),
                username: "bobjohnson".to_string(),
            },
            entries: vec![
                Entry {
                    account: AccountItem {
                        id: Cuid::from_str("aaaaaaaaad").expect("Invalid Cuid"),
                        name: "Cash".to_string(),
                    },
                    amount: 425, // $4.25 in cents
                    entry_type: EntryType::Credit,
                },
                Entry {
                    account: AccountItem {
                        id: Cuid::from_str("aaaaaaaaae").expect("Invalid Cuid"),
                        name: "Coffee Expense".to_string(),
                    },
                    amount: 425, // $4.25 in cents
                    entry_type: EntryType::Debit,
                },
            ],
        },
    ]
}

pub async fn transaction_list_page(
    Extension(pool): Extension<PgPool>,
    session: AuthSession,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user_id = user::get_id(session)?;

    let journal_id = Cuid::from_str(&id);

    let journals = get_associated_journals(&user_id, &pool).await;

    let journal_res = journals
        .ok()
        .and_then(|journals| journals.get(&journal_id.unwrap_or_default()).cloned());

    let content = html! {
        @for transaction in transactions() {
            a
            href=(format!("/journal/{}/transaction/{}", id, transaction.id.to_string()))
            class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                div class="space-y-3" {
                    div class="space-y-2" {
                        @for entry in transaction.entries {
                            @let entry_amount = format!("${}.{:02}", entry.amount / 100, entry.amount % 100);

                            div class="flex justify-between items-center" {
                                span class="text-base font-medium text-gray-900 dark:text-white" {
                                    (entry.account.name)
                                }

                                span class="text-base text-gray-700 dark:text-gray-300" {
                                    (entry_amount) " " (entry.entry_type)
                                }
                            }
                        }
                    }
                    div class="text-xs text-gray-400 dark:text-gray-500" {
                        (transaction.author.username)
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

                form class="space-y-6" {
                    // Entry 1
                    div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3" {
                        div {
                            label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                "Account"
                            }
                            select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400" {
                                option value="" { "Select account..." }
                                option value="cash" { "Cash" }
                                option value="checking" { "Checking Account" }
                                option value="savings" { "Savings Account" }
                                option value="groceries" { "Groceries Expense" }
                                option value="fuel" { "Fuel Expense" }
                                option value="coffee" { "Coffee Expense" }
                            }
                        }
                        div class="grid grid-cols-4 gap-2 sm:gap-3" {
                            div class="col-span-3" {
                                label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                    "Amount"
                                }
                                input class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]" type="number" step="0.01" min="0" placeholder="0.00";
                            }
                            div {
                                label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                    "Type"
                                }
                                select class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400" {
                                    option value="debit" { "Dr" }
                                    option value="credit" { "Cr" }
                                }
                            }
                        }
                    }

                    // Entry 2
                    div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3" {
                        div {
                            label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                "Account"
                            }
                            select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400" {
                                option value="" { "Select account..." }
                                option value="cash" { "Cash" }
                                option value="checking" { "Checking Account" }
                                option value="savings" { "Savings Account" }
                                option value="groceries" { "Groceries Expense" }
                                option value="fuel" { "Fuel Expense" }
                                option value="coffee" { "Coffee Expense" }
                            }
                        }
                        div class="grid grid-cols-4 gap-2 sm:gap-3" {
                            div class="col-span-3" {
                                label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                    "Amount"
                                }
                                input class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]" type="number" step="0.01" min="0" placeholder="0.00";
                            }
                            div {
                                label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                    "Type"
                                }
                                select class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400" {
                                    option value="debit" { "Dr" }
                                    option value="credit" { "Cr" }
                                }
                            }
                        }
                    }

                    // Entry 3
                    div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3" {
                        div {
                            label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                "Account (Optional)"
                            }
                            select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400" {
                                option value="" { "Select account..." }
                                option value="cash" { "Cash" }
                                option value="checking" { "Checking Account" }
                                option value="savings" { "Savings Account" }
                                option value="groceries" { "Groceries Expense" }
                                option value="fuel" { "Fuel Expense" }
                                option value="coffee" { "Coffee Expense" }
                            }
                        }
                        div class="grid grid-cols-4 gap-2 sm:gap-3" {
                            div class="col-span-3" {
                                label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                    "Amount"
                                }
                                input class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]" type="number" step="0.01" min="0" placeholder="0.00";
                            }
                            div {
                                label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                    "Type"
                                }
                                select class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400" {
                                    option value="debit" { "Dr" }
                                    option value="credit" { "Cr" }
                                }
                            }
                        }
                    }

                    // Entry 4
                    div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3" {
                        div {
                            label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                "Account (Optional)"
                            }
                            select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400" {
                                option value="" { "Select account..." }
                                option value="cash" { "Cash" }
                                option value="checking" { "Checking Account" }
                                option value="savings" { "Savings Account" }
                                option value="groceries" { "Groceries Expense" }
                                option value="fuel" { "Fuel Expense" }
                                option value="coffee" { "Coffee Expense" }
                            }
                        }
                        div class="grid grid-cols-4 gap-2 sm:gap-3" {
                            div class="col-span-3" {
                                label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                    "Amount"
                                }
                                input class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]" type="number" step="0.01" min="0" placeholder="0.00";
                            }
                            div {
                                label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                    "Type"
                                }
                                select class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400" {
                                    option value="debit" { "Dr" }
                                    option value="credit" { "Cr" }
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

    Ok(layout::maud_layout(
        Some(&journal_res.map_or_else(|| "unknown journal".to_string(), |j| j.get_name())),
        true,
        Some(&id),
        content,
    ))
}
