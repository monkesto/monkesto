//! Shared controls for the journal's collection pages. Filtering happens before pagination.
use crate::authority::Authority;
use crate::dollars::Dollars;
use crate::journal::JournalId;
use crate::time::Timestamp;
use maud::{Markup, PreEscaped, html};
use serde::Deserialize;
use std::ops::Range;
use url::form_urlencoded;

pub const INPUT: &str = "block w-full rounded-lg border border-gray-300 bg-white px-3 py-2.5 text-sm text-gray-900 shadow-xs focus:border-indigo-500 focus:outline-2 focus:outline-indigo-500 dark:border-gray-600 dark:bg-gray-900 dark:text-white";
pub const BUTTON: &str = "inline-flex items-center justify-center rounded-lg bg-indigo-600 px-4 py-2.5 text-sm font-semibold text-white hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-500 dark:bg-indigo-500 dark:hover:bg-indigo-400";
pub const SECONDARY: &str = "inline-flex items-center justify-center rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 focus-visible:outline-2 focus-visible:outline-indigo-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:hover:bg-gray-700";
pub const PANEL: &str = "overflow-hidden rounded-xl border border-gray-200 bg-white shadow-xs dark:border-gray-700 dark:bg-gray-800";
pub const TH: &str = "px-4 py-3 text-left text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400";
pub const TD: &str = "px-4 py-4 text-sm text-gray-700 dark:text-gray-300";
const PAGE_SIZE: usize = 50;

#[derive(Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    NameAsc,
    NameDesc,
    Newest,
    Oldest,
}

