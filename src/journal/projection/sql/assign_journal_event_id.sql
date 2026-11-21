CREATE OR REPLACE FUNCTION assign_journal_event_id() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.event_id IS NOT NULL THEN
        RAISE EXCEPTION 'journal event IDs must be assigned by the append trigger';
    END IF;
    -- Keep this lock until commit. The projection uses a different lock so it can
    -- process committed events while an append is in progress.
    PERFORM pg_advisory_xact_lock(185745011, hashtext(NEW.journal_id));
    NEW.event_id := nextval('journal_events_event_id_seq');
    RETURN NEW;
END;
$$;

CREATE OR REPLACE TRIGGER assign_journal_event_id
BEFORE INSERT ON journal_events
FOR EACH ROW EXECUTE FUNCTION assign_journal_event_id();
