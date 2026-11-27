# Payment

A payment is money paid out of an account to a [party](party.md). It is
separate from a [deposit](deposit.md). It can be saved while it is incomplete
or inconsistent, and it produces a [transaction](transaction-v2.md) when it is
[submitted](submission.md).

```rust
struct Payment {
    id: PaymentId,
    date: Date,
    account: AccountId,
    payee: PartyId,
    instrument: Instrument,
    lines: Vec<Line>,
    transaction: TransactionId,
    account_entry: EntryId,
    submission: Option<Submission>,
}
```

## Shape

A payment has one payee, one [instrument](instrument.md), and one or more
[lines](line.md). The instrument's amount is the amount paid. A batch of checks
is several payments.

A line is usually an expense, and may be income or negative when there is
occasion for it, such as a credit from the payee netted against the payment.
The amount paid is the lines, expense less income.

## Completion

A payment can be submitted when its instrument is known and its lines are
complete. The transaction's entries are generated from it: the account entry
from the amount paid, and one activity entry per line. The payment holds the ID
of the account entry, and each line holds the ID of its entry; the transaction
does not refer to the payment.

## Editing

A payment is submitted, edited, and resubmitted as described in
[submission](submission.md). Its transaction is edited only through the
payment.

## Corrections and later events

A voided check may be either, depending on what happened. A check voided before
it was sent, or written wrong, is a correction: the payment is edited or
removed, reopening its period if necessary. A check that was sent and later
went stale, was lost, or was stopped is a later event, recorded as a
[cancellation](cancellation.md).
