CREATE TABLE journals (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    owner BLOB NOT NULL,
    parent_journal_id BLOB,
    as_of BIGINT NOT NULL
)