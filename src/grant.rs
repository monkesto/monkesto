use crate::authority::Authority;
use crate::event::EventStore;
use crate::id;
use crate::ident::Ident;
use crate::ident::IdentError;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Display;
use std::ops::Deref;
use std::str::FromStr;

id!(GrantId, Ident::new16());

pub enum GrantEvent {
    Created,
    Revoked,
}

pub trait GrantStore:
    Clone + Sync + Send + EventStore<Id = GrantId, Payload = GrantEvent, Error = ()>
{
}

pub struct GrantService<G: GrantStore> {
    grant_store: G,
}

impl<G: GrantStore> GrantService<G> {
    #[expect(dead_code)]
    pub fn new(grant_store: G) -> Self {
        Self { grant_store }
    }

    #[expect(dead_code)]
    pub async fn create(&self, authority: Authority) -> Result<GrantId, ()> {
        let grant_id = GrantId::new();
        self.grant_store
            .record(grant_id, authority, GrantEvent::Created)
            .await?;
        Ok(grant_id)
    }

    #[expect(dead_code)]
    pub async fn revoke(&self, grant_id: GrantId, authority: Authority) -> Result<(), ()> {
        self.grant_store
            .record(grant_id, authority, GrantEvent::Revoked)
            .await?;
        Ok(())
    }
}
