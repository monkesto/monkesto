CREATE OR REPLACE FUNCTION signal_journal_events() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO journal_projection_checkpoint (journal_id)
    SELECT DISTINCT journal_id FROM appended_events
    ON CONFLICT (journal_id) DO NOTHING;

    IF EXISTS (SELECT 1 FROM appended_events) THEN
        PERFORM pg_notify('journal_events', '');
    END IF;
    RETURN NULL;
END;
$$;

CREATE OR REPLACE TRIGGER signal_journal_events
AFTER INSERT ON journal_events
REFERENCING NEW TABLE AS appended_events
FOR EACH STATEMENT EXECUTE FUNCTION signal_journal_events();
