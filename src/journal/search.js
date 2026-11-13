// Keep the GET form as a fallback, and update only results so typing and draft forms survive.
const form = document.querySelector('[data-list-search]');
const results = document.querySelector('[data-list-results]');

if (form && results) {
    const search = form.elements.namedItem('q');
    const sort = form.elements.namedItem('sort');
    const reset = form.querySelector('[data-list-reset]');
    const status = form.querySelector('[data-list-status]');
    const path = new URL(form.action).pathname;
    let timer;
    let controller;
    let revision = 0;
    let composing = false;
    let useNativeSubmit = false;

    function updateReset() {
        reset.hidden = !search.value && sort.value === form.dataset.defaultSort;
    }

    function cancel() {
        clearTimeout(timer);
        controller?.abort();
        controller = null;
        revision += 1;
        results.removeAttribute('aria-busy');
    }

    function formUrl() {
        const url = new URL(form.action);
        url.search = new URLSearchParams(new FormData(form)).toString();
        return url;
    }

    function syncControls(url) {
        search.value = url.searchParams.get('q') || '';
        const value = url.searchParams.get('sort') || form.dataset.defaultSort;
        sort.value = Array.from(sort.options).some(option => option.value === value)
            ? value : form.dataset.defaultSort;
        updateReset();
    }

    async function load(url, { history = 'replace', scroll = false } = {}) {
        cancel();
        const currentRevision = revision;
        controller = new AbortController();
        useNativeSubmit = false;
        results.setAttribute('aria-busy', 'true');
        updateReset();

        try {
            const response = await fetch(url, {
                signal: controller.signal,
                headers: { Accept: 'text/html' },
                credentials: 'same-origin',
            });
            if (currentRevision !== revision) return;
            if (response.redirected) {
                window.location.assign(response.url);
                return;
            }
            if (!response.ok) throw new Error(`Search failed: ${response.status}`);
            const html = await response.text();
            if (currentRevision !== revision) return;
            const page = new DOMParser().parseFromString(html, 'text/html');
            const nextResults = page.querySelector('[data-list-results]');
            if (!nextResults) throw new Error('Search results missing');

            const focusResults = results.contains(document.activeElement);
            results.replaceChildren(...nextResults.childNodes);
            if (history === 'push') {
                window.history.pushState(null, '', url);
            } else if (history === 'replace') {
                window.history.replaceState(null, '', url);
            }
            if (focusResults) results.focus({ preventScroll: true });
            if (scroll) results.scrollIntoView({ block: 'start' });
            const count = results.querySelector('[role="status"]')?.textContent.trim();
            status.textContent = count ? `Results updated: ${count}.` : 'Results updated.';
        } catch (error) {
            if (currentRevision !== revision || error.name === 'AbortError') return;
            // The next explicit submit uses a regular navigation if the live update failed.
            useNativeSubmit = true;
            status.textContent = 'Could not update results. Try again, or press Apply to reload.';
        } finally {
            if (currentRevision === revision) {
                results.removeAttribute('aria-busy');
                controller = null;
            }
        }
    }

    function schedule() {
        cancel(); // Invalidate an older response as soon as the text changes.
        updateReset();
        if (!composing) timer = setTimeout(() => load(formUrl()), 250);
    }

    search.addEventListener('input', schedule);
    search.addEventListener('compositionstart', () => {
        composing = true;
        cancel();
    });
    search.addEventListener('compositionend', () => {
        composing = false;
        schedule();
    });
    sort.addEventListener('change', () => load(formUrl()));
    form.addEventListener('submit', event => {
        if (useNativeSubmit) return;
        event.preventDefault();
        if (!composing) load(formUrl());
    });

    function navigate(event, link, scroll) {
        if (!link || event.defaultPrevented || event.button !== 0 || event.metaKey
            || event.ctrlKey || event.shiftKey || event.altKey || link.hasAttribute('download')
            || (link.target && link.target !== '_self')) return;
        const url = new URL(link.href);
        if (url.origin !== window.location.origin || url.pathname !== path) return;
        event.preventDefault();
        syncControls(url);
        load(url, { history: 'push', scroll });
    }

    reset.addEventListener('click', event => navigate(event, reset, false));
    results.addEventListener('click', event => {
        navigate(event, event.target.closest('a[href]'), true);
    });
    window.addEventListener('popstate', () => {
        if (window.location.pathname !== path) return;
        const url = new URL(window.location.href);
        syncControls(url);
        load(url, { history: 'none' });
    });
    window.addEventListener('pagehide', cancel);
    updateReset();
}
