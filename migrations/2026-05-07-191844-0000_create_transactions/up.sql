CREATE TABLE transactions (
    id BLOB PRIMARY KEY NOT NULL,
    journal_id BLOB NOT NULL,
    updates BLOB NOT NULL,
    as_of BIGINT NOT NULL
)