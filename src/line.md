# Line

A line is an amount assigned to a fund and activity on a
[deposit](deposit.md) source, a [payment](payment.md), a [return](return.md),
or a [cancellation](cancellation.md). They all use the same line.

```rust
struct Line {
    id: LineId,
    entry: EntryId,
    amount: Amount,
    kind: ActivityKind,
    fund: Fund,
    activity: ActivityId,
}
```

## Notes

A line is either income or expense, like an activity entry. Its amount is
usually positive, and may be negative when there is occasion for it.

When its deposit or payment is [submitted](submission.md), each line produces
one activity entry in the [transaction](transaction-v2.md). The line holds that
entry's ID, which stays the same across edits and resubmissions.
