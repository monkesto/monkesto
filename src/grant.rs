#![cfg_attr(not(test), expect(dead_code))]

use crate::authority::Authority;
use crate::id;
use crate::ident::Ident;
use crate::store::Event;
use crate::store::Outcome;
use crate::store::Store;
use crate::store::Stream;
use crate::store::When;
use crate::store::universal::registry::AnyPayload;
use crate::store::{After, EventId};
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error as StdError;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Payload)]
pub enum GrantPayload {
    Created,
    Revoked,
}

impl From<GrantPayload> for AnyPayload {
    fn from(val: GrantPayload) -> Self {
        AnyPayload::Grant(val)
    }
}

id!(GrantId, Ident::new16());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GrantState {
    Absent,
    Active { when: When<EventId> },
    Revoked { when: When<EventId> },
}

#[derive(Debug, Error)]
#[error("invalid grant state transition")]
struct GrantStateError;

impl GrantState {
    fn apply(
        &mut self,
        event: Event<Authority, GrantId, GrantPayload>,
    ) -> Result<(), GrantStateError> {
        let when = When::Within(event.event_id);
        *self = match (*self, event.payload) {
            (GrantState::Absent, GrantPayload::Created) => GrantState::Active { when },
            (GrantState::Active { .. }, GrantPayload::Revoked) => GrantState::Revoked { when },
            _ => return Err(GrantStateError),
        };
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum GrantCreateError<E: StdError + Send + Sync + 'static> {
    #[error("grant creation exceeded the maximum number of attempts")]
    AttemptsExceeded,

    #[error(transparent)]
    Store(#[from] E),
}

#[derive(Debug, Error)]
pub enum GrantRevokeError<E: StdError + Send + Sync + 'static> {
    #[error("invalid grant: {0}")]
    InvalidGrant(GrantId),

    #[error("grant has unexpected history: {0}")]
    UnexpectedHistory(GrantId),

    #[error("grant revocation exceeded the maximum number of attempts")]
    AttemptsExceeded,

    #[error(transparent)]
    Store(#[from] E),
}

pub struct GrantStream;
impl Stream for GrantStream {
    type Id = GrantId;
    type Payload = GrantPayload;
}

pub struct GrantService<G: Store<Authority, GrantStream>> {
    store: G,
}

impl<G: Store<Authority, GrantStream>> GrantService<G> {
    const MAX_EVENTS_PER_GRANT: usize = 2;
    const MAX_CREATE_ATTEMPTS: usize = 8;
    const MAX_REVOKE_ATTEMPTS: usize = 8;

    pub fn new(store: G) -> Self {
        Self { store }
    }

    pub async fn create(
        &self,
        authority: Authority,
    ) -> Result<GrantId, GrantCreateError<G::Error>> {
        for _ in 0..Self::MAX_CREATE_ATTEMPTS {
            let grant_id = GrantId::new();
            let outcome = self
                .store
                .record(
                    authority.clone(),
                    Utc::now(),
                    grant_id,
                    GrantPayload::Created,
                    When::Empty,
                )
                .await?;
            match outcome {
                Outcome::Recorded(_) => return Ok(grant_id),
                Outcome::Skipped => continue,
            }
        }

        Err(GrantCreateError::AttemptsExceeded)
    }

    pub async fn revoke(
        &self,
        grant_id: GrantId,
        authority: Authority,
    ) -> Result<(), GrantRevokeError<G::Error>> {
        for _ in 0..Self::MAX_REVOKE_ATTEMPTS {
            let page = self
                .store
                .review(grant_id, After::Start, Self::MAX_EVENTS_PER_GRANT)
                .await?;

            if page.more {
                return Err(GrantRevokeError::UnexpectedHistory(grant_id));
            }

            let mut state = GrantState::Absent;
            for event in page.items {
                state
                    .apply(event)
                    .map_err(|_| GrantRevokeError::UnexpectedHistory(grant_id))?;
            }

            let when = match state {
                GrantState::Absent => return Err(GrantRevokeError::InvalidGrant(grant_id)),
                GrantState::Revoked { .. } => return Ok(()),
                GrantState::Active { when } => when,
            };

            let outcome = self
                .store
                .record(
                    authority.clone(),
                    Utc::now(),
                    grant_id,
                    GrantPayload::Revoked,
                    when,
                )
                .await?;

            match outcome {
                Outcome::Recorded(_) => return Ok(()),
                Outcome::Skipped => continue,
            }
        }

        Err(GrantRevokeError::AttemptsExceeded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::Actor;
    use crate::store::memory::memory_store;

    memory_store! {
        type TestStore = MemoryStore<Authority, GrantStream>
    }

    fn make_service() -> GrantService<TestStore> {
        GrantService::new(TestStore::new())
    }

    #[tokio::test]
    async fn create_succeeds() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);

        service
            .create(authority)
            .await
            .expect("create should succeed");
    }

    #[tokio::test]
    async fn revoke_succeeds() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);
        let grant_id = service
            .create(authority.clone())
            .await
            .expect("create should succeed");

        service
            .revoke(grant_id, authority)
            .await
            .expect("revoke should succeed");
    }

    #[tokio::test]
    async fn revoke_fails_for_missing_grant() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);

        let result = service.revoke(GrantId::new(), authority).await;

        assert!(matches!(result, Err(GrantRevokeError::InvalidGrant(_))));
    }

    #[tokio::test]
    async fn revoke_is_idempotent() {
        let service = make_service();
        let authority = Authority::Direct(Actor::System);
        let grant_id = service
            .create(authority.clone())
            .await
            .expect("create should succeed");

        service
            .revoke(grant_id, authority.clone())
            .await
            .expect("first revoke should succeed");
        service
            .revoke(grant_id, authority)
            .await
            .expect("second revoke should be a no-op");
    }
}
