#![allow(clippy::module_inception)]

pub mod name {
    pub mod name {
        include!(concat!(env!("OUT_DIR"), "/proto.name.name.rs"));
    }

    pub mod name_error {
        include!(concat!(env!("OUT_DIR"), "/proto.name.name_error.rs"));
    }
}

pub mod error {
    pub mod monkesto_error {
        include!(concat!(env!("OUT_DIR"), "/proto.error.monkesto_error.rs"));
    }

    pub mod decode_error {
        include!(concat!(env!("OUT_DIR"), "/proto.error.decode_error.rs"));
    }
}

pub mod journal {
    pub mod error {
        pub mod journal_error {
            include!(concat!(
                env!("OUT_DIR"),
                "/proto.journal.error.journal_error.rs"
            ));
        }
    }
    pub mod event {
        pub mod journal_event {
            include!(concat!(env!("OUT_DIR"), "/proto.journal.event.journal.rs"));
        }
    }

    pub mod entry {
        pub mod entry {
            include!(concat!(env!("OUT_DIR"), "/proto.journal.entry.entry.rs"));
        }
    }

    pub mod transaction {
        pub mod memo {
            pub mod memo {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/proto.journal.transaction.memo.memo.rs"
                ));
            }

            pub mod memo_error {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/proto.journal.transaction.memo.memo_error.rs"
                ));
            }
        }
    }
}

pub mod authn {
    pub mod event {
        pub mod authn {
            include!(concat!(env!("OUT_DIR"), "/proto.authn.event.authn.rs"));
        }
    }

    pub mod error {
        pub mod user_error {
            include!(concat!(env!("OUT_DIR"), "/proto.authn.error.user_error.rs"));
        }
    }

    pub mod passkey {
        pub mod error {
            pub mod passkey_error {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/proto.authn.passkey.error.passkey_error.rs"
                ));
            }
        }
    }
}

pub mod id {
    pub mod ident {
        include!(concat!(env!("OUT_DIR"), "/proto.id.ident.rs"));
    }

    pub mod ident_error {
        include!(concat!(env!("OUT_DIR"), "/proto.id.ident_error.rs"));
    }
}

pub mod authz {
    pub mod event {
        pub mod authz {
            include!(concat!(env!("OUT_DIR"), "/proto.authz.event.authz.rs"));
        }
    }
}

pub mod time {
    pub mod timestamp {
        include!(concat!(env!("OUT_DIR"), "/proto.time.timestamp.rs"));
    }
}

pub mod authority {
    pub mod authority {
        include!(concat!(env!("OUT_DIR"), "/proto.authority.authority.rs"));
    }
}
