CREATE OR REPLACE FUNCTION apply_journal_projection_batch(p_journal_id TEXT, p_events JSONB)
RETURNS BIGINT
LANGUAGE plpgsql AS $$
DECLARE
    e RECORD;
    balance_delta BIGINT;
    last_event_id BIGINT;
BEGIN
    FOR e IN
        SELECT *
        FROM jsonb_to_recordset(p_events) AS event (
            event_id BIGINT,
            kind TEXT,
            id TEXT,
            owner_id TEXT,
            user_id TEXT,
            name TEXT,
            permissions INTEGER,
            account_type SMALLINT,
            transaction_id TEXT,
            financial_period SMALLINT,
            memo TEXT,
            hash TEXT,
            amount BIGINT,
            entry_side SMALLINT,
            entry_kind TEXT,
            account_id TEXT,
            fund_id TEXT,
            activity_id TEXT,
            activity_kind SMALLINT,
            donor_id TEXT,
            donation_id TEXT,
            instrument SMALLINT,
            check_num TEXT
        )
        ORDER BY event_id
    LOOP
        last_event_id := e.event_id;
        CASE e.kind
            WHEN 'JournalCreated' THEN
                INSERT INTO journals (id, owner_id, name)
                VALUES (p_journal_id, e.owner_id, e.name)
                ON CONFLICT DO NOTHING;
            WHEN 'JournalDeleted' THEN
                DELETE FROM journals WHERE id = p_journal_id;
            WHEN 'MemberAdded' THEN
                INSERT INTO journal_members (user_id, journal_id, permissions)
                VALUES (e.user_id, p_journal_id, e.permissions)
                ON CONFLICT DO NOTHING;
            WHEN 'MemberPermissionsUpdated' THEN
                UPDATE journal_members SET permissions = e.permissions
                WHERE journal_id = p_journal_id AND user_id = e.user_id;
            WHEN 'MemberRemoved' THEN
                DELETE FROM journal_members WHERE journal_id = p_journal_id AND user_id = e.user_id;
            WHEN 'AccountCreated' THEN
                INSERT INTO accounts (id, journal_id, name, balance, account_type)
                VALUES (e.id, p_journal_id, e.name, 0, e.account_type)
                ON CONFLICT DO NOTHING;
            WHEN 'AccountRenamed' THEN
                UPDATE accounts SET name = e.name WHERE journal_id = p_journal_id AND id = e.id;
            WHEN 'AccountDeleted' THEN
                DELETE FROM accounts WHERE journal_id = p_journal_id AND id = e.id;
            WHEN 'TransactionCreated' THEN
                INSERT INTO transactions (id, journal_id, financial_period, memo)
                VALUES (e.id, p_journal_id, e.financial_period, e.memo)
                ON CONFLICT DO NOTHING;
            WHEN 'FileUploaded' THEN
                INSERT INTO files (id, journal_id, hash, name)
                VALUES (e.id, p_journal_id, decode(e.hash, 'hex'), e.name)
                ON CONFLICT DO NOTHING;
            WHEN 'FileDeleted' THEN
                DELETE FROM files WHERE journal_id = p_journal_id AND id = e.id;
            WHEN 'FundCreated' THEN
                INSERT INTO funds (id, journal_id, name, balance)
                VALUES (e.id, p_journal_id, e.name, 0)
                ON CONFLICT DO NOTHING;
            WHEN 'EntryCreated' THEN
                INSERT INTO entries (id, journal_id, transaction_id, amount, entry_side, entry_kind)
                VALUES (e.id, p_journal_id, e.transaction_id, e.amount, e.entry_side,
                        decode(e.entry_kind, 'hex'))
                ON CONFLICT DO NOTHING;
                CONTINUE WHEN NOT FOUND;

                balance_delta := CASE WHEN e.entry_side = 1 THEN -e.amount ELSE e.amount END;
                IF e.account_id IS NOT NULL THEN
                    UPDATE accounts SET balance = balance + balance_delta
                    WHERE journal_id = p_journal_id AND id = e.account_id;
                ELSE
                    UPDATE funds SET balance = balance - balance_delta
                    WHERE journal_id = p_journal_id AND id = e.fund_id;
                    IF NOT FOUND THEN
                        RAISE EXCEPTION 'journal % event %: missing fund %', p_journal_id, e.event_id, e.fund_id;
                    END IF;
                    UPDATE activities SET balance = balance + balance_delta
                    WHERE journal_id = p_journal_id AND id = e.activity_id;
                END IF;
                IF NOT FOUND THEN
                    RAISE EXCEPTION 'journal % event %: missing balance target %',
                        p_journal_id, e.event_id, COALESCE(e.account_id, e.activity_id);
                END IF;
            WHEN 'ActivityCreated' THEN
                INSERT INTO activities (id, journal_id, name, balance, activity_kind)
                VALUES (e.id, p_journal_id, e.name, 0, e.activity_kind)
                ON CONFLICT DO NOTHING;
            WHEN 'DonorCreated' THEN
                INSERT INTO donors (id, journal_id, name, total_donated)
                VALUES (e.id, p_journal_id, e.name, 0)
                ON CONFLICT DO NOTHING;
            WHEN 'DonationEntryCreated' THEN
                INSERT INTO donation_entries
                    (id, journal_id, donor_id, fund_id, donation_id, amount, instrument, check_num)
                VALUES (e.id, p_journal_id, e.donor_id, e.fund_id, e.donation_id, e.amount,
                        e.instrument, e.check_num)
                ON CONFLICT DO NOTHING;
                CONTINUE WHEN NOT FOUND;

                UPDATE donors SET total_donated = total_donated + e.amount
                WHERE journal_id = p_journal_id AND id = e.donor_id;
                IF NOT FOUND THEN
                    RAISE EXCEPTION 'journal % event %: missing donor %', p_journal_id, e.event_id, e.donor_id;
                END IF;
                UPDATE funds SET balance = balance + e.amount
                WHERE journal_id = p_journal_id AND id = e.fund_id;
                IF NOT FOUND THEN
                    RAISE EXCEPTION 'journal % event %: missing fund %', p_journal_id, e.event_id, e.fund_id;
                END IF;
            WHEN 'DonationCreated' THEN
                INSERT INTO donations (id, journal_id, deposit_account_id, donation_total)
                VALUES (e.id, p_journal_id, e.account_id, e.amount)
                ON CONFLICT DO NOTHING;
                CONTINUE WHEN NOT FOUND;

                UPDATE accounts SET balance = balance + e.amount
                WHERE journal_id = p_journal_id AND id = e.account_id;
                IF NOT FOUND THEN
                    RAISE EXCEPTION 'journal % event %: missing account %', p_journal_id, e.event_id, e.account_id;
                END IF;
            ELSE
                RAISE EXCEPTION 'journal % event %: unknown projection kind %',
                    p_journal_id, e.event_id, e.kind;
        END CASE;
    END LOOP;

    RETURN last_event_id;
END;
$$;
