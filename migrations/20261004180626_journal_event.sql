CREATE TABLE journal_events (
    event_id BIGSERIAL PRIMARY KEY,
    journal_id TEXT NOT NULL,
    stream_type int2 NOT NULL,
    stream_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    event_type int2 NOT NULL,
    payload BYTEA NOT NULL,
    user_id TEXT,
    authority BYTEA NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    display_timestamp TIMESTAMPTZ,
    CONSTRAINT uq_steam_ver UNIQUE (journal_id, stream_type, stream_id, version)
);

CREATE INDEX idx_event_stream ON journal_events (journal_id, stream_id, stream_type);

CREATE INDEX idx_joural_events_effective_tx ON journal_events (journal_id, COALESCE(display_timestamp, timestamp))