impl Sort {
    fn value(self) -> &'static str {
        match self {
            Self::NameAsc => "name_asc",
            Self::NameDesc => "name_desc",
            Self::Newest => "newest",
            Self::Oldest => "oldest",
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct ListQuery {
    pub q: String,
    pub sort: Option<Sort>,
    pub page: usize,
}

impl ListQuery {
    pub fn sort(&self, named: bool) -> Sort {
        match self.sort {
            Some(Sort::NameAsc | Sort::NameDesc) if !named => Sort::Newest,
            Some(sort) => sort,
            None if named => Sort::NameAsc,
            None => Sort::Newest,
        }
    }

    pub fn needle(&self) -> String {
        self.q.trim().to_lowercase()
    }

    pub fn range(&self, total: usize) -> Range<usize> {
        let pages = total.div_ceil(PAGE_SIZE).max(1);
        let start = self.page.max(1).min(pages).saturating_sub(1) * PAGE_SIZE;
        start..(start + PAGE_SIZE).min(total)
    }

    fn page_url(&self, path: &str, page: usize, named: bool) -> String {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("q", &self.q)
            .append_pair("sort", self.sort(named).value())
            .append_pair("page", &page.to_string())
            .finish();
        format!("{path}?{query}")
    }
}

pub fn sort_history<T>(
    items: &mut [(T, Authority, Timestamp)],
    query: &ListQuery,
    id: impl Fn(&T) -> String,
) {
    items.sort_by_cached_key(|(item, _, timestamp)| (timestamp.0, id(item)));
    if query.sort(false) == Sort::Newest {
        items.reverse();
    }
}

pub fn header(
    journal_id: JournalId,
    section: &str,
    title: &str,
    description: &str,
    create: Option<&str>,
) -> Markup {
    html! {
        nav aria-label="Journal sections" class="flex flex-wrap gap-1 border-b border-gray-200 pb-4 dark:border-gray-700" {
            @for (label, path) in [("Overview", ""), ("Activities", "activity"), ("Funds", "fund"), ("Accounts", "account"), ("Donors", "donor"), ("Donations", "donation"), ("Transactions", "transaction")] {
                a href=(if path.is_empty() { format!("/journal/{journal_id}") } else { format!("/journal/{journal_id}/{path}") })
                    aria-current=[(path == section).then_some("page")]
                    class=(if path == section { "rounded-lg bg-indigo-50 px-3 py-2 text-sm font-semibold text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300" } else { "rounded-lg px-3 py-2 text-sm font-medium text-gray-600 hover:bg-gray-100 hover:text-gray-900 focus-visible:outline-2 focus-visible:outline-indigo-500 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-white" }) { (label) }
            }
        }
        header class="flex flex-wrap items-start justify-between gap-4" {
            div {
                h1 class="text-2xl font-semibold tracking-tight text-gray-900 sm:text-3xl dark:text-white" { (title) }
                p class="mt-2 max-w-2xl text-sm leading-6 text-gray-500 dark:text-gray-400" { (description) }
            }
            @if let Some(label) = create {
                a href="#create" class=(BUTTON) { (label) }
            }
        }
    }
}

pub fn controls(path: &str, query: &ListQuery, named: bool, placeholder: &str) -> Markup {
    html! {
        form method="get" action=(path) role="search" data-list-search data-default-sort=(if named { "name_asc" } else { "newest" }) class="flex flex-wrap items-end gap-3 rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800" {
            div class="min-w-0 grow basis-64" {
                label for="list-search" class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Search" }
                input id="list-search" name="q" type="search" aria-controls="list-results" value=(&query.q) placeholder=(placeholder) class=(INPUT);
            }
            div class="grow basis-48 sm:grow-0" {
                label for="list-sort" class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Sort by" }
                select id="list-sort" name="sort" aria-controls="list-results" class=(INPUT) {
                    @if named {
                        option value="name_asc" selected[query.sort(named) == Sort::NameAsc] { "Name: A–Z" }
                        option value="name_desc" selected[query.sort(named) == Sort::NameDesc] { "Name: Z–A" }
                    }
                    option value="newest" selected[query.sort(named) == Sort::Newest] { (if named { "Created: newest first" } else { "Date: newest first" }) }
                    option value="oldest" selected[query.sort(named) == Sort::Oldest] { (if named { "Created: oldest first" } else { "Date: oldest first" }) }
                }
            }
            a href=(path) data-list-reset hidden[query.q.is_empty() && query.sort.is_none()] class="px-2 py-2.5 text-sm font-medium text-gray-500 underline-offset-4 hover:underline dark:text-gray-400" { "Reset" }
        }
        script type="module" {
            @if named {
                (PreEscaped(include_str!("directory.js")))
            } @else {
                (PreEscaped(include_str!("search.js")))
            }
        }
    }
}

pub fn pagination(path: &str, query: &ListQuery, total: usize, all: usize, named: bool) -> Markup {
    let range = query.range(total);
    let page = range.start / PAGE_SIZE + 1;
    let pages = total.div_ceil(PAGE_SIZE).max(1);
    html! {
        div data-list-pagination class="flex flex-wrap items-center justify-between gap-3 text-sm text-gray-500 dark:text-gray-400" {
            p data-list-count role="status" {
                @if total == 0 { "0 results" } @else { (range.start + 1) "–" (range.end) " of " (total) }
                @if total != all { " (" (all) " total)" }
            }
            nav data-list-pages hidden[pages <= 1] aria-label="Pagination" class="flex items-center gap-3" {
                a data-list-prev hidden[page <= 1] href=(query.page_url(path, page.saturating_sub(1).max(1), named)) rel="prev" class=(SECONDARY) { "Previous" }
                span data-list-page { "Page " (page) " of " (pages) }
                a data-list-next hidden[page >= pages] href=(query.page_url(path, page + 1, named)) rel="next" class=(SECONDARY) { "Next" }
            }
        }
    }
}

pub fn empty(filtered: bool, title: &str, description: &str, path: &str) -> Markup {
    html! {
        div class="px-6 py-14 text-center" {
            h2 class="text-base font-semibold text-gray-900 dark:text-white" { (if filtered { "No matching results" } else { title }) }
            p class="mx-auto mt-2 max-w-md text-sm leading-6 text-gray-500 dark:text-gray-400" { (if filtered { "Try a different search or clear the filters to see all records." } else { description }) }
            @if filtered {
                a href=(path) class="mt-4 inline-block text-sm font-semibold text-indigo-600 hover:underline dark:text-indigo-400" { "Clear filters" }
            }
        }
    }
}

pub fn date(timestamp: Timestamp) -> Markup {
    html! {
        time datetime=(timestamp.to_rfc3339()) title=(timestamp.format("%Y-%m-%d %H:%M:%S UTC")) { (timestamp.format("%b %-d, %Y")) }
    }
}

pub struct DirectoryEntry {
    pub id: String,
    pub name: String,
    pub created: Timestamp,
    pub kind: Option<&'static str>,
    pub amount: Dollars,
    pub side: Option<&'static str>,
}

pub struct Directory {
    pub section: &'static str,
    pub title: &'static str,
    pub singular: &'static str,
    pub description: &'static str,
    pub action: &'static str,
    pub button: &'static str,
    pub types: &'static [(&'static str, i8)],
    pub type_field: &'static str,
}

