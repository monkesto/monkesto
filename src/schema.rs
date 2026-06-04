// @generated automatically by Diesel CLI.

diesel::table! {
    accounts (id) {
        id -> Binary,
        name -> Text,
        journal_id -> Binary,
        balance -> BigInt,
        parent_account_id -> Nullable<Binary>,
        as_of -> BigInt,
    }
}

diesel::table! {
    entities (id) {
        id -> Binary,
        entity_type -> SmallInt,
    }
}

diesel::table! {
    events (event_id) {
        event_id -> BigInt,
        timestamp -> BigInt,
        authority -> Binary,
        entity_id -> Binary,
        payload -> Binary,
        applied_to_state -> Bool,
    }
}

diesel::table! {
    examples (id) {
        id -> Binary,
        counter -> BigInt,
        as_of -> BigInt,
    }
}

diesel::table! {
    journal_members (rowid) {
        rowid -> Integer,
        user_id -> Binary,
        journal_id -> Binary,
        permissions -> Integer,
    }
}

diesel::table! {
    journals (id) {
        id -> Binary,
        name -> Text,
        owner -> Binary,
        parent_journal_id -> Nullable<Binary>,
        as_of -> BigInt,
    }
}

diesel::table! {
    passkeys (id) {
        id -> Binary,
        user_id -> Binary,
        passkey -> Binary,
        as_of -> BigInt,
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
        as_of -> BigInt,
    }
}

diesel::table! {
    users (id) {
        id -> Binary,
        webauthn_uuid -> Binary,
        email -> Text,
        as_of -> BigInt,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    accounts,
    entities,
    events,
    examples,
    journal_members,
    journals,
    passkeys,
    sessions,
    transactions,
    users,
);
