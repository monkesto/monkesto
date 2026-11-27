# Deposit

A deposit is money brought into a bank account together, along with the record
of where that money came from.

A deposit can be saved while it is incomplete or inconsistent, and it produces
a [transaction](transaction-v2.md) when it is [submitted](submission.md).

```rust
struct Deposit {
    id: DepositId,
    date: Date,
    account: AccountId,
    instruments: Vec<Instrument>,
    sources: Vec<Source>,
    transaction: TransactionId,
    bank_entry: EntryId,
    submission: Option<Submission>,
}

/// One origin of money in the deposit, such as a donor's envelope, loose
/// offering cash, or a rent payment.
struct Source {
    id: SourceId,
    from: PartyId,
    lines: Vec<Line>,
    claims: Vec<Claim>,
}

/// A claim may be negative, giving back part of an instrument.
struct Claim {
    instrument: InstrumentId,
    amount: Amount,
}
```

## Two sides

A deposit has two sides that must agree:

- [Instruments](instrument.md): what went to the bank: cash, checks, cards,
  and electronic payments.
- Sources: where the money came from. Each source has lines, each with an
  amount, kind, fund, and activity, and claims on the instruments that paid it.

Each check is its own instrument, even when one source is paid by several
checks. Cash is one instrument per deposit: the amount counted and taken to the
bank. Each source's cash is a claim on that instrument, not an instrument of its
own.

A source's [lines](line.md) may be income or expense, and occasionally
negative, such as reducing loose offering to match the cash counted. A source
whose lines net to an expense, or to negative income, has negative claims: it
gives back part of an instrument rather than claiming it.

When the counted cash differs from what the sources say, the difference is just
another source. For example, if the envelopes say more cash than was counted,
the donors are still credited with what their envelopes say, and a source with
an expense line on a cash over/short activity covers the shortfall. Which
activity the difference goes to is the treasurer's choice.

## Donations

A donation is a common pattern rather than a codified feature. It is a source
whose lines are on activities the church treats as giving. The deposit does not
distinguish gifts from other income; completion and the generated entries are
the same either way.

Which activities count as giving is decided by the reports that need it, such as
donor statements and giving reports. Those reports should share one definition
so that they agree.

One source may mix gifts and other income. A check that pays an event fee and
includes a gift is one source with two lines, and only the gift line counts as
giving.

Every source has a "from". When nobody in particular gave it, such as loose
offering cash, it is from the built-in anonymous [party](party.md). Such a
source on giving activities is an anonymous gift.

## Payment processors

A payment processor, such as a card or online giving service, is an asset
account like a bank account. Gifts are deposited into the processor's account
at their full amount. Its fees are expenses from that account, and its payouts
are transfers from it to the bank. Fees are not part of a deposit or its
instruments.

## Completion

A deposit can be submitted only when:

- Every instrument is fully claimed by sources, no more and no less. Claims
  are summed with their signs.
- Every source's claims equal its lines, income less expense.

The transaction's entries are generated from the deposit: the bank entry from
the instruments and activity entries from the source lines. They balance by
construction.

The deposit generates one bank entry, because the bank sees one deposit total,
and one activity entry per source line. The deposit holds the ID of the bank
entry, and each source line holds the ID of its entry, so every entry can be
traced to where the money came from. The transaction does not refer to the
deposit. A transaction view may group activity entries by fund for display.

Because sources claim instruments rather than copying their details, the same
check cannot be claimed twice. If each donation instead copied the check
number, and donation detail were not reconciled with the transaction, a
duplicated donation for one check could go unnoticed: the deposit transaction
would stay correct while donor statements were overstated.

## Editing

A deposit is submitted, edited, and resubmitted as described in
[submission](submission.md). Its transaction is edited only through the
deposit.

A returned check is not a correction. It is a later event, recorded as a
[return](return.md).
