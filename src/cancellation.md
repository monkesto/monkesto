# Cancellation

A cancellation undoes a [payment](payment.md) whose money did not end up
leaving: a check that went stale, was lost, or was stopped after it was sent,
or an electronic payment that was reversed. It is a later event, not a
correction. The payment was right, and then it was cancelled.

```rust
struct Cancellation {
    id: CancellationId,
    date: Date,
    account: AccountId,
    payment: PaymentId,
    lines: Vec<Line>,
    transaction: TransactionId,
    account_entry: EntryId,
    submission: Option<Submission>,
}
```

## Shape

A cancellation refers to one payment. A payment has one
[instrument](instrument.md), so the payment identifies it.

Its date is when the payment was cancelled, which puts its transaction in that
date's period. The payment is not edited, and earlier periods keep showing the
money as paid, which was true at the time.

By default, its [lines](line.md) reverse the payment's lines, on the same
activities and funds.

A lost check is usually replaced. The replacement is a new payment, separate
from the cancellation.

## Completion

A cancellation can be submitted when its lines net to the payment's amount. The
transaction's entries are generated from it: the account entry returning the
amount to the account, and one activity entry per line. The cancellation holds
the ID of the account entry, and each line holds the ID of its entry.

## Editing

A cancellation is submitted, edited, and resubmitted as described in
[submission](submission.md). Its transaction is edited only through the
cancellation.
