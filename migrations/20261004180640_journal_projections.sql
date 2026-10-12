CREATE table journal_event_listener (
    id TEXT NOT NULL,
    last_processed_event_id BIGINT
);

CREATE TABLE journals (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE journal_members (
    user_id     TEXT   NOT NULL,
    journal_id  TEXT   NOT NULL,
    permissions INTEGER NOT NULL
);

CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    journal_id TEXT NOT NULL,
    name TEXT NOT NULL,
    account_type int2 NOT NULL,
    balance BIGINT NOT NULL
);

CREATE TABLE activities (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL,
    name TEXT NOT NULL,
    activity_kind int2 NOT NULL,
    balance BIGINT NOT NULL
);

CREATE TABLE funds (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL,
    name TEXT NOT NULL,
    balance BIGINT NOT NULL
);

CREATE TABLE transactions (
    id TEXT PRIMARY KEY,
    journal_id TEXT NOT NULL,
    entries BYTEA NOT NULL,
    memo TEXT
);

CREATE TABLE entries (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL,
    amount BIGINT NOT NULL,
    entry_side int2 NOT NULL,
    entry_kind BYTEA NOT NULL
);

CREATE TABLE files (
    id TEXT PRIMARY KEY,
    journal_id TEXT NOT NULL,
    hash BYTEA NOT NULL,
    name TEXT NOT NULL
);


