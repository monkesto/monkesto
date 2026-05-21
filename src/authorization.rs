#![cfg_attr(not(test), expect(dead_code))]

use crate::authority::Authority;
use crate::grant::GrantId;
use crate::grant::GrantPayload;
use crate::grant::GrantStream;
use crate::role::RoleId;
use crate::role::RolePayload;
use crate::role::RoleStream;
use crate::store::After;
use crate::store::Event;
use crate::store::EventId;
use crate::store::Observe;
use crate::store::Store;
use crate::store::When;
use crate::store::memory::memory_store;
use chrono::Utc;
use std::error::Error as StdError;
use thiserror::Error;

#[derive(Clone, Debug)]
pub enum AuthorizationEvent {
    Role(Event<Authority, RoleId, RolePayload>),
    Grant(Event<Authority, GrantId, GrantPayload>),
}

memory_store! {
    type AuthorizationMemoryStore = MemoryStore<Authority, RoleStream, GrantStream>
    where AuthorizationEvent {
        RoleStream => Role,
        GrantStream => Grant,
    }
}

// stream! {
//     pub struct AuthorizationStream {
//         Role(RoleStream),
//         Grant(GrantStream),
//     }
// }
//
// event! {
//     pub enum AuthorizationEvent {
//         type Stream = AuthorizationStream;
//         type Authority = Authority;
//     }
// }
//
// memory_store! {
//     pub struct AuthorizationMemoryStore {
//         type Event = AuthorizationEvent;
//     }
// }
//
// memory_store! {
//     type OtherAuthorizationMemoryStore = MemoryStore<Authority, RoleStream, GrantStream>
//     where AuthorizationEvent {
//         RoleStream => Role,
//         GrantStream => Grant,
//     }
// }

#[derive(Debug, Error)]
pub enum AuthorizationError<E: StdError + Send + Sync + 'static> {
    #[error("grant not found: {0}")]
    GrantNotFound(GrantId),

    #[error("role not found: {0}")]
    RoleNotFound(RoleId),

    #[error(transparent)]
    Store(E),
}

pub struct AuthorizationService<S>
where
    S: Store<Authority, RoleStream>
        + Store<Authority, GrantStream>
        + Observe<Event = AuthorizationEvent>,
{
    store: S,
}

