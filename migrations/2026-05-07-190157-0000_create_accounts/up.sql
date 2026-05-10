CREATE TABLE accounts (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    journal_id BLOB NOT NULL,
    balance BIGINT NOT NULL,
    deleted BOOLEAN NOT NULL,
    parent_account_id BLOB
)