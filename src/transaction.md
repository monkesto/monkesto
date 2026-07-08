# Transaction

A transaction is a balanced set of entries that are applied to journals and accumulated to accounts as a single atomic unit. A transaction's entries are authored, accepted, and applied as one unit. A subset of its entries is not the whole transaction.

A transaction may have an arbitrary number of entries, provided that the entries, together, follow the transactional consistency rules. Each entry consists of several pieces of data:

- Journal
- Account
- Entry Type (Debit/Credit)
- Amount
- Currency

## Currency

Currency is not currently a recorded field, so all entries are assumed to be in USD minor units (cents). Future versions may support multiple currencies, and if they did they would be recorded in the associated minor unit as standardized in ISO 4217:2015.

When currencies are introduced, the amendment version should be recorded in the same store and event log as the transactions, so that we can determine which transactions are before and after various currency minor unit changes.

## Balanced

A transaction is consistent if the amounts are balanced across all balanced dimensions. That is, if the total debit amounts equal the total credit amounts.

Every pair of journal and currency must have credit amounts that exactly equal the debit amounts for the same pair of journal and currency entries.

## Balances

An account balance is determined by summing the debit and credit amounts for all entries in all transactions for a given account. The account balance must always be in relation to some set of journals. It could be a balance for just one journal, or it could be a balance for a journal and all its subjournals, recursively. Less usually, it could even be a balance for a disjoint set of subjournals.

When calculating the balances for an account, it can also be desirable to report the balance for an account as well as the balance for all its subaccounts for a given set of journals, in order to provide a summary of the account balance that includes subaccounts, recursively.

An account with no associated entries in any transaction has a balance of 0.

## Account visibility

The account that is associated with a journal in an entry must be in the same journal, or in a parent journal of the associated journal. That is, accounts that are for a parent journal are visible to entries in subjournals of the parent journal automatically, even though a manager of accounts on a subjournal is not allowed to remove those accounts because they are not a manager of the parent journal.

## Entry visibility

Because a single transaction may involve entries in multiple journals, and viewers may not have permission to view all involved journals, in some cases a transaction may only be partially visible to the viewer. This partial view should still be balanced for the visible set of journals, but it should be made clear to the user that they are only viewing part of a bigger transaction, and editing of a partial transaction should never be permitted.

## Consistency

Because there can be a delay between when the accounts and journals are verified as still open and active and when a transaction is created, it is possible for a transaction to be created with entries that reference journals or accounts that are no longer open or active. These are still considered valid transactions.

The practical effect is that an account or journal is not immediately and fully closed or deactivated until it has been distributed fully to any readers. During the eventual consistency delay, some workers may still accept transactions that other, more up-to-date workers would reject.

For most interactions, this eventual consistency delay is likely to be imperceptible. However, when writing code that interacts with transactions and account balances, it is important to include transactions for an account that may show up after the account or journal has been closed or deactivated.
