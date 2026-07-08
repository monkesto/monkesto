# Account

An account is a named association for entries that modify an accumulated balance. Note that entries are not directly part of an account, but rather are associated with transactions.

Each account is associated with a particular journal. The balance of an account in a journal or set of journals is the sum of all of the entries associated with that account in the specified journal or set of journals.

The balance of each account starts at zero, and changes as transaction entries are added that include the account.

## Subaccount

A subaccount is an account that has a parent relationship to another account. This parent relationship means that, in some contexts, things that are part of a subaccount are treated as being part of the parent account as well. Similarly, in some contexts, the balances of a subaccount are treated as being part of the parent account's balance.

Because a subaccount is in some ways also part of the parent account, some views that display data for an account may need to have a way to determine whether the user wants to view things that are only for that account, or whether it should include information from some or all subaccounts as well.

A subaccount may be included in parent summaries, but it remains a distinct account that has its own accumulated balance that is separate from the parent account's balance.
