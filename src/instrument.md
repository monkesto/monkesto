# Instrument

An instrument is a unit of money moving into or out of an account: cash, a
check, a card, or an electronic payment. A [deposit](deposit.md) is
made up of the instruments it received, and a [payment](payment.md) is made with
the instrument it issued. Both use the same shape.

An instrument has no direction field. One on a deposit was received; one on a
payment was issued.

```rust
struct Instrument {
    id: InstrumentId,
    kind: InstrumentKind,
    amount: Amount,
}

enum InstrumentKind {
    Cash,
    Check { number: Option<String> },
    Card { reference: Option<String> },
    Electronic { reference: Option<String> },
}
```

## Kinds

The kinds are defined by Monkesto, because the system depends on them:

- Cash: one pooled instrument per deposit, claimed in parts by its sources.
- Check: has a number. A received check may be returned; an issued check may go
  stale, be stopped, or be voided.
- Card: may be charged back later.
- Electronic: ACH, wire, and other electronic payments. It may be reversed
  later.

## Notes

Each check is its own instrument. Cash is a single instrument per deposit, for
the amount counted and taken to the bank; the sources in the deposit claim it in
parts.

An instrument belongs to the deposit or payment that holds it. It has a stable
ID so that later events can refer to it.

An instrument has no stored status. What has happened to it, such as being
returned, stopped, or voided, is known from the events that refer to it. Which
events apply depends on whether it was received or issued.

## Corrections and later events

A correction fixes a record that was wrong, such as a mistyped check amount. It
is made by editing the deposit or payment while its period is open, reopening
the period if necessary.

A later event is something new that happened after a correct record. A
received instrument that went back, such as a returned check, is a
[return](return.md). An issued instrument whose money did not end up leaving,
such as a stopped check, is a [cancellation](cancellation.md). Each produces
its own transaction, dated when it happened.
