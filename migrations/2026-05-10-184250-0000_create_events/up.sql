CREATE TABLE events (
    event_id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    timestamp BIGINT NOT NULL,
    authority BLOB NOT NULL,
    entity_id BLOB NOT NULL,
    payload BLOB NOT NULL,
    applied_to_state BOOLEAN NOT NULL
)