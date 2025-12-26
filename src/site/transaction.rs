use super::layout::Layout;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

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

#[derive(Debug, Clone)]
struct AccountItem {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone)]
struct Person {
    pub id: Uuid,
    pub username: String,
}

#[derive(Debug, Clone)]
struct Transaction {
    pub id: Uuid,
    pub author: Person,
    pub entries: Vec<Entry>,
}

fn transactions() -> Vec<Transaction> {
    use std::str::FromStr;
    vec![
        Transaction {
            id: Uuid::from_str("350e8400-e29b-41d4-a716-446655440000").expect("Invalid UUID"),
            author: Person {
                id: Uuid::from_str("250e8400-e29b-41d4-a716-446655440000").expect("Invalid UUID"),
                username: "johndoe".to_string(),
            },
            entries: vec![
                Entry {
                    account: AccountItem {
                        id: Uuid::from_str("450e8400-e29b-41d4-a716-446655440000")
                            .expect("Invalid UUID"),
                        name: "Cash".to_string(),
                    },
                    amount: 4567, // $45.67 in cents
                    entry_type: EntryType::Credit,
                },
                Entry {
                    account: AccountItem {
                        id: Uuid::from_str("450e8400-e29b-41d4-a716-446655440002")
                            .expect("Invalid UUID"),
                        name: "Groceries Expense".to_string(),
                    },
                    amount: 4567, // $45.67 in cents
                    entry_type: EntryType::Debit,
                },
            ],
        },
        Transaction {
            id: Uuid::from_str("350e8400-e29b-41d4-a716-446655440001").expect("Invalid UUID"),
            author: Person {
                id: Uuid::from_str("250e8400-e29b-41d4-a716-446655440001").expect("Invalid UUID"),
                username: "janesmith".to_string(),
            },
            entries: vec![
                Entry {
                    account: AccountItem {
                        id: Uuid::from_str("450e8400-e29b-41d4-a716-446655440001")
                            .expect("Invalid UUID"),
                        name: "Checking Account".to_string(),
                    },
                    amount: 3214, // $32.14 in cents
                    entry_type: EntryType::Credit,
                },
                Entry {
                    account: AccountItem {
                        id: Uuid::from_str("450e8400-e29b-41d4-a716-446655440003")
                            .expect("Invalid UUID"),
                        name: "Fuel Expense".to_string(),
                    },
                    amount: 3214, // $32.14 in cents
                    entry_type: EntryType::Debit,
                },
            ],
        },
        Transaction {
            id: Uuid::from_str("350e8400-e29b-41d4-a716-446655440002").expect("Invalid UUID"),
            author: Person {
                id: Uuid::from_str("250e8400-e29b-41d4-a716-446655440002").expect("Invalid UUID"),
                username: "bobjohnson".to_string(),
            },
            entries: vec![
                Entry {
                    account: AccountItem {
                        id: Uuid::from_str("450e8400-e29b-41d4-a716-446655440000")
                            .expect("Invalid UUID"),
                        name: "Cash".to_string(),
                    },
                    amount: 425, // $4.25 in cents
                    entry_type: EntryType::Credit,
                },
                Entry {
                    account: AccountItem {
                        id: Uuid::from_str("450e8400-e29b-41d4-a716-446655440004")
                            .expect("Invalid UUID"),
                        name: "Coffee Expense".to_string(),
                    },
                    amount: 425, // $4.25 in cents
                    entry_type: EntryType::Debit,
                },
            ],
        },
    ]
}

fn journals() -> Vec<super::journal::Journal> {
    use std::str::FromStr;
    vec![
        super::journal::Journal {
            id: Uuid::from_str("550e8400-e29b-41d4-a716-446655440000").expect("Invalid UUID"),
            name: "Personal".to_string(),
            creator_username: "johndoe".to_string(),
            created_at: "2024-01-15 09:30:45".to_string(),
        },
        super::journal::Journal {
            id: Uuid::from_str("550e8400-e29b-41d4-a716-446655440001").expect("Invalid UUID"),
            name: "Business".to_string(),
            creator_username: "janesmith".to_string(),
            created_at: "2024-01-20 14:22:18".to_string(),
        },
    ]
}

