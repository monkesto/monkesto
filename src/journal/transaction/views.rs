use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::journal::error::JournalError;
use crate::journal::transaction::FinancialPeriod;
use crate::journal::transaction::entry::{EntryKind, EntrySide};
use crate::journal::ui::{self, ListQuery};
use crate::journal::{JournalId, layout};
use crate::time::{DefaultTimeProvider, TimeProvider};
use crate::{BackendType, StateType};
use axum::extract::{Path, Query, State};
use axum_login::AuthSession;
use maud::{Markup, PreEscaped, html};
use std::collections::HashMap;
use std::str::FromStr;

const LABEL: &str = "mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200";

pub async fn transaction_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Markup, JournalError> {
    let user = get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&id)?;
    let mut transactions = state
        .journal_service
        .list_journal_transactions(journal_id, authority)
        .await?;
    let accounts: HashMap<_, _> = state
        .journal_service
        .list_journal_accounts(journal_id, authority)
        .await?
        .into_iter()
        .map(|(item, _, _)| (item.id, item.name.to_string()))
        .collect();
    let activities: HashMap<_, _> = state
        .journal_service
        .list_journal_activities(journal_id, authority)
        .await?
        .into_iter()
        .map(|(item, _, _)| (item.id, item.name.to_string()))
        .collect();
    let funds: HashMap<_, _> = state
        .journal_service
        .list_journal_funds(journal_id, authority)
        .await?
        .into_iter()
        .map(|(item, _, _)| (item.id, item.name.to_string()))
        .collect();
    let (journal, _, _) = state
        .journal_service
        .get_journal(journal_id, authority)
        .await?;
    let path = format!("/journal/{journal_id}/transaction");
    let description = |kind: EntryKind| -> (String, String) {
        match kind {
            EntryKind::Account { account_id } => (
                accounts
                    .get(&account_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown account".into()),
                "Account".into(),
            ),
            EntryKind::Activity {
                activity_id,
                fund_id,
                ..
            } => (
                activities
                    .get(&activity_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown activity".into()),
                format!(
                    "Fund: {}",
                    funds
                        .get(&fund_id)
                        .map(String::as_str)
                        .unwrap_or("Unknown fund")
                ),
            ),
        }
    };
    let all = transactions.len();
    let needle = query.q.trim().to_lowercase();
    transactions.retain(|(transaction, _, date)| {
        needle.is_empty()
            || transaction.id.to_string().to_lowercase().contains(&needle)
            || date.format("%Y-%m-%d").to_string().contains(&needle)
            || transaction
                .memo
                .as_ref()
                .is_some_and(|memo| memo.as_ref().to_lowercase().contains(&needle))
            || transaction.entries.iter().any(|entry| {
                let (name, detail) = description(entry.entry_kind);
                name.to_lowercase().contains(&needle) || detail.to_lowercase().contains(&needle)
            })
    });
    ui::sort_history(&mut transactions, &query, |transaction| {
        transaction.id.to_string()
    });
    let total = transactions.len();
    let page = &transactions[query.range(total)];
    let authors = ui::authors(&state, page.iter().map(|(_, authority, _)| *authority)).await?;
    let content = html! {
        div class="mx-auto flex w-full max-w-6xl flex-col gap-6" {
            (ui::header(journal_id, "transaction", "Transactions", "Review account movements and fund activity. Expand a transaction to see its debits and credits.", Some("New transaction")))
            (ui::controls(&path, &query, false, "Memo, account, activity, fund, or YYYY-MM-DD…"))
            div id="list-results" data-list-results role="region" aria-label="Search results" tabindex="-1" class="flex flex-col gap-6" {
                (ui::pagination(&path, &query, total, all, false))
                @if transactions.is_empty() {
                    div class=(ui::PANEL) { (ui::empty(!needle.is_empty(), "No transactions yet", "Create a transaction below to get started.", &path)) }
                }
                div class="space-y-3" {
                    @for (transaction, authority, timestamp) in page {
                        @let debits: i128 = transaction.entries.iter().filter(|entry| entry.entry_side == EntrySide::Debit).map(|entry| i128::from(entry.amount.unsigned_abs())).sum();
                        @let credits: i128 = transaction.entries.iter().filter(|entry| entry.entry_side == EntrySide::Credit).map(|entry| i128::from(entry.amount.unsigned_abs())).sum();
                        details class="group overflow-hidden rounded-xl border border-gray-200 bg-white shadow-xs open:border-indigo-300 dark:border-gray-700 dark:bg-gray-800 dark:open:border-indigo-600" {
                            summary class="flex cursor-pointer list-none flex-wrap items-center gap-4 p-4 select-none hover:bg-gray-50 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-indigo-500 sm:p-5 dark:hover:bg-gray-700/40 [&::-webkit-details-marker]:hidden" {
                                span aria-hidden="true" class="text-gray-400 transition-transform group-open:rotate-90" { "▸" }
                                div class="min-w-0 grow basis-48" {
                                    h2 class="break-words text-base font-semibold text-gray-900 dark:text-white" {
                                        (transaction.memo.as_ref().map(|memo| memo.as_ref()).filter(|memo| !memo.trim().is_empty()).unwrap_or("Transaction"))
                                    }
                                    p class="mt-1 text-sm text-gray-500 dark:text-gray-400" { (ui::date(*timestamp)) " · " (transaction.entries.len()) " entries" }
                                }
                                div class="ml-auto text-right" {
                                    p class="text-lg font-semibold tabular-nums text-gray-900 dark:text-white" { (Dollars(debits)) }
                                    p class="mt-1 text-xs text-gray-500 dark:text-gray-400" { "Total debits" }
                                }
                                span class="sr-only" { "Expand transaction details" }
                            }
                            div class="border-t border-gray-100 dark:border-gray-700" {
                                div class="overflow-x-auto" {
                                    table class="w-full" {
                                        caption class="sr-only" { "Transaction entries" }
                                        thead class="bg-gray-50 dark:bg-gray-900/40" {
                                            tr {
                                                th scope="col" class=(ui::TH) { "Account / activity" }
                                                th scope="col" class="px-4 py-3 text-right text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400" { "Debit" }
                                                th scope="col" class="px-4 py-3 text-right text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400" { "Credit" }
                                            }
                                        }
                                        tbody class="divide-y divide-gray-100 dark:divide-gray-700" {
                                            @for entry in &transaction.entries {
                                                @let (name, detail) = description(entry.entry_kind);
                                                tr {
                                                    th scope="row" class="min-w-48 px-4 py-3 text-left text-sm font-medium text-gray-900 dark:text-white" {
                                                        (name)
                                                        p class="mt-1 text-xs font-normal text-gray-500 dark:text-gray-400" { (detail) }
                                                    }
                                                    td class="whitespace-nowrap px-4 py-3 text-right text-sm tabular-nums text-gray-900 dark:text-gray-100" {
                                                        @if entry.entry_side == EntrySide::Debit { (Dollars(entry.amount)) } @else { span class="text-gray-400" { "—" } }
                                                    }
                                                    td class="whitespace-nowrap px-4 py-3 text-right text-sm tabular-nums text-gray-900 dark:text-gray-100" {
                                                        @if entry.entry_side == EntrySide::Credit { (Dollars(entry.amount)) } @else { span class="text-gray-400" { "—" } }
                                                    }
                                                }
                                            }
                                        }
                                        tfoot class="border-t border-gray-200 bg-gray-50 text-sm font-semibold text-gray-900 dark:border-gray-700 dark:bg-gray-900/40 dark:text-white" {
                                            tr {
                                                th scope="row" class="px-4 py-3 text-left" { "Totals" }
                                                td class="whitespace-nowrap px-4 py-3 text-right tabular-nums" { (Dollars(debits)) }
                                                td class="whitespace-nowrap px-4 py-3 text-right tabular-nums" { (Dollars(credits)) }
                                            }
                                        }
                                    }
                                }
                                footer class="flex flex-wrap justify-between gap-2 border-t border-gray-100 px-4 py-3 text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400" {
                                    p { "Recorded by " (ui::author(*authority, &authors)) }
                                    p class="break-all font-mono" { "ID: " (transaction.id) }
                                }
                            }
                        }
                    }
                }
                @if total > 50 { (ui::pagination(&path, &query, total, all, false)) }
            }
            section id="create" aria-labelledby="create-heading" class="scroll-mt-6 rounded-xl border border-gray-200 bg-white p-5 sm:p-6 dark:border-gray-700 dark:bg-gray-800" {
                h2 id="create-heading" class="text-lg font-semibold text-gray-900 dark:text-white" { "New transaction" }
                @let has_accounts = !accounts.is_empty();
                @let has_activities = !activities.is_empty() && !funds.is_empty();
                @if has_accounts || has_activities {
                    @let period = FinancialPeriod::from(DefaultTimeProvider.get_time().0);
                    noscript {
                        p class="mt-4 text-sm text-gray-600 dark:text-gray-300" { "Enable JavaScript to create a transaction." }
                    }
                    form id="transaction-form" method="post" action=(&path) hidden class="mt-4" {
                        fieldset class="space-y-5" data-transaction-controls {
                            div class="grid gap-4 sm:grid-cols-2" {
                                label {
                                    span class=(LABEL) { "Financial period" }
                                    select name="period" required class=(ui::INPUT) {
                                        @for month in 1..=12 {
                                            @let month = FinancialPeriod::try_from(month).expect("valid month");
                                            @let name = format!("{month:?}");
                                            option value=(&name) selected[month == period] { (name) }
                                        }
                                    }
                                }
                                label {
                                    span class=(LABEL) { "Memo (optional)" }
                                    input name="memo" type="text" maxlength="100" class=(ui::INPUT);
                                }
                            }
                            ol data-transaction-entries aria-label="Transaction entries" class="space-y-2" {}
                            div class="flex flex-wrap items-center justify-between gap-3 border-t border-gray-200 pt-5 dark:border-gray-700" {
                                button type="button" data-add-entry aria-haspopup="dialog" class=(ui::SECONDARY) { "Add entry" }
                                button type="submit" class=(ui::BUTTON) { "Create transaction" }
                            }
                        }
                        p data-transaction-error hidden role="alert" tabindex="-1"
                            class="mt-4 rounded-lg bg-red-50 p-3 text-sm text-red-800 dark:bg-red-950 dark:text-red-200" {}
                    }
                    template id="transaction-entry" {
                        li class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-gray-200 p-3 dark:border-gray-700" {
                            div class="min-w-0 grow basis-40" {
                                p data-entry-name class="break-words text-sm font-semibold text-gray-900 dark:text-white" {}
                                p data-entry-detail class="mt-1 break-words text-xs text-gray-500 dark:text-gray-400" {}
                            }
                            p data-entry-amount class="text-sm font-semibold tabular-nums text-gray-900 dark:text-white" {}
                            div class="flex flex-wrap gap-2" {
                                button type="button" data-edit-entry aria-haspopup="dialog" class=(ui::SECONDARY) { "Edit" }
                                button type="button" data-remove-entry class=(ui::SECONDARY) { "Remove" }
                            }
                        }
                    }
                    dialog id="transaction-entry-dialog" aria-labelledby="transaction-entry-heading"
                        class="fixed inset-0 m-auto h-fit max-h-[calc(100dvh_-_2rem)] w-[calc(100%_-_2rem)] max-w-2xl overflow-y-auto rounded-xl border border-gray-200 bg-white p-5 text-gray-900 shadow-xl backdrop:bg-gray-950/60 sm:p-6 dark:border-gray-700 dark:bg-gray-800 dark:text-white" {
                        form id="transaction-entry-editor" method="dialog" class="space-y-5" {
                            h3 id="transaction-entry-heading" class="text-lg font-semibold" { "Add entry" }
                            fieldset class="grid grid-cols-2 gap-2" {
                                legend class=(LABEL) { "Entry kind" }
                                @for (kind, label, available, only_kind) in [
                                    ("account", "Account", has_accounts, !has_activities),
                                    ("activity", "Activity", has_activities, !has_accounts),
                                ] {
                                    @if available {
                                        label {
                                            input type="radio" name="kind" value=(kind) required checked[only_kind] class="peer sr-only";
                                            span class="block cursor-pointer rounded-lg border border-gray-300 px-3 py-2 text-center text-sm font-semibold peer-checked:border-indigo-500 peer-checked:bg-indigo-50 peer-checked:text-indigo-700 peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-indigo-500 dark:border-gray-600 dark:peer-checked:bg-indigo-950 dark:peer-checked:text-indigo-200" { (label) }
                                        }
                                    }
                                }
                            }
                            @for (kind, pickers) in [
                                ("account", &[("account", "Account")][..]),
                                ("activity", &[("activity", "Activity"), ("fund", "Fund")][..]),
                            ] {
                                fieldset data-entry-group=(kind) hidden disabled {
                                    div class=(if kind == "activity" { "grid gap-4 sm:grid-cols-2" } else { "" }) {
                                        @for (key, label) in pickers {
                                            div class="space-y-2" {
                                                label class="block" {
                                                    span class=(LABEL) { "Search " (label.to_lowercase()) " names" }
                                                    input type="search" data-option-search=(key) class=(ui::INPUT);
                                                }
                                                select name=(format!("{key}_id")) aria-label=(label) data-entry-picker=(key) size="4" required class=(ui::INPUT) {}
                                            }
                                        }
                                    }
                                    @if kind == "activity" {
                                        label class="mt-4 flex items-center gap-2 text-sm text-gray-700 dark:text-gray-200" {
                                            input name="transfer" type="checkbox";
                                            "Internal transfer"
                                        }
                                    }
                                }
                            }
                            fieldset data-entry-group="details" hidden disabled {
                                div class="grid gap-4 sm:grid-cols-2" {
                                    label {
                                        span class=(LABEL) { "Amount ($)" }
                                        input name="amount" type="text" inputmode="decimal" required placeholder="0.00" class=(ui::INPUT);
                                    }
                                    label {
                                        span class=(LABEL) { "Side" }
                                        select name="entry_side" required class=(ui::INPUT) {
                                            option value="" disabled selected { "Choose a side…" }
                                            option value="Debit" { "Debit (Dr)" }
                                            option value="Credit" { "Credit (Cr)" }
                                        }
                                    }
                                }
                            }
                            div class="flex justify-end gap-3 border-t border-gray-200 pt-4 dark:border-gray-700" {
                                button type="button" data-cancel-entry class=(ui::SECONDARY) { "Cancel" }
                                button type="submit" data-save-entry class=(ui::BUTTON) { "Add entry" }
                            }
                        }
                    }
                    template id="transaction-options" {
                        select data-options="account" {
                            @for (id, name) in &accounts { option value=(id) { (name) } }
                        }
                        select data-options="activity" {
                            @for (id, name) in &activities { option value=(id) { (name) } }
                        }
                        select data-options="fund" {
                            @for (id, name) in &funds { option value=(id) { (name) } }
                        }
                    }
                    script type="module" { (PreEscaped(include_str!("form.js"))) }
                } @else {
                    p class="mt-4 text-sm text-gray-600 dark:text-gray-300" {
                        "To record a transaction, add an account or set up an activity and fund."
                    }
                    div class="mt-3 flex flex-wrap gap-2" {
                        a href=(format!("/journal/{journal_id}/account#create")) class=(ui::SECONDARY) { "Add an account" }
                        @if activities.is_empty() {
                            a href=(format!("/journal/{journal_id}/activity#create")) class=(ui::SECONDARY) { "Add an activity" }
                        }
                        @if funds.is_empty() {
                            a href=(format!("/journal/{journal_id}/fund#create")) class=(ui::SECONDARY) { "Add a fund" }
                        }
                    }
                }
            }
        }
    };
    Ok(layout::layout(
        Some(journal.name.as_ref()),
        true,
        Some(&id),
        content,
    ))
}
