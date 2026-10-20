CREATE table journal_event_listener (
    id TEXT NOT NULL,
    last_processed_event_id BIGINT
);

INSERT INTO journal_event_listener (id, last_processed_event_id) VALUES ('Journal Projection', -1);

CREATE TABLE journals (
    id TEXT NOT NULL PRIMARY KEY,
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE journal_members (
    user_id     TEXT   NOT NULL,
    journal_id  TEXT   NOT NULL REFERENCES journals(id),
    permissions INTEGER NOT NULL,
    PRIMARY KEY (journal_id, user_id)
);

CREATE TABLE accounts (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    name TEXT NOT NULL,
    account_type int2 NOT NULL,
    balance BIGINT NOT NULL,
    PRIMARY KEY (journal_id, id)
);

CREATE TABLE activities (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    name TEXT NOT NULL,
    activity_kind int2 NOT NULL,
    balance BIGINT NOT NULL,
    PRIMARY KEY (journal_id, id)
);


CREATE TABLE funds (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    name TEXT NOT NULL,
    balance BIGINT NOT NULL,
    PRIMARY KEY (journal_id, id)
);


CREATE TABLE transactions (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    memo TEXT,
    PRIMARY KEY (journal_id, id)
);

CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS btree_gin;
CREATE INDEX idx_tx_memo_trgm ON transactions USING GIN (journal_id, memo gin_trgm_ops);

CREATE TABLE entries (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    transaction_id TEXT NOT NULL,
    amount BIGINT NOT NULL,
    entry_side int2 NOT NULL,
    entry_kind BYTEA NOT NULL,
    PRIMARY KEY (journal_id, transaction_id, id)
);


CREATE TABLE files (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    hash BYTEA NOT NULL,
    name TEXT NOT NULL,
    PRIMARY KEY (journal_id, id)
);



