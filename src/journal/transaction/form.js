const form = document.getElementById("transaction-form");
const controls = form.querySelector("[data-transaction-controls]");
const entries = form.querySelector("[data-transaction-entries]");
const addButton = form.querySelector("[data-add-entry]");
const error = form.querySelector("[data-transaction-error]");
const entryTemplate = document.getElementById("transaction-entry");
const optionsTemplate = document.getElementById("transaction-options");
const dialog = document.getElementById("transaction-entry-dialog");
const editor = document.getElementById("transaction-entry-editor");
const heading = dialog.querySelector("#transaction-entry-heading");
const saveButton = editor.querySelector("[data-save-entry]");
const kindChoices = editor.querySelectorAll('[name="kind"]');
const groups = editor.querySelectorAll("[data-entry-group]");
const transactionEntries = [];
let editingIndex = null;
let entryOpener;

function normalize(name) {
    return name.trim().normalize("NFKC").toLowerCase();
}

function compareOptions(a, b) {
    return a.name.localeCompare(b.name) || a.id.localeCompare(b.id);
}

const options = Object.fromEntries(
    Array.from(optionsTemplate.content.querySelectorAll("[data-options]"), select => [
        select.dataset.options,
        Array.from(select.options, option => ({
            id: option.value,
            name: option.textContent,
            key: normalize(option.textContent),
        })).sort(compareOptions),
    ]),
);

function editDistance(left, right) {
    const a = Array.from(left);
    const b = Array.from(right);
    let previous = Array.from({ length: b.length + 1 }, (_, index) => index);

    for (let i = 0; i < a.length; i += 1) {
        const current = [i + 1];
        for (let j = 0; j < b.length; j += 1) {
            current.push(Math.min(
                previous[j + 1] + 1,
                current[j] + 1,
                previous[j] + (a[i] === b[j] ? 0 : 1),
            ));
        }
        previous = current;
    }

    return previous[b.length];
}

const pickers = Object.fromEntries(
    Array.from(editor.querySelectorAll("[data-entry-picker]"), select => [
        select.dataset.entryPicker,
        { select, search: editor.querySelector(`[data-option-search="${select.dataset.entryPicker}"]`) },
    ]),
);

function renderPicker(key, oppositeKey) {
    const { select, search } = pickers[key];
    const query = normalize(search.value);
    const selected = options[key].find(option => option.id === select.value);
    const opposite = oppositeKey && options[oppositeKey].find(
        option => option.id === pickers[oppositeKey].select.value,
    );
    let matches = options[key].filter(option => option.key.includes(query));

    if (opposite) {
        matches = matches
            .map(option => ({ option, distance: editDistance(option.key, opposite.key) }))
            .sort((a, b) => a.distance - b.distance || compareOptions(a.option, b.option))
            .map(({ option }) => option);
    }

    select.replaceChildren(...matches.map(option => new Option(option.name, option.id)));
    if (!matches.length) {
        const empty = new Option("No matching names", "");
        empty.disabled = true;
        select.append(empty);
    }
    if (selected && !matches.includes(selected)) {
        const group = document.createElement("optgroup");
        group.label = "Current selection";
        group.append(new Option(selected.name, selected.id));
        select.append(group);
    }
    if (selected) select.value = selected.id;
    else select.selectedIndex = -1;
}

for (const [key, oppositeKey] of [["account"], ["activity", "fund"]]) {
    const { search, select } = pickers[key];
    search.addEventListener("input", () => renderPicker(key, oppositeKey));
    search.addEventListener("keydown", event => {
        if (event.key === "Enter" && !event.isComposing) {
            event.preventDefault();
            select.focus();
        }
    });
    if (oppositeKey) {
        select.addEventListener("change", () => renderPicker(oppositeKey, key));
    }
}

function showEntryFields() {
    const kind = editor.querySelector('[name="kind"]:checked')?.value;
    for (const group of groups) {
        const active = Boolean(kind) && (group.dataset.entryGroup === kind || group.dataset.entryGroup === "details");
        group.hidden = !active;
        group.disabled = !active;
    }
}

