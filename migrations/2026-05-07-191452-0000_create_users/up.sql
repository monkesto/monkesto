CREATE TABLE users (
    id BLOB PRIMARY KEY NOT NULL,
    webauthn_uuid BLOB NOT NULL,
    email TEXT NOT NULL,
    as_of BIGINT NOT NULL
)