fn directory_row(entry: &DirectoryEntry) -> Markup {
    html! {
        tr data-id=(&entry.id) data-name=(entry.name.to_lowercase()) data-created=(entry.created.timestamp()) data-created-subsec=(entry.created.timestamp_subsec_nanos()) class="hover:bg-gray-50 dark:hover:bg-gray-700/40" {
            th scope="row" class="min-w-40 px-4 py-4 text-left text-sm font-medium text-gray-900 dark:text-white" { (&entry.name) }
            @if let Some(kind) = entry.kind {
                td class=(TD) { span class="inline-flex rounded-md bg-gray-100 px-2 py-1 text-xs font-medium text-gray-600 dark:bg-gray-700 dark:text-gray-300" { (kind) } }
            }
            td class="whitespace-nowrap px-4 py-4 text-sm text-gray-500 dark:text-gray-400" { (date(entry.created)) }
            td class="whitespace-nowrap px-4 py-4 text-right text-sm font-medium tabular-nums text-gray-900 dark:text-gray-100" {
                (entry.amount)
                @if let Some(side) = entry.side { " " span class="text-xs font-normal text-gray-500 dark:text-gray-400" { (side) } }
            }
        }
    }
}

pub fn directory(
    journal_id: JournalId,
    query: &ListQuery,
    mut entries: Vec<DirectoryEntry>,
    config: Directory,
) -> Markup {
    let path = format!("/journal/{journal_id}/{}", config.section);
    let all = entries.len();
    let needle = query.needle();
    let sort = query.sort(true);
    entries.sort_by_cached_key(|entry| match sort {
        Sort::NameAsc | Sort::NameDesc => {
            (entry.name.to_lowercase(), entry.created.0, entry.id.clone())
        }
        Sort::Newest | Sort::Oldest => (String::new(), entry.created.0, entry.id.clone()),
    });
    if matches!(sort, Sort::NameDesc | Sort::Newest) {
        entries.reverse();
    }
    // Keep every row in the initial document; client controls operate on this full collection.
    // The filtered first page also renders on the server for ordinary GET navigation.
    let filtered: Vec<_> = entries
        .iter()
        .filter(|entry| entry.name.to_lowercase().contains(&needle))
        .collect();
    let total = filtered.len();
    let name_field = format!("{}_name", config.section);
    html! {
        div class="mx-auto flex w-full max-w-6xl flex-col gap-6" {
            (header(journal_id, config.section, config.title, config.description, Some(config.button)))
            (controls(&path, query, true, &format!("Search {} by name…", config.title.to_lowercase())))
            div id="list-results" data-list-results role="region" aria-label="Search results" tabindex="-1" class="flex flex-col gap-6" {
                (pagination(&path, query, total, all, true))
                div class=(PANEL) {
                    div data-directory-empty hidden[total > 0] {
                        div data-directory-no-matches hidden[needle.is_empty()] {
                            (empty(true, "", "", &path))
                        }
                        div data-directory-no-entries hidden[!needle.is_empty()] {
                            (empty(false, &format!("No {} yet", config.title.to_lowercase()), &format!("Add your first {} using the form below.", config.singular), &path))
                        }
                    }
                    div data-directory-table hidden[total == 0] class="overflow-x-auto" {
                        table class="w-full" {
                            thead class="border-b border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-900/40" {
                                tr {
                                    th scope="col" class=(TH) { "Name" }
                                    @if !config.types.is_empty() { th scope="col" class=(TH) { "Type" } }
                                    th scope="col" class=(TH) { "Created" }
                                    th scope="col" class="px-4 py-3 text-right text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400" { (if config.section == "donor" { "Total donated" } else { "Balance" }) }
                                }
                            }
                            tbody data-directory-visible class="divide-y divide-gray-100 dark:divide-gray-700" {
                                @for entry in &filtered[query.range(total)] { (directory_row(entry)) }
                            }
                        }
                    }
                }
                div data-directory-bottom-pagination hidden[total <= PAGE_SIZE] {
                    (pagination(&path, query, total, all, true))
                }
            }
            template data-directory-rows data-page-size=(PAGE_SIZE) {
                table { tbody {
                    @for entry in &entries { (directory_row(entry)) }
                } }
            }
            section id="create" aria-labelledby="create-heading" class="scroll-mt-6 rounded-xl border border-gray-200 bg-white p-5 sm:p-6 dark:border-gray-700 dark:bg-gray-800" {
                div class="mb-5" {
                    h2 id="create-heading" class="text-lg font-semibold text-gray-900 dark:text-white" { (config.button) }
                    p class="mt-1 text-sm text-gray-500 dark:text-gray-400" { "Add a new " (config.singular) " to this journal." }
                }
                form method="post" action=(format!("/journal/{journal_id}/{}", config.action)) class="flex max-w-2xl flex-col gap-5" {
                    div {
                        label for=(&name_field) class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Name" }
                        input type="text" id=(&name_field) name=(&name_field) required maxlength="64" placeholder=(format!("{} name", config.singular)) class=(INPUT);
                    }
                    @if !config.types.is_empty() {
                        fieldset {
                            legend class="mb-2 text-sm font-medium text-gray-700 dark:text-gray-200" { "Type" }
                            div class="flex flex-wrap gap-3" {
                                @for (index, (label, value)) in config.types.iter().enumerate() {
                                    label class="flex cursor-pointer items-center gap-2 rounded-lg border border-gray-200 px-4 py-3 text-sm text-gray-700 has-checked:border-indigo-500 has-checked:bg-indigo-50 dark:border-gray-600 dark:text-gray-200 dark:has-checked:bg-indigo-500/10" {
                                        input type="radio" name=(config.type_field) value=(value) required checked[index == 0] class="accent-indigo-600";
                                        (label)
                                    }
                                }
                            }
                        }
                    }
                    div { button type="submit" class=(BUTTON) { (config.button) } }
                }
            }
        }
    }
}

