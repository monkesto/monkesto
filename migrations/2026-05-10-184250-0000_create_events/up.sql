CREATE TABLE events (
    event_id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    sequence_id INTEGER NOT NULL,
    timestamp BIGINT NOT NULL,
    authority BLOB NOT NULL,
    entity_id BLOB NOT NULL,
    payload BLOB NOT NULL,
    entity_type SMALLINT NOT NULL,
    applied_to_state BOOLEAN NOT NULL,
    UNIQUE (entity_id, sequence_id)
)