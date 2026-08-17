# Transaction V2

## Transaction

```rust
struct Transaction {
    id: TransactionId,
    date: Date,
    period: Period,
    entries: Vec<Entry>,
}
```

## Entry

```rust
struct Entry {
    id: EntryId,
    journal: JournalId,
    amount: Amount,
    side: EntrySide,
    kind: EntryKind,
}

enum EntryKind {
    Account {
        account: AccountId,
    },
    Activity {
        activity: ActivityId,
        fund: Fund,
    },
    Transfer {
        activity: ActivityId,
        fund: Fund,
    },
}

enum EntrySide {
    Debit,
    Credit,
}

enum AccountKind {
    Asset,
    Liability,
}

enum ActivityKind {
    Income,
    Expense,
    Transfer,
}
```

## Notes

The date and period do not need to match. They generally should, but they may
diverge when necessary.

The term "account" is often overloaded in accounting. It may refer to where value is
held or owed, such as a bank account or credit card; to revenue or expense
activity; or to equity reserved for a particular nonprofit purpose. These do not
form one interchangeable set. They may be separate dimensions of the same entry,
so they should not be modeled as mutually exclusive accounts.

Names in the transaction model should make sense to lay people doing accounting
and reading reports for a small local church.

A fund is a designated purpose, similar to how an equity account may be used in
nonprofit accounting. Revenue, expenses, and donations are activities. A fund
is referenced through an activity or transfer, and every activity is associated
with a fund.

Accounts represent the asset and liability side. An account entry does not need
an additional dimension, and a transaction may transfer value between accounts
without involving a fund or activity.

Every activity entry has a fund. The general fund is the default when no named,
designated fund applies; it is a fund value rather than the absence of one.

References to journals, accounts, and activities do not include `id` in the
field name. Their types identify them as IDs.

An amount includes its currency. Debit or credit is the entry's side rather than
part of the amount.

Each entry has a stable ID so it can be referenced by reconciliations and by
extensions such as contributions and invoices.
