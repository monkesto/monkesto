(() => {
    const root = document.getElementById("jewel-import");
    const baseUrl = root.dataset.url;
    const statusText = document.getElementById("jewel-status");
    const progress = document.getElementById("jewel-progress");
    const counts = document.getElementById("jewel-counts");
    const errorBox = document.getElementById("jewel-error");
    const review = document.getElementById("jewel-review");
    const form = document.getElementById("jewel-confirm");
    const confirmButton = document.getElementById("jewel-confirm-button");
    const retryButton = document.getElementById("jewel-retry");
    const money = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });
    let currentPhase = "preparing";
    let renderedReport = false;
    let submitting = false;
    let timer;

    function element(tag, text, className) {
        const node = document.createElement(tag);
        if (text !== undefined) node.textContent = text;
        if (className) node.className = className;
        return node;
    }

    function showError(message) {
        errorBox.textContent = (typeof message === "string" && message.trim())
            || "The import could not continue. Please try again or contact support.";
        errorBox.hidden = false;
    }

    async function request(path, options = {}) {
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 15000);
        try {
            const response = await fetch(`${baseUrl}/${path}`, {
                ...options,
                cache: "no-store",
                signal: controller.signal,
                headers: { Accept: "application/json", ...options.headers },
            });
            if (response.redirected) {
                window.location.assign(response.url);
                throw new Error("Sign in again to continue monitoring this import.");
            }
            if (!response.ok) {
                const body = (await response.text()).trim();
                const contentType = response.headers.get("Content-Type") || "";
                let message = "";
                if (contentType.includes("application/json")) {
                    try {
                        const error = JSON.parse(body);
                        if (typeof error.message === "string") message = error.message;
                        else if (typeof error.error === "string") message = error.error;
                    } catch {
                        // Invalid or empty JSON still gets a useful HTTP error below.
                    }
                } else if (!contentType.includes("text/html")) {
                    message = body;
                }
                throw new Error(message.trim()
                    || `The import request failed (HTTP ${response.status}). Please try again or contact support.`);
            }
            return await response.json();
        } catch (error) {
            if (error.name === "AbortError") {
                throw new Error("The import request timed out. Check the import status before retrying.");
            }
            throw error;
        } finally {
            clearTimeout(timeout);
        }
    }

    function renderReport(report) {
        document.getElementById("jewel-summary").textContent =
            `${report.accounts.toLocaleString()} accounts, ${report.donors.toLocaleString()} donors, ` +
            `${report.journals.toLocaleString()} transactions, ${report.reversals.toLocaleString()} reversals, ` +
            `and ${(report.fund_reclassifications ?? 0).toLocaleString()} fund corrections. ` +
            `${report.issues.length.toLocaleString()} offerings need a reconciliation choice.`;
        const issues = document.getElementById("jewel-issues");
        issues.replaceChildren();
        if (report.issues.length === 0) {
            issues.append(element("p", "No offerings require a reconciliation choice. You can start the import."));
        } else {
            issues.append(element("p", "Choose a treatment for each discrepancy. Keeping contributions may change balances; an adjustment preserves donor details while matching the deposit. Transaction only preserves the deposit without importing that offering's donor details.", "text-sm text-gray-600 dark:text-gray-400"));
        }
        for (const issue of report.issues) {
            const card = element("section", undefined, "space-y-3 rounded-lg bg-white p-5 shadow-sm dark:bg-gray-800");
            const headingId = `jewel-offering-${issue.offering_id}`;
            const heading = element("h3", `Offering ${issue.offering_id} — ${issue.date}`, "font-semibold");
            heading.id = headingId;
            card.append(heading);
            card.append(element("p", `Deposit date: ${issue.deposit_date}. OfferingTotal: ${money.format(issue.offering_total / 100)}. Envelopes: ${money.format(issue.envelope_total / 100)}. Donor contributions after offsets: ${money.format(issue.positive_contributions / 100)}. Net contributions: ${money.format(issue.contribution_total / 100)}. Bank journal: ${money.format(issue.bank_total / 100)}.`, "text-sm"));
            for (const problem of issue.problems) card.append(element("p", problem, "text-sm"));
            if (issue.differences.length > 0) {
                const wrapper = element("div", undefined, "overflow-x-auto");
                const table = element("table", undefined, "w-full text-left text-sm");
                const header = element("tr");
                for (const title of ["Account / fund", "From contributions", "Deposit journal"]) {
                    const cell = element("th", title, "px-2 py-1");
                    cell.scope = "col";
                    header.append(cell);
                }
                const tableHead = element("thead");
                tableHead.append(header);
                table.append(tableHead);
                const body = element("tbody");
                for (const difference of issue.differences) {
                    const row = element("tr");
                    for (const value of [difference.name, money.format(difference.contributions / 100), money.format(difference.journal / 100)]) {
                        row.append(element("td", value, "px-2 py-1"));
                    }
                    body.append(row);
                }
                table.append(body);
                wrapper.append(table);
                card.append(wrapper);
            }
            const select = element("select", undefined, "w-full rounded-md border border-gray-300 bg-white p-2 text-gray-900 dark:border-gray-600 dark:bg-gray-900 dark:text-gray-100");
            select.dataset.offering = issue.offering_id;
            select.required = true;
            select.setAttribute("aria-labelledby", headingId);
            select.append(new Option("Choose how to reconcile this offering", ""));
            select.append(new Option("Keep deposit — transaction only, omit donor details", "journal_only"));
            if (issue.has_contributions) {
                select.append(new Option("Keep contributions — balances may differ from the deposit", "contributions"));
                select.append(new Option("Keep donor details + adjustment to match the deposit", "adjustment"));
            }
            card.append(select);
            issues.append(card);
        }

        renderedReport = true;
    }

    function renderStatus(status) {
        if (status.phase === "review" && ["saving", "projecting", "complete"].includes(currentPhase)) return;
        currentPhase = status.phase;
        statusText.textContent = status.message;
        review.hidden = status.phase !== "review";
        confirmButton.disabled = status.phase !== "review" || submitting;
        retryButton.hidden = status.phase !== "failed";
        document.getElementById("jewel-complete").hidden = status.phase !== "complete";
        const stages = ["jewel-stage-prepare", "jewel-stage-save", "jewel-stage-project"];
        const activeStage = { preparing: 0, review: 0, saving: 1, projecting: 2 }[status.phase];
        stages.forEach((id, index) => {
            const stage = document.getElementById(id);
            stage.classList.toggle("font-semibold", index === activeStage);
            if (index === activeStage) stage.setAttribute("aria-current", "step");
            else stage.removeAttribute("aria-current");
        });
        if (status.total > 0) {
            progress.max = status.total;
            progress.value = status.completed;
            const percent = Math.floor(status.completed * 100 / status.total);
            counts.textContent = `${status.completed.toLocaleString()} / ${status.total.toLocaleString()} records — ${percent}%`;
        } else {
            progress.removeAttribute("value");
            counts.textContent = "";
        }
        progress.hidden = ["review", "failed"].includes(status.phase);
        if (status.phase === "review" && status.report && !renderedReport) renderReport(status.report);
        if (status.phase === "failed") showError(status.message);
        else errorBox.hidden = true;
    }

    function schedulePoll() {
        clearTimeout(timer);
        if (!["complete", "failed"].includes(currentPhase)) timer = setTimeout(poll, 1000);
    }

    async function poll() {
        try {
            const status = await request("status");
            if (!submitting) renderStatus(status);
        } catch (error) {
            showError(`Could not refresh progress: ${error.message}. Retrying; do not start another import.`);
            retryButton.hidden = false;
        }
        schedulePoll();
    }

    async function start() {
        clearTimeout(timer);
        retryButton.disabled = true;
        renderedReport = false;
        currentPhase = "preparing";
        statusText.textContent = "Opening the import…";
        errorBox.hidden = true;
        review.hidden = true;
        confirmButton.disabled = true;
        progress.hidden = false;
        try {
            renderStatus(await request("start", { method: "POST" }));
            schedulePoll();
        } catch (error) {
            currentPhase = "failed";
            statusText.textContent = "Could not start the import.";
            progress.hidden = true;
            showError(error.message);
            retryButton.hidden = false;
        } finally {
            retryButton.disabled = false;
        }
    }

    form.addEventListener("submit", async (event) => {
        event.preventDefault();
        if (submitting || !form.reportValidity()) return;
        submitting = true;
        confirmButton.disabled = true;
        const offerings = Object.fromEntries(Array.from(form.querySelectorAll("select[data-offering]"), select => [select.dataset.offering, select.value]));
        try {
            const status = await request("confirm", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ offerings }),
            });
            submitting = false;
            renderStatus(status);
        } catch (error) {
            showError(`Could not confirm the import: ${error.message}. Checking the import status before retrying.`);
            submitting = false;
            confirmButton.disabled = true;
        }
        schedulePoll();
    });
    retryButton.addEventListener("click", start);
    start();
})();
