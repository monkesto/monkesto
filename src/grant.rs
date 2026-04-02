use crate::authority::Authority;
use crate::id;
use crate::ident::Ident;
use crate::ident::IdentError;
use crate::store::After;
use crate::store::Outcome;
use crate::store::Select;
use crate::store::Store;
use crate::store::When;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error as StdError;
use std::fmt::Display;
use std::ops::Deref;
use std::str::FromStr;
use thiserror::Error;

id!(GrantId, Ident::new16());

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantEvent {
    Created,
    Revoked,
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

pub struct GrantService<G: Store<Id = GrantId, Payload = GrantEvent>> {
    store: G,
}

impl<G: Store<Id = GrantId, Payload = GrantEvent>> GrantService<G> {
    const MAX_EVENTS_PER_GRANT: usize = 2;
    const MAX_CREATE_ATTEMPTS: usize = 8;
    const MAX_REVOKE_ATTEMPTS: usize = 8;

    #[expect(dead_code)]
    pub fn new(store: G) -> Self {
        Self { store }
    }

    #[expect(dead_code)]
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
                    GrantEvent::Created,
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

    #[expect(dead_code)]
    pub async fn revoke(
        &self,
        grant_id: GrantId,
        authority: Authority,
    ) -> Result<(), GrantRevokeError<G::Error>> {
        for _ in 0..Self::MAX_REVOKE_ATTEMPTS {
            let page = self
                .store
                .review(
                    Select::One(grant_id),
                    After::Start,
                    Self::MAX_EVENTS_PER_GRANT,
                )
                .await?;

            if page.more {
                return Err(GrantRevokeError::UnexpectedHistory(grant_id));
            }

            let Some(latest_event) = page.items.last() else {
                return Err(GrantRevokeError::InvalidGrant(grant_id));
            };

            match &latest_event.payload {
                GrantEvent::Revoked => return Ok(()),
                GrantEvent::Created => {}
            }

            let outcome = self
                .store
                .record(
                    authority.clone(),
                    Utc::now(),
                    grant_id,
                    GrantEvent::Revoked,
                    When::Within(latest_event.event_id),
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