/// Resolve authors in one query for the displayed page, including former journal members.
pub async fn authors(
    state: &crate::StateType,
    authorities: impl Iterator<Item = Authority>,
) -> crate::journal::error::JournalResult<std::collections::HashMap<crate::authn::UserId, String>> {
    let ids: std::collections::HashSet<_> = authorities
        .filter_map(|authority| match authority.actor() {
            crate::authority::Actor::User(id) => Some(id),
            _ => None,
        })
        .collect();
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    Ok(state
        .authn_service
        .fetch_users(&ids.into_iter().collect::<Vec<_>>())
        .await?
        .into_iter()
        .map(|user| (user.id, user.email.to_string()))
        .collect())
}

pub fn author(
    authority: Authority,
    authors: &std::collections::HashMap<crate::authn::UserId, String>,
) -> &str {
    match authority.actor() {
        crate::authority::Actor::User(id) => authors
            .get(&id)
            .map(String::as_str)
            .unwrap_or("Unknown user"),
        crate::authority::Actor::System => "System",
        crate::authority::Actor::Anonymous => "Anonymous",
    }
}

pub fn money(cents: u128) -> String {
    use thousands::Separable;
    format!(
        "${}.{:02}",
        (cents / 100).separate_with_commas(),
        cents % 100
    )
}

pub fn history_limit(count: usize) -> Markup {
    html! {
        @if count == 500 {
            p class="rounded-lg bg-indigo-50 px-4 py-3 text-sm leading-6 text-indigo-800 dark:bg-indigo-500/10 dark:text-indigo-200" {
                "Showing the latest 500 records. Search and sorting apply to these records."
            }
        }
    }
}
