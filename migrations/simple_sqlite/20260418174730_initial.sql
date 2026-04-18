CREATE TABLE IF NOT EXISTS entities (
    id BLOB PRIMARY KEY,
    entity_type INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS event_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id BLOB NOT NULL REFERENCES entities(id),
    payload BLOB NOT NULL,
    sequence_num INTEGER NOT NULL,
    authority BLOB NOT NULL,
    timestamp INTEGER NOT NULL,
    UNIQUE (entity_id, sequence_num)
);

CREATE TABLE IF NOT EXISTS projections (
    entity_id BLOB NOT NULL REFERENCES entities(id),
    projection BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS user_lookup (
    entity_id BLOB NOT NULL REFERENCES entities(id),
    email TEXT UNIQUE NOT NULL
);

CREATE TABLE IF NOT EXISTS account_balance (
    entity_id BLOB NOT NULL REFERENCES entities(id),
    balance INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS account_lookup (
    account_id BLOB NOT NULL REFERENCES entities(id),
    journal_id BLOB NOT NULL REFERENCES entities(id)
);


