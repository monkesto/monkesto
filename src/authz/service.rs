#![cfg_attr(not(test), expect(dead_code))]

use super::projection::AuthzProjection;
use super::role::RoleState;
use super::store::AuthzId;
use super::store::AuthzRecord;
use super::store::AuthzStore;
use crate::authority::Authority;
use crate::grant::GrantId;
use crate::grant::GrantPayload;
use crate::name::Name;
use crate::role::RoleId;
use crate::role::RolePayload;
use crate::store::revised::After;
use crate::store::revised::EventFamily;
use crate::store::revised::Outcome;
use crate::store::revised::Record;
use crate::store::revised::When;
use chrono::Utc;
use std::error::Error as StdError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthzError<S: StdError + Send + Sync + 'static, P: StdError + Send + Sync + 'static> {
    #[error("grant not found: {0}")]
    GrantNotFound(GrantId),

    #[error("grant update was skipped: {0}")]
    GrantSkipped(GrantId),

    #[error("role not found: {0}")]
    RoleNotFound(RoleId),

    #[error("role creation was skipped: {0}")]
    RoleSkipped(RoleId),

    #[error("grant creation was skipped: {0}")]
    GrantCreationSkipped(GrantId),

    #[error(transparent)]
    Store(S),

    #[error(transparent)]
    Projection(P),
}

pub struct AuthzService<S, P>
where
    S: AuthzStore,
    P: AuthzProjection,
{
    store: S,
    projection: P,
}

impl<S, P> AuthzService<S, P>
where
    S: AuthzStore,
    P: AuthzProjection,
{
    pub fn new(store: S, projection: P) -> Self {
        Self { store, projection }
    }

    pub async fn create_role(
        &self,
        authority: Authority,
        name: Name,
    ) -> Result<RoleId, AuthzError<S::Error, P::Error>> {
        let role_id = RoleId::new();
        let outcome = self
            .store
            .record(
                authority,
                Utc::now(),
                AuthzRecord::Role(Record {
                    id: role_id,
                    payload: RolePayload::Created(name),
                    when: When::Empty,
                }),
            )
            .await
            .map_err(AuthzError::Store)?;

        match outcome {
            Outcome::Recorded(_) => Ok(role_id),
            Outcome::Skipped => Err(AuthzError::RoleSkipped(role_id)),
        }
    }

    pub async fn grant_role(
        &self,
        authority: Authority,
        role_id: RoleId,
    ) -> Result<GrantId, AuthzError<S::Error, P::Error>> {
        if matches!(self.role(role_id).await?, RoleState::Absent) {
            return Err(AuthzError::RoleNotFound(role_id));
        }

        let grant_id = GrantId::new();
        let outcome = self
            .store
            .record(
                authority,
                Utc::now(),
                AuthzRecord::Grant(Record {
                    id: grant_id,
                    payload: GrantPayload::Created,
                    when: When::Empty,
                }),
            )
            .await
            .map_err(AuthzError::Store)?;

        match outcome {
            Outcome::Recorded(_) => Ok(grant_id),
            Outcome::Skipped => Err(AuthzError::GrantCreationSkipped(grant_id)),
        }
    }

    pub async fn revoke_grant(
        &self,
        authority: Authority,
        grant_id: GrantId,
    ) -> Result<(), AuthzError<S::Error, P::Error>> {
        let page = self
            .store
            .review(AuthzId::Grant(grant_id), After::Start, 2)
            .await
            .map_err(AuthzError::Store)?;

        let Some(latest_event) = page.items.last() else {
            return Err(AuthzError::GrantNotFound(grant_id));
        };

        let outcome = self
            .store
            .record(
                authority,
                Utc::now(),
                AuthzRecord::Grant(Record {
                    id: grant_id,
                    payload: GrantPayload::Revoked,
                    when: When::Within(latest_event.event_id()),
                }),
            )
            .await
            .map_err(AuthzError::Store)?;

        match outcome {
            Outcome::Recorded(_) => Ok(()),
            Outcome::Skipped => Err(AuthzError::GrantSkipped(grant_id)),
        }
    }

    pub async fn role(&self, role_id: RoleId) -> Result<RoleState, AuthzError<S::Error, P::Error>> {
        self.projection
            .role(role_id)
            .await
            .map_err(AuthzError::Projection)
    }
}

#[cfg(test)]
mod tests {
    use super::super::memory::AuthzMemoryProjection;
    use super::super::memory::AuthzMemoryStore;
    use super::*;
    use crate::authority::Actor;

    #[tokio::test]
    async fn create_role_succeeds() {
        let projection = AuthzMemoryProjection::default();
        let store = AuthzMemoryStore::with_projection(projection.clone());
        let service = AuthzService::new(store, projection);
        let authority = Authority::Direct(Actor::System);

        let role_id = service
            .create_role(
                authority,
                Name::try_new("Administrator".to_string()).expect("valid name"),
            )
            .await
            .expect("create should succeed");

        assert!(matches!(
            service.role(role_id).await.expect("role should load"),
            RoleState::Present { .. }
        ));
    }

    #[tokio::test]
    async fn grant_role_succeeds() {
        let projection = AuthzMemoryProjection::default();
        let store = AuthzMemoryStore::with_projection(projection.clone());
        let service = AuthzService::new(store, projection);
        let authority = Authority::Direct(Actor::System);

        let role_id = service
            .create_role(
                authority.clone(),
                Name::try_new("Administrator".to_string()).expect("valid name"),
            )
            .await
            .expect("create should succeed");
        service
            .grant_role(authority, role_id)
            .await
            .expect("grant should succeed");
    }

    #[tokio::test]
    async fn grant_role_fails_for_missing_role() {
        let projection = AuthzMemoryProjection::default();
        let store = AuthzMemoryStore::with_projection(projection.clone());
        let service = AuthzService::new(store, projection);
        let authority = Authority::Direct(Actor::System);

        let result = service.grant_role(authority, RoleId::new()).await;
        assert!(matches!(result, Err(AuthzError::RoleNotFound(_))));
    }

    #[tokio::test]
    async fn revoke_grant_succeeds() {
        let projection = AuthzMemoryProjection::default();
        let store = AuthzMemoryStore::with_projection(projection.clone());
        let service = AuthzService::new(store, projection);
        let authority = Authority::Direct(Actor::System);

        let role_id = service
            .create_role(
                authority.clone(),
                Name::try_new("Administrator".to_string()).expect("valid name"),
            )
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
}
