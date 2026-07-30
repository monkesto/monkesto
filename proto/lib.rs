pub mod balance_update {
    include!(concat!(env!("OUT_DIR"), "/proto.balance_update.rs"));
}

pub mod error {
    include!(concat!(env!("OUT_DIR"), "/proto.error.rs"));
}

pub mod ident {
    include!(concat!(env!("OUT_DIR"), "/proto.ident.rs"));
}

pub mod event {
    pub mod authn {
        include!(concat!(env!("OUT_DIR"), "/proto.event.authn.rs"));
    }

    pub mod authority {
        include!(concat!(env!("OUT_DIR"), "/proto.event.authority.rs"));
    }

    pub mod authz {
        include!(concat!(env!("OUT_DIR"), "/proto.event.authz.rs"));
    }

    pub mod journal {
        include!(concat!(env!("OUT_DIR"), "/proto.event.journal.rs"));
    }

    pub mod timestamp {
        include!(concat!(env!("OUT_DIR"), "/proto.event.timestamp.rs"));
    }
}
