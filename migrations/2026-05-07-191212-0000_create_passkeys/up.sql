CREATE TABLE passkeys (
    id BLOB PRIMARY KEY NOT NULL,
    user_id BLOB NOT NULL,
    passkey BLOB NOT NULL,
    deleted BOOLEAN NOT NULL,
    as_of INTEGER NOT NULL
)