impl<S> AuthorizationService<S>
where
    S: Store<Authority, RoleStream>
        + Store<Authority, GrantStream, Error = <S as Store<Authority, RoleStream>>::Error>
        + Observe<Event = AuthorizationEvent, Error = <S as Store<Authority, RoleStream>>::Error>,
{
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub async fn create_role(
        &self,
        authority: Authority,
    ) -> Result<RoleId, AuthorizationError<<S as Store<Authority, RoleStream>>::Error>> {
        let role_id = RoleId::new();
        Store::<Authority, RoleStream>::record(
            &self.store,
            authority,
            Utc::now(),
            role_id,
            RolePayload::Created,
            When::Empty,
        )
        .await
        .map_err(AuthorizationError::Store)?;
        Ok(role_id)
    }

    pub async fn grant_role(
        &self,
        authority: Authority,
        role_id: RoleId,
    ) -> Result<GrantId, AuthorizationError<<S as Store<Authority, RoleStream>>::Error>> {
        let role_page =
            Store::<Authority, RoleStream>::review(&self.store, role_id, After::Start, 1)
                .await
                .map_err(AuthorizationError::Store)?;

        if role_page.items.is_empty() {
            return Err(AuthorizationError::RoleNotFound(role_id));
        }

        let grant_id = GrantId::new();
        Store::<Authority, GrantStream>::record(
            &self.store,
            authority,
            Utc::now(),
            grant_id,
            GrantPayload::Created,
            When::Empty,
        )
        .await
        .map_err(AuthorizationError::Store)?;
        Ok(grant_id)
    }

    pub async fn revoke_grant(
        &self,
        authority: Authority,
        grant_id: GrantId,
    ) -> Result<(), AuthorizationError<<S as Store<Authority, RoleStream>>::Error>> {
        let page = Store::<Authority, GrantStream>::review(&self.store, grant_id, After::Start, 2)
            .await
            .map_err(AuthorizationError::Store)?;

        let Some(latest_event) = page.items.last() else {
            return Err(AuthorizationError::GrantNotFound(grant_id));
        };

        Store::<Authority, GrantStream>::record(
            &self.store,
            authority,
            Utc::now(),
            grant_id,
            GrantPayload::Revoked,
            When::Within(latest_event.event_id),
        )
        .await
        .map_err(AuthorizationError::Store)?;
        Ok(())
    }

    pub async fn latest_event_ids(
        &self,
    ) -> Result<
        (Option<EventId>, Option<EventId>),
        AuthorizationError<<S as Store<Authority, RoleStream>>::Error>,
    > {
        let mut latest_role = None;
        let mut latest_grant = None;
        let mut after = After::Start;
        loop {
            let page = self
                .store
                .observe(after, 128)
                .await
                .map_err(AuthorizationError::Store)?;
            for event in &page.items {
                match event {
                    AuthorizationEvent::Role(e) => latest_role = Some(e.event_id),
                    AuthorizationEvent::Grant(e) => latest_grant = Some(e.event_id),
                }
            }
            if !page.more {
                break;
            }
            after = After::Specific(page.next);
        }
        Ok((latest_role, latest_grant))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::Actor;

    fn make_service() -> AuthorizationService<AuthorizationMemoryStore> {
        AuthorizationService::new(AuthorizationMemoryStore::new())
    }

    #[tokio::test]
    async fn create_role_succeeds() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);

        service
            .create_role(authority)
            .await
            .expect("create should succeed");
    }

    #[tokio::test]
    async fn grant_role_succeeds() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);

        let role_id = service
            .create_role(authority.clone())
            .await
            .expect("create should succeed");
        service
            .grant_role(authority, role_id)
            .await
            .expect("grant should succeed");
    }

    #[tokio::test]
    async fn grant_role_fails_for_missing_role() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);

        let result = service.grant_role(authority, RoleId::new()).await;
        assert!(matches!(result, Err(AuthorizationError::RoleNotFound(_))));
    }

    #[tokio::test]
    async fn revoke_grant_succeeds() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);

        let role_id = service
            .create_role(authority.clone())
            .await
            .expect("create should succeed");
        let grant_id = service
            .grant_role(authority.clone(), role_id)
            .await
            .expect("grant should succeed");
        service
            .revoke_grant(authority, grant_id)
            .await
            .expect("revoke should succeed");
    }

    #[tokio::test]
    async fn revoke_grant_fails_for_missing_grant() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);

        let result = service.revoke_grant(authority, GrantId::new()).await;
        assert!(matches!(result, Err(AuthorizationError::GrantNotFound(_))));
    }

    #[tokio::test]
    async fn latest_event_ids_empty() {
        let service = make_service();
        let (role, grant) = service.latest_event_ids().await.expect("should succeed");
        assert!(role.is_none());
        assert!(grant.is_none());
    }

    #[tokio::test]
    async fn latest_event_ids_tracks_both_streams() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);

        let role_id = service
            .create_role(authority.clone())
            .await
            .expect("create should succeed");
        service
            .grant_role(authority.clone(), role_id)
            .await
            .expect("grant should succeed");

        let (role, grant) = service.latest_event_ids().await.expect("should succeed");
        assert!(role.is_some());
        assert!(grant.is_some());
        // Grant was recorded after role, so its event id is higher
        assert!(grant > role);
    }

    #[tokio::test]
    async fn latest_event_ids_after_revocation() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);

        let role_id = service
            .create_role(authority.clone())
            .await
            .expect("create should succeed");
        let grant_id = service
            .grant_role(authority.clone(), role_id)
            .await
            .expect("grant should succeed");
        service
            .revoke_grant(authority.clone(), grant_id)
            .await
            .expect("revoke should succeed");

        let (role, grant) = service.latest_event_ids().await.expect("should succeed");
        // Revocation is a grant event, so grant's latest is after role's
        assert!(grant > role);
    }
}
