CREATE TABLE jewel_imports (
    journal_id TEXT NOT NULL,
    file_id TEXT NOT NULL,
    first_event_id BIGINT NOT NULL DEFAULT 0,
    last_event_id BIGINT NOT NULL DEFAULT 0,
    total_events BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (journal_id, file_id),
    FOREIGN KEY (journal_id, file_id) REFERENCES file_stream(journal_id, file_id)
);
