# Submission

A submission is how a [deposit](deposit.md) or [payment](payment.md) produces
and updates its [transaction](transaction-v2.md). The deposit or payment and
the transaction may be kept in separate event logs, so they cannot be written
together atomically. Submissions keep them in agreement anyway.

[Returns](return.md) and [cancellations](cancellation.md) are submitted the
same way. What is said here of a deposit or payment applies to them too.

```rust
struct Submission {
    /// The version of the deposit or payment being submitted.
    version: Version,
    /// The transaction version this submission expects to replace, or none if
    /// the transaction does not exist yet.
    expected_transaction_version: Option<Version>,
    /// The entries the transaction will have, with their IDs.
    entries: Vec<Entry>,
    outcome: Option<SubmissionOutcome>,
}

enum SubmissionOutcome {
    Applied { transaction_version: Version },
    Rejected,
}
```

## Links

The deposit or payment owns the link to its transaction. The transaction does
not refer to the deposit or payment.

The deposit or payment assigns its transaction's ID, and the ID of each entry it
produces, when it creates the things those entries come from. The IDs stay the
same across edits and resubmissions, so anything referring to an entry, such as
a reconciliation, keeps working.

## Lifecycle

1. Draft. The deposit or payment is built up and may be incomplete or
   inconsistent. It has no transaction yet.
2. Submit. When the deposit or payment meets its completion rules, a
   submission is recorded on the deposit or payment: its version, the
   transaction version it expects, and the entries. A submission can be made
   only if no earlier submission is still waiting for its outcome.
3. Apply. The submission is applied to the transaction only if the transaction
   is still at the expected version. Applying the same submission again has no
   further effect, so it can be retried safely.
4. Record the outcome on the deposit or payment: applied, with the transaction's
   new version, or rejected, because the transaction was at a different version.

The state of a deposit or payment follows from its latest submission:

- No submission: draft.
- Applied, and the deposit or payment has not changed since: submitted.
- Applied, and the deposit or payment has changed since: submitted, with
  changes not yet submitted. The transaction keeps the last submitted version
  until the deposit or payment is submitted again.
- No outcome yet: submitting. The deposit or payment cannot be submitted again
  until the outcome is recorded.
- Rejected: out of sync. The transaction changed in some other way.

A transaction produced by a deposit or payment is edited only through it, so a
rejection points to a real problem rather than routine use.

## Recovering a submission without an outcome

If the outcome of a submission is never recorded, the deposit or payment is
stuck. It can be resolved by looking at the transaction:

- Still at the expected version: the submission was not applied. Apply it.
- At the next version, with exactly the submitted entries: it was applied.
  Record it as applied.
- Anything else: record it as rejected.

This can be done automatically. It needs manual handling only if the transaction
cannot be read.

## Periods

A deposit or payment may be submitted while its transaction's period is open.
To change a deposit or payment whose transaction's period is closed, reopen the
period.
