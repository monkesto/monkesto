use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::journal::error::JournalError;
use crate::journal::ui::{self, ListQuery};
use crate::journal::{JournalId, layout};
use crate::{BackendType, StateType};
use axum::extract::{Path, Query, State};
use axum_login::AuthSession;
use maud::{Markup, html};
use std::collections::HashMap;
use std::str::FromStr;

pub async fn donation_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Markup, JournalError> {
    let user = get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&id)?;
    let mut donations = state
        .journal_service
        .list_journal_donations(journal_id, authority)
        .await?;
    let accounts: HashMap<_, _> = state
        .journal_service
        .list_journal_accounts(journal_id, authority)
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
    let donors: HashMap<_, _> = state
        .journal_service
        .list_journal_donors(journal_id, authority)
        .await?
        .into_iter()
        .map(|(item, _, _)| (item.id, item.name.to_string()))
        .collect();
    let (journal, _, _) = state
        .journal_service
        .get_journal(journal_id, authority)
        .await?;
    let path = format!("/journal/{journal_id}/donation");
    let all = donations.len();
    let needle = query.needle();
    let matches = |value: &str| value.to_lowercase().contains(&needle);
    donations.retain(|(donation, _, date)| {
        needle.is_empty()
            || matches(&donation.id.to_string())
            || matches(&date.format("%Y-%m-%d").to_string())
            || accounts
                .get(&donation.deposit_account)
                .is_some_and(|name| matches(name))
            || donation.entries.iter().any(|entry| {
                funds.get(&entry.fund_id).is_some_and(|name| matches(name))
                    || donors
                        .get(&entry.donor_id)
                        .is_some_and(|name| matches(name))
            })
    });
    ui::sort_history(&mut donations, &query, |donation| donation.id.to_string());
    let total = donations.len();
    let page = &donations[query.range(total)];
    let authors = ui::authors(&state, page.iter().map(|(_, authority, _)| *authority)).await?;
    let content = html! {
        div class="mx-auto flex w-full max-w-6xl flex-col gap-6" {
            (ui::header(journal_id, "donation", "Donations", "Review deposited donations and see how each contribution was allocated to funds. Expand a deposit to see its details.", None))
            (ui::controls(&path, &query, false, "Donor, fund, bank account, or YYYY-MM-DD…"))
            (ui::history_limit(all))
            div id="list-results" data-list-results role="region" aria-label="Search results" tabindex="-1" class="flex flex-col gap-6" {
                (ui::pagination(&path, &query, total, all, false))
                @if donations.is_empty() {
                    div class=(ui::PANEL) { (ui::empty(!needle.is_empty(), "No donations yet", "Imported and recorded donations will appear here.", &path)) }
                }
                div class="space-y-3" {
                    @for (donation, authority, timestamp) in page {
                        details class="group overflow-hidden rounded-xl border border-gray-200 bg-white shadow-xs open:border-indigo-300 dark:border-gray-700 dark:bg-gray-800 dark:open:border-indigo-600" {
                            summary class="flex cursor-pointer list-none flex-wrap items-center gap-4 p-4 select-none hover:bg-gray-50 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-indigo-500 sm:p-5 dark:hover:bg-gray-700/40 [&::-webkit-details-marker]:hidden" {
                                span aria-hidden="true" class="text-gray-400 transition-transform group-open:rotate-90" { "▸" }
                                div class="min-w-0 grow basis-48" {
                                    h2 class="text-base font-semibold text-gray-900 dark:text-white" { "Donation · " (ui::date(*timestamp)) }
                                    p class="mt-1 break-words text-sm text-gray-500 dark:text-gray-400" { (accounts.get(&donation.deposit_account).map(String::as_str).unwrap_or("Unknown account")) }
                                }
                                div class="ml-auto text-right" {
                                    p class="text-lg font-semibold tabular-nums text-gray-900 dark:text-white" { (Dollars(donation.total as i64)) }
                                    p class="mt-1 text-xs text-gray-500 dark:text-gray-400" { (donation.entries.len()) " entries" }
                                }
                                span class="sr-only" { "Expand donation details" }
                            }
                            div class="border-t border-gray-100 dark:border-gray-700" {
                                div class="overflow-x-auto" {
                                    table class="w-full" {
                                        caption class="sr-only" { "Donation allocations" }
                                        thead class="bg-gray-50 dark:bg-gray-900/40" {
                                            tr {
                                                th scope="col" class=(ui::TH) { "Donor" }
                                                th scope="col" class=(ui::TH) { "Fund" }
                                                th scope="col" class="px-4 py-3 text-right text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400" { "Amount" }
                                            }
                                        }
                                        tbody class="divide-y divide-gray-100 dark:divide-gray-700" {
                                            @for entry in &donation.entries {
                                                tr {
                                                    td class=(ui::TD) { (donors.get(&entry.donor_id).map(String::as_str).unwrap_or("Unknown donor")) }
                                                    td class=(ui::TD) { (funds.get(&entry.fund_id).map(String::as_str).unwrap_or("Unknown fund")) }
                                                    td class="whitespace-nowrap px-4 py-3 text-right text-sm tabular-nums text-gray-900 dark:text-gray-100" { (Dollars(entry.amount as i64)) }
                                                }
                                            }
                                        }
                                        tfoot class="border-t border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-900/40" {
                                            tr {
                                                th scope="row" colspan="2" class="px-4 py-3 text-left text-sm font-semibold text-gray-900 dark:text-white" { "Deposited total" }
                                                td class="whitespace-nowrap px-4 py-3 text-right text-sm font-semibold tabular-nums text-gray-900 dark:text-white" { (Dollars(donation.total as i64)) }
                                            }
                                        }
                                    }
                                }
                                footer class="flex flex-wrap justify-between gap-2 border-t border-gray-100 px-4 py-3 text-xs text-gray-500 dark:border-gray-700 dark:text-gray-400" {
                                    p { "Recorded by " (ui::author(*authority, &authors)) }
                                    p class="break-all font-mono" { "ID: " (donation.id) }
                                }
                            }
                        }
                    }
                }
                @if total > 50 { (ui::pagination(&path, &query, total, all, false)) }
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
