# Journal

A journal is a self-balancing set of books. How exactly that manifests is controlled by rules about how accounts and transactions work, but this is the guiding principle behind the rules that are enforced when authoring transactions.

Transactions may involve entries in multiple journals, but each affected journal must still balance independently.

## Subjournal

A subjournal is a journal, so it is a balanced set of books. It has a parent relationship to another journal. This parent relationship means that, in some contexts, things that are part of a subjournal are treated as being part of the parent journal as well.

Because a subjournal is in some ways also part of the parent journal, some views that display data for a journal may need to have a way to determine whether the user wants to view things that are only for the current journal, or whether it should include information from some or all subjournals as well.

A subjournal may be included in parent summaries, but it remains a distinct journal that must still balance independently and can be viewed independently as well.