#[component]
pub fn TransactionListPage() -> impl IntoView {
    let params = use_params_map();
    let journal_id = move || params.get().get("id").unwrap_or_default().to_string();

    let journal_name = move || {
        journals()
            .into_iter()
            .find(|j| j.id.to_string() == journal_id())
            .map(|j| j.name)
            .unwrap_or_else(|| "Unknown Journal".to_string())
    };

    view! {
        <Layout page_title=journal_name() show_switch_link=true journal_id=journal_id()>
            {transactions()
                .into_iter()
                .map(|transaction| {
                    view! {
                        <a
                            href=format!("/{}/transaction/{}", journal_id(), transaction.id)
                            class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                        >
                            <div class="space-y-3">
                                <div class="space-y-2">
                                    {transaction
                                        .entries
                                        .iter()
                                        .map(|entry| {
                                            let entry_amount = format!(
                                                "${}.{:02}",
                                                entry.amount / 100,
                                                entry.amount % 100,
                                            );
                                            let entry_type_str = match entry.entry_type {
                                                EntryType::Debit => "Dr",
                                                EntryType::Credit => "Cr",
                                            };
                                            view! {
                                                <div class="flex justify-between items-center">
                                                    <span class="text-base font-medium text-gray-900 dark:text-white">
                                                        {entry.account.name.clone()}
                                                    </span>
                                                    <span class="text-base text-gray-700 dark:text-gray-300">
                                                        {entry_amount} " " {entry_type_str}
                                                    </span>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                                <div class="text-xs text-gray-400 dark:text-gray-500">
                                    {transaction.author.username}
                                </div>
                            </div>
                        </a>
                    }
                })
                .collect_view()}
            <hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600" />
            <div class="mt-10">
                <div class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl p-6">
                    <h3 class="text-lg font-semibold text-gray-900 dark:text-white mb-6">
                        "Create New Transaction"
                    </h3>
                    <form class="space-y-6">
                        // Entry 1
                        <div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3">
                            <div>
                                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                    "Account"
                                </label>
                                <select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400">
                                    <option value="">"Select account..."</option>
                                    <option value="cash">"Cash"</option>
                                    <option value="checking">"Checking Account"</option>
                                    <option value="savings">"Savings Account"</option>
                                    <option value="groceries">"Groceries Expense"</option>
                                    <option value="fuel">"Fuel Expense"</option>
                                    <option value="coffee">"Coffee Expense"</option>
                                </select>
                            </div>
                            <div class="grid grid-cols-4 gap-2 sm:gap-3">
                                <div class="col-span-3">
                                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                        "Amount"
                                    </label>
                                    <input
                                        type="number"
                                        step="0.01"
                                        min="0"
                                        placeholder="0.00"
                                        class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]"
                                    />
                                </div>
                                <div>
                                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                        "Type"
                                    </label>
                                    <select class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400">
                                        <option value="debit">"Dr"</option>
                                        <option value="credit">"Cr"</option>
                                    </select>
                                </div>
                            </div>
                        </div>

                        // Entry 2
                        <div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3">
                            <div>
                                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                    "Account"
                                </label>
                                <select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400">
                                    <option value="">"Select account..."</option>
                                    <option value="cash">"Cash"</option>
                                    <option value="checking">"Checking Account"</option>
                                    <option value="savings">"Savings Account"</option>
                                    <option value="groceries">"Groceries Expense"</option>
                                    <option value="fuel">"Fuel Expense"</option>
                                    <option value="coffee">"Coffee Expense"</option>
                                </select>
                            </div>
                            <div class="grid grid-cols-4 gap-2 sm:gap-3">
                                <div class="col-span-3">
                                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                        "Amount"
                                    </label>
                                    <input
                                        type="number"
                                        step="0.01"
                                        min="0"
                                        placeholder="0.00"
                                        class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]"
                                    />
                                </div>
                                <div>
                                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                        "Type"
                                    </label>
                                    <select class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400">
                                        <option value="debit">"Dr"</option>
                                        <option value="credit">"Cr"</option>
                                    </select>
                                </div>
                            </div>
                        </div>

                        // Entry 3
                        <div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3">
                            <div>
                                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                    "Account (Optional)"
                                </label>
                                <select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400">
                                    <option value="">"Select account..."</option>
                                    <option value="cash">"Cash"</option>
                                    <option value="checking">"Checking Account"</option>
                                    <option value="savings">"Savings Account"</option>
                                    <option value="groceries">"Groceries Expense"</option>
                                    <option value="fuel">"Fuel Expense"</option>
                                    <option value="coffee">"Coffee Expense"</option>
                                </select>
                            </div>
                            <div class="grid grid-cols-4 gap-2 sm:gap-3">
                                <div class="col-span-3">
                                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                        "Amount"
                                    </label>
                                    <input
                                        type="number"
                                        step="0.01"
                                        min="0"
                                        placeholder="0.00"
                                        class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]"
                                    />
                                </div>
                                <div>
                                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                        "Type"
                                    </label>
                                    <select class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400">
                                        <option value="debit">"Dr"</option>
                                        <option value="credit">"Cr"</option>
                                    </select>
                                </div>
                            </div>
                        </div>

                        // Entry 4
                        <div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg space-y-3">
                            <div>
                                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                    "Account (Optional)"
                                </label>
                                <select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400">
                                    <option value="">"Select account..."</option>
                                    <option value="cash">"Cash"</option>
                                    <option value="checking">"Checking Account"</option>
                                    <option value="savings">"Savings Account"</option>
                                    <option value="groceries">"Groceries Expense"</option>
                                    <option value="fuel">"Fuel Expense"</option>
                                    <option value="coffee">"Coffee Expense"</option>
                                </select>
                            </div>
                            <div class="grid grid-cols-4 gap-2 sm:gap-3">
                                <div class="col-span-3">
                                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                        "Amount"
                                    </label>
                                    <input
                                        type="number"
                                        step="0.01"
                                        min="0"
                                        placeholder="0.00"
                                        class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]"
                                    />
                                </div>
                                <div>
                                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                                        "Type"
                                    </label>
                                    <select class="w-full h-10 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400">
                                        <option value="debit">"Dr"</option>
                                        <option value="credit">"Cr"</option>
                                    </select>
                                </div>
                            </div>
                        </div>

                        <div class="flex justify-between items-center pt-4 border-t border-gray-200 dark:border-gray-600">
                            <div class="text-sm text-gray-500 dark:text-gray-400">
                                "Debits must equal credits"
                            </div>
                            <button
                                type="submit"
                                class="px-6 py-2 bg-indigo-600 text-white font-medium rounded-md hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-400 dark:ring-offset-gray-800"
                            >
                                "Create Transaction"
                            </button>
                        </div>
                    </form>
                </div>
            </div>
        </Layout>
    }
}
