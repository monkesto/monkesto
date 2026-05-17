CREATE TABLE journals (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    owner BLOB NOT NULL,
    members BLOB NOT NULL,
    deleted BOOLEAN NOT NULL,
    parent_journal_id BLOB,
    as_of INTEGER NOT NULL
)