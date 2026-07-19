-- sqlx expects all tables to be in the public schema
-- these tables are stubs: they will be re-created in
-- their respective schemas when the program is run

CREATE TABLE IF NOT EXISTS users (
     id TEXT PRIMARY KEY,
     email TEXT NOT NULL,
     webauthn_uuid UUID NOT NULL
);

CREATE TABLE IF NOT EXISTS passkeys (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    passkey TEXT NOT NULL,
    credential_id BYTEA NOT NULL
);

CREATE TABLE IF NOT EXISTS authz_role (
    id TEXT PRIMARY KEY,
    name BYTEA NOT NULL,
    latest_event_id BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS authz_role_actor (
    role_id TEXT NOT NULL,
    actor BYTEA NOT NULL,
    PRIMARY KEY (role_id, actor)
);

CREATE TABLE IF NOT EXISTS journals (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS journal_members (
    user_id     TEXT   NOT NULL,
    journal_id  TEXT   NOT NULL,
    permissions INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    journal_id TEXT NOT NULL,
    name TEXT NOT NULL,
    balance BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS transactions (
    id TEXT PRIMARY KEY,
    journal_id TEXT NOT NULL,
    updates BYTEA NOT NULL
);

-- stub that includes indexes for the journal store
CREATE TABLE IF NOT EXISTS event (
    event_id BIGINT,
    event_type VARCHAR(255),
    payload BYTEA,
    inserted_at TIMESTAMP,
    journal_id TEXT,
    user_id TEXT,
    account_id TEXT,
    transaction_id TEXT
);