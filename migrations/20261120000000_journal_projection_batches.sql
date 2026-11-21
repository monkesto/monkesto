-- Cut over with the old projection and writers stopped.
ALTER TABLE journal_events ALTER COLUMN event_id DROP DEFAULT;
ALTER SEQUENCE journal_events_event_id_seq CACHE 1;

CREATE INDEX idx_journal_events_projection ON journal_events (journal_id, event_id);

CREATE TABLE journal_projection_checkpoint (
    journal_id TEXT PRIMARY KEY,
    last_processed_event_id BIGINT NOT NULL DEFAULT 0,
    retry_after TIMESTAMPTZ NOT NULL DEFAULT '-infinity'
);

INSERT INTO journal_projection_checkpoint (journal_id, last_processed_event_id)
SELECT e.journal_id, COALESCE(MAX(e.event_id) FILTER (WHERE e.event_id <= legacy.event_id), 0)
FROM journal_events AS e
CROSS JOIN (
    SELECT last_processed_event_id AS event_id
    FROM journal_event_listener
    WHERE id = 'Journal Projection'
) AS legacy
GROUP BY e.journal_id;
