# Party

A party is a person or organization the church deals with. The same party may
give a donation, be written a check, be reimbursed, or receive a refund. There
is one shared list of parties, not separate lists of donors and payees.

```rust
struct Party {
    id: PartyId,
    name: Name,
}
```

## Notes

A party is referenced by [deposits](deposit.md) and
[payments](payment.md).

Every journal has a built-in anonymous party, used when money comes from
nobody in particular, such as loose offering cash. Like the general fund, it is
a party value rather than the absence of one. Anything that contacts or
reports on parties individually, such as donor statements, should recognize it.

Being a donor or a payee is not a kind of party. It is what a party did on a
particular deposit or payment.
