// @generated automatically by Diesel CLI.

diesel::table! {
    accounts (id) {
        id -> Binary,
        name -> Text,
        journal_id -> Binary,
        balance -> BigInt,
        deleted -> Bool,
        parent_account_id -> Nullable<Binary>,
    }
}

diesel::table! {
    entities (id) {
        id -> Binary,
        entity_type -> Integer,
    }
}

diesel::table! {
    events (event_id) {
        event_id -> Integer,
        sequence_id -> Integer,
        timestamp -> BigInt,
        authority -> Binary,
        entity_id -> Binary,
        payload -> Binary,
    }
}

diesel::table! {
    examples (id) {
        id -> Binary,
        deleted -> Bool,
    }
}

diesel::table! {
    journal_members_lookup (rowid) {
        rowid -> Integer,
        user_id -> Binary,
        journal_id -> Binary,
    }
}

diesel::table! {
    journals (id) {
        id -> Binary,
        name -> Text,
        owner -> Binary,
        members -> Binary,
        deleted -> Bool,
        parent_journal_id -> Nullable<Binary>,
    }
}

diesel::table! {
    passkeys (id) {
        id -> Binary,
        user_id -> Binary,
        passkey -> Binary,
        deleted -> Bool,
    }
}

diesel::table! {
    sessions (id) {
        id -> Binary,
        data -> Binary,
        expiry_date -> BigInt,
    }
}

diesel::table! {
    transactions (id) {
        id -> Binary,
        journal_id -> Binary,
        updates -> Binary,
        deleted -> Bool,
    }
}

diesel::table! {
    users (id) {
        id -> Binary,
        webauthn_uuid -> Binary,
        email -> Text,
        deleted -> Bool,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    accounts,
    entities,
    events,
    examples,
    journal_members_lookup,
    journals,
    passkeys,
    sessions,
    transactions,
    users,
);