function openEntry(index = null) {
    editingIndex = index;
    entryOpener = index === null ? addButton : entries.children[index].querySelector("[data-edit-entry]");
    const entry = transactionEntries[index];
    heading.textContent = entry ? "Edit entry" : "Add entry";
    saveButton.textContent = entry ? "Save changes" : "Add entry";

    editor.reset();
    for (const key of Object.keys(pickers)) {
        pickers[key].select.selectedIndex = -1;
        renderPicker(key);
    }

    if (entry) {
        editor.querySelector(`[name="kind"][value="${entry.kind}"]`).checked = true;
        editor.elements.namedItem("amount").value = entry.amount;
        editor.elements.namedItem("entry_side").value = entry.entry_side;
        if (entry.kind === "account") {
            pickers.account.select.value = entry.account_id;
        } else {
            pickers.activity.select.value = entry.activity_id;
            pickers.fund.select.value = entry.fund_id;
            editor.elements.namedItem("transfer").checked = entry.transfer;
            renderPicker("activity", "fund");
            renderPicker("fund", "activity");
        }
    }

    showEntryFields();
    dialog.showModal();
    const kind = editor.querySelector('[name="kind"]:checked')?.value;
    (kind ? pickers[kind].search : kindChoices[0]).focus();
}

function renderEntries() {
    const rows = transactionEntries.map((entry, index) => {
        const row = entryTemplate.content.firstElementChild.cloneNode(true);
        const key = entry.kind;
        const name = options[key].find(option => option.id === entry[`${key}_id`]).name;
        let detail = "Account";
        if (key === "activity") {
            const fund = options.fund.find(option => option.id === entry.fund_id).name;
            detail = entry.transfer ? `${fund} · Internal transfer` : fund;
        }

        row.dataset.entryIndex = index;
        row.querySelector("[data-entry-name]").textContent = name;
        row.querySelector("[data-entry-detail]").textContent = detail;
        row.querySelector("[data-entry-amount]").textContent = `${entry.entry_side} · $${entry.amount}`;
        row.querySelector("[data-edit-entry]").setAttribute("aria-label", `Edit entry ${index + 1} for ${name}`);
        row.querySelector("[data-remove-entry]").setAttribute("aria-label", `Remove entry ${index + 1} for ${name}`);
        return row;
    });
    entries.replaceChildren(...rows);
}

addButton.addEventListener("click", () => openEntry());
for (const choice of kindChoices) choice.addEventListener("change", showEntryFields);
editor.querySelector("[data-cancel-entry]").addEventListener("click", () => dialog.close());
dialog.addEventListener("close", () => {
    if (!dialog.open && entryOpener?.isConnected) entryOpener.focus();
});

entries.addEventListener("click", event => {
    const button = event.target.closest("[data-edit-entry], [data-remove-entry]");
    if (!button) return;

    const index = Number(button.closest("[data-entry-index]").dataset.entryIndex);
    if (button.hasAttribute("data-edit-entry")) {
        openEntry(index);
    } else {
        transactionEntries.splice(index, 1);
        renderEntries();
        const next = entries.children[index] ?? entries.lastElementChild;
        (next?.querySelector("[data-remove-entry]") ?? addButton).focus();
    }
});

editor.addEventListener("submit", event => {
    event.preventDefault();

    const fields = new FormData(editor);
    const kind = fields.get("kind");
    const entry = {
        kind,
        amount: fields.get("amount").trim(),
        entry_side: fields.get("entry_side"),
    };
    if (kind === "account") {
        entry.account_id = fields.get("account_id");
    } else {
        entry.activity_id = fields.get("activity_id");
        entry.fund_id = fields.get("fund_id");
        entry.transfer = fields.has("transfer");
    }

    const index = editingIndex;
    if (index === null) transactionEntries.push(entry);
    else transactionEntries[index] = entry;

    dialog.close();
    renderEntries();
    const editButton = index === null ? null : entries.children[index].querySelector("[data-edit-entry]");
    (editButton ?? addButton).focus();
});

form.addEventListener("submit", async event => {
    event.preventDefault();
    if (controls.disabled) return;

    const body = JSON.stringify({
        period: form.elements.namedItem("period").value,
        memo: form.elements.namedItem("memo").value || null,
        entries: transactionEntries,
    });
    error.hidden = true;
    controls.disabled = true;
    form.setAttribute("aria-busy", "true");
    const fallback = "Could not confirm that the transaction was saved. Check the transaction list before trying again.";

    let message;
    try {
        const response = await fetch(form.action, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body,
        });
        if (response.redirected && response.ok) {
            window.location.assign(response.url);
            return;
        }
        message = response.status >= 400 && response.status < 500
            ? (await response.text()).trim() || fallback
            : fallback;
    } catch {
        message = fallback;
    }

    controls.disabled = false;
    form.removeAttribute("aria-busy");
    error.textContent = message;
    error.hidden = false;
    error.focus();
});
form.hidden = false;
