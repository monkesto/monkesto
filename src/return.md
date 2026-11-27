# Return

A return is a received [instrument](instrument.md) that went back after it was
deposited: a returned check, a card chargeback, or a reversed electronic
payment. It is a later event, not a correction. The [deposit](deposit.md) was
right, and then the money was taken back.

```rust
struct Return {
    id: ReturnId,
    date: Date,
    account: AccountId,
    deposit: DepositId,
    instrument: InstrumentId,
    sources: Vec<ReturnSource>,
    transaction: TransactionId,
    account_entry: EntryId,
    submission: Option<Submission>,
}

/// The part of a return that reverses one source of the deposit.
struct ReturnSource {
    source: SourceId,
    lines: Vec<Line>,
}
```

## Shape

A return refers to one instrument of one deposit. The instrument's ID is only
meaningful within its deposit, so the return holds both. The instrument's
claims identify which sources, and so which parties, are affected.

Its date is when the money was taken back, which puts its transaction in that
date's period. The deposit is not edited, and earlier periods keep showing the
money as received, which was true at the time.

A return has one part for each source that claimed the instrument. By default,
each part's [lines](line.md) reverse that source's lines, on the same
activities and funds. Because each part names its source, reports such as
donor statements know whose giving the return reduces. Any bank fee for the
return is a separate expense.

## Completion

A return can be submitted only when:

- It has a part for every source that claimed the instrument, and no others.
- Each part's lines net to its source's claim on the instrument.

Together, the parts then net to the instrument's amount. The transaction's
entries are generated from the return: the account entry taking the amount out
of the account, and one activity entry per line. The return holds the ID of the
account entry, and each line holds the ID of its entry.

## Editing

A return is submitted, edited, and resubmitted as described in
[submission](submission.md). Its transaction is edited only through the return.
