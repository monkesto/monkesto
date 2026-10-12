CREATE TABLE journal_stream (
    journal_id TEXT PRIMARY KEY,
    version BIGINT NOT NULL,
    valid bool NOT NULL
);

CREATE TABLE account_stream (
    journal_id TEXT NOT NULL REFERENCES journal_stream(journal_id),
    account_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    valid bool NOT NULL,
    PRIMARY KEY (journal_id, account_id)
);

CREATE TABLE activity_stream (
    journal_id TEXT NOT NULL REFERENCES journal_stream(journal_id),
    activity_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    valid bool NOT NULL,
    PRIMARY KEY (journal_id, activity_id)
);

CREATE TABLE fund_stream (
    journal_id TEXT NOT NULL REFERENCES journal_stream(journal_id),
    fund_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    valid bool NOT NULL,
    PRIMARY KEY (journal_id, fund_id)
);

CREATE TABLE transaction_stream (
    journal_id TEXT NOT NULL REFERENCES journal_stream(journal_id),
    transaction_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    valid bool NOT NULL,
    PRIMARY KEY (journal_id, transaction_id)
);

CREATE TABLE entry_stream (
    journal_id TEXT NOT NULL REFERENCES journal_stream(journal_id),
    entry_id TEXT NOT NULL,
    transaction_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    valid bool NOT NULL,
    PRIMARY KEY (journal_id, entry_id)
);

CREATE TABLE file_stream (
    journal_id TEXT NOT NULL REFERENCES journal_stream(journal_id),
    file_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    valid bool NOT NULL,
    PRIMARY KEY (journal_id, file_id)
);
