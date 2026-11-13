// The complete directory arrives with the page. All list interactions stay in memory.
const form = document.querySelector('[data-list-search]');
const results = document.querySelector('[data-list-results]');
const template = document.querySelector('[data-directory-rows]');

if (form && results && template) {
    const search = form.elements.namedItem('q');
    const sort = form.elements.namedItem('sort');
    const reset = form.querySelector('[data-list-reset]');
    const status = form.querySelector('[data-list-status]');
    const tbody = results.querySelector('[data-directory-visible]');
    const table = results.querySelector('[data-directory-table]');
    const empty = results.querySelector('[data-directory-empty]');
    const noMatches = results.querySelector('[data-directory-no-matches]');
    const noEntries = results.querySelector('[data-directory-no-entries]');
    const bottomPagination = results.querySelector('[data-directory-bottom-pagination]');
    const pagers = results.querySelectorAll('[data-list-pagination]');
    const pageSize = Number(template.dataset.pageSize);
    const path = new URL(form.action).pathname;
    const items = Array.from(template.content.querySelectorAll('tr'), row => ({
        row,
        id: row.dataset.id,
        name: row.dataset.name,
        created: Number(row.dataset.created),
        subsecond: Number(row.dataset.createdSubsec),
    }));
    let page = 1;
    let composing = false;
    let sortedBy;
    let sortedItems = items;
    template.remove();

    const compare = (a, b) => a < b ? -1 : a > b ? 1 : 0;

    function pageUrl(number) {
        const url = new URL(form.action);
        url.searchParams.set('q', search.value);
        url.searchParams.set('sort', sort.value);
        url.searchParams.set('page', String(number));
        return url;
    }

    function render({ history = 'replace', scroll = false } = {}) {
        const needle = search.value.trim().toLowerCase();
        if (sortedBy !== sort.value) {
            sortedBy = sort.value;
            const byName = sortedBy === 'name_asc' || sortedBy === 'name_desc';
            sortedItems = [...items].sort((a, b) => (byName ? compare(a.name, b.name) : 0)
                || compare(a.created, b.created) || compare(a.subsecond, b.subsecond)
                || compare(a.id, b.id));
            if (sortedBy === 'name_desc' || sortedBy === 'newest') sortedItems.reverse();
        }
        const filtered = sortedItems.filter(item => item.name.includes(needle));

        const pages = Math.max(1, Math.ceil(filtered.length / pageSize));
        page = Math.min(Math.max(1, page), pages);
        const start = (page - 1) * pageSize;
        const end = Math.min(start + pageSize, filtered.length);
        tbody.replaceChildren(...filtered.slice(start, end).map(item => item.row));
        table.hidden = filtered.length === 0;
        empty.hidden = filtered.length !== 0;
        noMatches.hidden = !needle;
        noEntries.hidden = Boolean(needle);
        bottomPagination.hidden = pages <= 1;
        reset.hidden = !search.value && sort.value === form.dataset.defaultSort;

        let count = filtered.length ? `${start + 1}–${end} of ${filtered.length}` : '0 results';
        if (filtered.length !== items.length) count += ` (${items.length} total)`;
        for (const pager of pagers) {
            pager.querySelector('[data-list-count]').textContent = count;
            pager.querySelector('[data-list-pages]').hidden = pages <= 1;
            pager.querySelector('[data-list-page]').textContent = `Page ${page} of ${pages}`;
            const previous = pager.querySelector('[data-list-prev]');
            previous.hidden = page <= 1;
            previous.href = pageUrl(Math.max(1, page - 1));
            const next = pager.querySelector('[data-list-next]');
            next.hidden = page >= pages;
            next.href = pageUrl(Math.min(pages, page + 1));
        }
        const url = pageUrl(page);
        if (history === 'push') window.history.pushState(null, '', url);
        else if (history === 'replace') window.history.replaceState(null, '', url);
        if (scroll) {
            results.focus({ preventScroll: true });
            results.scrollIntoView({ block: 'start' });
        }
    }

    function restore(url) {
        search.value = url.searchParams.get('q') || '';
        const value = url.searchParams.get('sort') || form.dataset.defaultSort;
        sort.value = Array.from(sort.options).some(option => option.value === value)
            ? value : form.dataset.defaultSort;
        const number = Number(url.searchParams.get('page') || 1);
        page = Number.isSafeInteger(number) && number > 0 ? number : 1;
    }

    function filter() {
        if (composing) return;
        page = 1;
        render();
    }

    search.addEventListener('input', filter);
    search.addEventListener('compositionstart', () => { composing = true; });
    search.addEventListener('compositionend', () => {
        composing = false;
        filter();
    });
    sort.addEventListener('change', filter);
    form.addEventListener('submit', event => {
        event.preventDefault();
        filter();
    });

    function navigate(event, link, scroll) {
        if (!link || event.defaultPrevented || event.button !== 0 || event.metaKey
            || event.ctrlKey || event.shiftKey || event.altKey || link.hasAttribute('download')
            || (link.target && link.target !== '_self')) return;
        const url = new URL(link.href);
        if (url.origin !== window.location.origin || url.pathname !== path) return;
        event.preventDefault();
        restore(url);
        render({ history: 'push', scroll });
    }

    reset.addEventListener('click', event => navigate(event, reset, false));
    results.addEventListener('click', event => {
        navigate(event, event.target.closest('a[href]'), true);
    });
    window.addEventListener('popstate', () => {
        if (window.location.pathname !== path) return;
        restore(new URL(window.location.href));
        render({ history: 'none' });
    });
    restore(new URL(window.location.href));
    render({ history: 'none' });
}
