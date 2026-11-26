CREATE TABLE donor_stream (
    journal_id TEXT NOT NULL REFERENCES journal_stream(journal_id),
    donor_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    valid bool NOT NULL,
    PRIMARY KEY (journal_id, donor_id)
);

CREATE TABLE donors (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    name TEXT NOT NULL,
    total_donated BIGINT NOT NULL,
    PRIMARY KEY (journal_id, id)
);

CREATE TABLE donation_stream (
    journal_id TEXT NOT NULL REFERENCES journal_stream(journal_id),
    donation_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    valid bool NOT NULL,
    PRIMARY KEY (journal_id, donation_id)
);

CREATE TABLE donations (
    id TEXT NOT NULL,
    journal_id TEXT NOT NULL REFERENCES journals(id),
    deposit_account_id TEXT NOT NULL,
    donation_total BIGINT NOT NULL,
    entries BYTEA NOT NULL,
    PRIMARY KEY (journal_id, id)
)


