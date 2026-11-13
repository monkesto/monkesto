use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::journal::error::JournalError;
use crate::journal::transaction::entry::{EntryKind, EntrySide};
use crate::journal::ui::{self, ListQuery};
use crate::journal::{JournalId, layout};
use crate::{BackendType, StateType};
use axum::extract::{Path, Query, State};
use axum_login::AuthSession;
use maud::{Markup, html};
use std::collections::HashMap;
use std::str::FromStr;

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
    let mut account_options: Vec<_> = state
        .journal_service
        .list_journal_accounts(journal_id, authority)
        .await?
        .into_iter()
        .map(|(item, _, _)| (item.id, item.name.to_string()))
        .collect();
    account_options.sort_by_cached_key(|(id, name)| (name.to_lowercase(), id.to_string()));
    let accounts: HashMap<_, _> = account_options.iter().cloned().collect();
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
    let needle = query.needle();
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
            (ui::history_limit(all))
            div id="list-results" data-list-results role="region" aria-label="Search results" tabindex="-1" class="flex flex-col gap-6" {
                (ui::pagination(&path, &query, total, all, false))
                @if transactions.is_empty() {
                    div class=(ui::PANEL) { (ui::empty(!needle.is_empty(), "No transactions yet", "Create a transaction below to record an account movement.", &path)) }
                }
                div class="space-y-3" {
                    @for (transaction, authority, timestamp) in page {
                        @let debits: u128 = transaction.entries.iter().filter(|entry| entry.entry_side == EntrySide::Debit).map(|entry| u128::from(entry.amount.unsigned_abs())).sum();
                        @let credits: u128 = transaction.entries.iter().filter(|entry| entry.entry_side == EntrySide::Credit).map(|entry| u128::from(entry.amount.unsigned_abs())).sum();
                        details class="group overflow-hidden rounded-xl border border-gray-200 bg-white shadow-xs open:border-indigo-300 dark:border-gray-700 dark:bg-gray-800 dark:open:border-indigo-600" {
                            summary class="flex cursor-pointer list-none flex-wrap items-center gap-4 p-4 select-none hover:bg-gray-50 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-indigo-500 sm:p-5 dark:hover:bg-gray-700/40 [&::-webkit-details-marker]:hidden" {
                                span aria-hidden="true" class="text-gray-400 transition-transform group-open:rotate-90" { "▸" }
                                div class="min-w-0 grow basis-48" {
                                    h2 class="break-words text-base font-semibold text-gray-900 dark:text-white" {
                                        (transaction.memo.as_ref().map(|memo| memo.as_ref()).filter(|memo| !memo.trim().is_empty()).unwrap_or("Account transaction"))
                                    }
                                    p class="mt-1 text-sm text-gray-500 dark:text-gray-400" { (ui::date(*timestamp)) " · " (transaction.entries.len()) " entries" }
                                }
                                div class="ml-auto text-right" {
                                    p class="text-lg font-semibold tabular-nums text-gray-900 dark:text-white" { (ui::money(debits)) }
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
                                                td class="whitespace-nowrap px-4 py-3 text-right tabular-nums" { (ui::money(debits)) }
                                                td class="whitespace-nowrap px-4 py-3 text-right tabular-nums" { (ui::money(credits)) }
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
                p class="mt-1 text-sm leading-6 text-gray-500 dark:text-gray-400" { "Choose the accounts and amounts for each entry. Total debits must equal total credits." }
                @if account_options.is_empty() {
                    p class="mt-5 text-sm text-gray-600 dark:text-gray-300" {
                        "Add accounts before recording a transaction. "
                        a href=(format!("/journal/{journal_id}/account#create")) class="font-semibold text-indigo-600 hover:underline dark:text-indigo-400" { "Add an account" }
                    }
                } @else {
                    form method="post" action=(&path) class="mt-6 space-y-5" {
                        div {
                            label for="transaction-memo" class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Memo (optional)" }
                            input id="transaction-memo" name="memo" type="text" maxlength="100" placeholder="What is this transaction for?" class=(ui::INPUT);
                        }
                        @for i in 0..4 {
                            fieldset class="rounded-lg border border-gray-200 p-4 dark:border-gray-700" {
                                legend class="px-1 text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400" { "Entry " (i + 1) @if i >= 2 { " · optional" } }
                                div class="grid gap-4 sm:grid-cols-12" {
                                    div class="sm:col-span-6" {
                                        label for=(format!("entry-{i}-account")) class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Account" }
                                        select id=(format!("entry-{i}-account")) name="account" required[i < 2] class=(ui::INPUT) {
                                            option value="" { "Select account…" }
                                            @for (account_id, name) in &account_options { option value=(account_id) { (name) } }
                                        }
                                    }
                                    div class="sm:col-span-3" {
                                        label for=(format!("entry-{i}-amount")) class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Amount ($)" }
                                        input id=(format!("entry-{i}-amount")) name="amount" type="number" inputmode="decimal" min="0.01" step="0.01" required[i < 2] placeholder="0.00" class=(ui::INPUT);
                                    }
                                    div class="sm:col-span-3" {
                                        label for=(format!("entry-{i}-side")) class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Side" }
                                        select id=(format!("entry-{i}-side")) name="entry_type" class=(ui::INPUT) {
                                            option value=(EntrySide::Debit as i8) selected[i != 1] { "Debit (Dr)" }
                                            option value=(EntrySide::Credit as i8) selected[i == 1] { "Credit (Cr)" }
                                        }
                                    }
                                }
                            }
                        }
                        div class="flex flex-wrap items-center justify-between gap-3 border-t border-gray-200 pt-5 dark:border-gray-700" {
                            p class="text-sm text-gray-500 dark:text-gray-400" { "Use positive amounts and choose debit or credit." }
                            button type="submit" class=(ui::BUTTON) { "Create transaction" }
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
