#![cfg_attr(not(test), expect(dead_code))]

use super::GrantId;
use super::GrantPayload;
use super::RoleId;
use super::RolePayload;
use super::grant::GrantStream;
use super::projection::AuthzProjection;
use super::role::RoleState;
use super::role::RoleStream;
use super::store::AuthzEvent;
use super::store::AuthzId;
use super::store::AuthzStore;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::name::Name;
use crate::store::After;
use crate::store::EventFamily;
use crate::store::Outcome;
use crate::store::When;
use chrono::Utc;
use std::collections::HashSet;
use std::error::Error as StdError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthzError<S: StdError + Send + Sync + 'static, P: StdError + Send + Sync + 'static> {
    #[error("grant not found: {0}")]
    GrantNotFound(GrantId),

    #[error("grant update was skipped: {0}")]
    GrantSkipped(GrantId),

    #[error("grant has unexpected history: {0}")]
    GrantUnexpectedHistory(GrantId),

    #[error("role not found: {0}")]
    RoleNotFound(RoleId),

    #[error("role creation was skipped: {0}")]
    RoleSkipped(RoleId),

    #[error("role lookup requires system authority")]
    RoleRequiresSystemAuthority,

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

impl<S, P> Clone for AuthzService<S, P>
where
    S: AuthzStore + Clone,
    P: AuthzProjection + Clone,
{
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            projection: self.projection.clone(),
        }
    }
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
            .record::<RoleStream>(
                authority,
                Utc::now(),
                role_id,
                RolePayload::Created(name),
                When::Empty,
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
            .record::<GrantStream>(
                authority,
                Utc::now(),
                grant_id,
                GrantPayload::Created,
                When::Empty,
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

        if page.more {
            return Err(AuthzError::GrantUnexpectedHistory(grant_id));
        }

        let Some(latest_event) = page.items.last() else {
            return Err(AuthzError::GrantNotFound(grant_id));
        };

        match latest_event {
            AuthzEvent::Grant(event) if event.payload == GrantPayload::Revoked => return Ok(()),
            AuthzEvent::Grant(event) if event.payload == GrantPayload::Created => {}
            AuthzEvent::Grant(_) => return Err(AuthzError::GrantUnexpectedHistory(grant_id)),
            AuthzEvent::Role(_) => return Err(AuthzError::GrantUnexpectedHistory(grant_id)),
        }

        let outcome = self
            .store
            .record::<GrantStream>(
                authority,
                Utc::now(),
                grant_id,
                GrantPayload::Revoked,
                When::Within(latest_event.event_id()),
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

    pub async fn roles(
        &self,
        authority: Authority,
        actor: &Actor,
    ) -> Result<HashSet<RoleId>, AuthzError<S::Error, P::Error>> {
        if !matches!(authority.actor(), Actor::System) {
            return Err(AuthzError::RoleRequiresSystemAuthority);
        }

        self.projection
            .roles(actor)
            .await
            .map_err(AuthzError::Projection)
    }

    pub async fn all_roles(
        &self,
    ) -> Result<Vec<(RoleId, RoleState)>, AuthzError<S::Error, P::Error>> {
        self.projection
            .all_roles()
            .await
            .map_err(AuthzError::Projection)
    }

    pub async fn add_role_actor(
        &self,
        authority: Authority,
        role_id: RoleId,
        actor: Actor,
    ) -> Result<(), AuthzError<S::Error, P::Error>> {
        let RoleState::Present { actors, when, .. } = self.role(role_id).await? else {
            return Err(AuthzError::RoleNotFound(role_id));
        };

        if actors.contains(&actor) {
            return Ok(());
        }

        let outcome = self
            .store
            .record::<RoleStream>(
                authority,
                Utc::now(),
                role_id,
                RolePayload::ActorAdded(actor),
                when,
            )
            .await
            .map_err(AuthzError::Store)?;

        match outcome {
            Outcome::Recorded(_) => Ok(()),
            Outcome::Skipped => Err(AuthzError::RoleSkipped(role_id)),
        }
    }

    pub async fn remove_role_actor(
        &self,
        authority: Authority,
        role_id: RoleId,
        actor: Actor,
    ) -> Result<(), AuthzError<S::Error, P::Error>> {
        let RoleState::Present { actors, when, .. } = self.role(role_id).await? else {
            return Err(AuthzError::RoleNotFound(role_id));
        };

        if !actors.contains(&actor) {
            return Ok(());
        }

        let outcome = self
            .store
            .record::<RoleStream>(
                authority,
                Utc::now(),
                role_id,
                RolePayload::ActorRemoved(actor),
                when,
            )
            .await
            .map_err(AuthzError::Store)?;

        match outcome {
            Outcome::Recorded(_) => Ok(()),
            Outcome::Skipped => Err(AuthzError::RoleSkipped(role_id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::memory::AuthzMemoryProjection;
    use super::super::memory::AuthzMemoryStore;
    use super::super::sqlite::AuthzSqliteProjection;
    use super::*;
    use crate::auth::user::UserId;
    use crate::store::Event;
    use crate::store::Store;
    use crate::store::sqlite::SqliteStore;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::str::FromStr;
    use tempfile::TempDir;

    type MemoryAuthzStore = AuthzMemoryStore<AuthzMemoryProjection>;
    type MemoryAuthzService = AuthzService<MemoryAuthzStore, AuthzMemoryProjection>;
    type SqliteAuthzStore = SqliteStore<AuthzEvent, AuthzSqliteProjection>;
    type SqliteAuthzService = AuthzService<SqliteAuthzStore, AuthzSqliteProjection>;

    struct MemoryFixture {
        service: MemoryAuthzService,
    }

    struct SqliteFixture {
        _dir: TempDir,
        store: SqliteAuthzStore,
        service: SqliteAuthzService,
    }

    async fn memory_fixture() -> MemoryFixture {
        let projection = AuthzMemoryProjection::default();
        let store = AuthzMemoryStore::with_projection(projection.clone());
        MemoryFixture {
            service: AuthzService::new(store, projection),
        }
    }

    async fn sqlite_fixture() -> SqliteFixture {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("authz.sqlite");
        let url = format!("sqlite://{}", path.display());
        let connection_options = SqliteConnectOptions::from_str(&url)
            .expect("sqlite url should parse")
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connection_options)
            .await
            .expect("sqlite pool should connect");
        let projection = AuthzSqliteProjection::new(pool.clone());
        let store = SqliteStore::new(pool, projection.clone())
            .await
            .expect("sqlite store should initialize");
        let service = AuthzService::new(store.clone(), projection);
        SqliteFixture {
            _dir: dir,
            store,
            service,
        }
    }

    macro_rules! authz_service_contract_tests {
        ($fixture:expr) => {
            #[tokio::test]
            async fn create_role_succeeds() {
                let fixture = $fixture.await;
                let service = &fixture.service;
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
                let fixture = $fixture.await;
                let service = &fixture.service;
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
                let fixture = $fixture.await;
                let service = &fixture.service;
                let authority = Authority::Direct(Actor::System);

                let result = service.grant_role(authority, RoleId::new()).await;
                assert!(matches!(result, Err(AuthzError::RoleNotFound(_))));
            }

            #[tokio::test]
            async fn role_returns_absent_for_missing_role() {
                let fixture = $fixture.await;
                let service = &fixture.service;

                let result = service
                    .role(RoleId::new())
                    .await
                    .expect("role lookup should succeed");

                assert!(matches!(result, RoleState::Absent));
            }

            #[tokio::test]
            async fn roles_returns_empty_for_actor_without_membership() {
                let fixture = $fixture.await;
                let service = &fixture.service;
                let authority = Authority::Direct(Actor::System);
                let actor = Actor::User(UserId::new());

                service
                    .create_role(
                        authority.clone(),
                        Name::try_new("Administrator".to_string()).expect("valid name"),
                    )
                    .await
                    .expect("create should succeed");

                let roles = service
                    .roles(authority, &actor)
                    .await
                    .expect("lookup should succeed");

                assert!(roles.is_empty());
            }

            #[tokio::test]
            async fn roles_requires_system_authority() {
                let fixture = $fixture.await;
                let service = &fixture.service;
                let actor = Actor::User(UserId::new());
                let authority = Authority::Direct(Actor::User(UserId::new()));

                let result = service.roles(authority, &actor).await;

                assert!(matches!(
                    result,
                    Err(AuthzError::RoleRequiresSystemAuthority)
                ));
            }

            #[tokio::test]
            async fn add_role_actor_adds_many_actors() {
                let fixture = $fixture.await;
                let service = &fixture.service;
                let authority = Authority::Direct(Actor::System);
                let role_id = service
                    .create_role(
                        authority.clone(),
                        Name::try_new("Administrator".to_string()).expect("valid name"),
                    )
                    .await
                    .expect("create should succeed");
                let first_user = Actor::User(UserId::new());
                let second_user = Actor::User(UserId::new());

                service
                    .add_role_actor(authority.clone(), role_id, first_user.clone())
                    .await
                    .expect("first add should succeed");
                service
                    .add_role_actor(authority.clone(), role_id, second_user.clone())
                    .await
                    .expect("second add should succeed");
                service
                    .add_role_actor(authority.clone(), role_id, Actor::System)
                    .await
                    .expect("system add should succeed");

                let first_user_roles = service
                    .roles(authority.clone(), &first_user)
                    .await
                    .expect("lookup should succeed");
                assert_eq!(first_user_roles, HashSet::from([role_id]));

                let second_user_roles = service
                    .roles(authority.clone(), &second_user)
                    .await
                    .expect("lookup should succeed");
                assert_eq!(second_user_roles, HashSet::from([role_id]));

                let system_roles = service
                    .roles(authority, &Actor::System)
                    .await
                    .expect("lookup should succeed");
                assert_eq!(system_roles, HashSet::from([role_id]));
            }

            #[tokio::test]
            async fn add_role_actor_is_idempotent() {
                let fixture = $fixture.await;
                let service = &fixture.service;
                let authority = Authority::Direct(Actor::System);
                let role_id = service
                    .create_role(
                        authority.clone(),
                        Name::try_new("Administrator".to_string()).expect("valid name"),
                    )
                    .await
                    .expect("create should succeed");
                let user = Actor::User(UserId::new());

                service
                    .add_role_actor(authority.clone(), role_id, user.clone())
                    .await
                    .expect("first add should succeed");
                service
                    .add_role_actor(authority.clone(), role_id, user.clone())
                    .await
                    .expect("duplicate add should be a no-op");

                let roles = service
                    .roles(authority, &user)
                    .await
                    .expect("lookup should succeed");

                assert_eq!(roles, HashSet::from([role_id]));
            }

            #[tokio::test]
            async fn remove_role_actor_removes_actor() {
                let fixture = $fixture.await;
                let service = &fixture.service;
                let authority = Authority::Direct(Actor::System);
                let role_id = service
                    .create_role(
                        authority.clone(),
                        Name::try_new("Administrator".to_string()).expect("valid name"),
                    )
                    .await
                    .expect("create should succeed");
                let user = Actor::User(UserId::new());

                service
                    .add_role_actor(authority.clone(), role_id, user.clone())
                    .await
                    .expect("add should succeed");
                service
                    .remove_role_actor(authority.clone(), role_id, user.clone())
                    .await
                    .expect("remove should succeed");

                let roles = service
                    .roles(authority, &user)
                    .await
                    .expect("lookup should succeed");
                assert!(roles.is_empty());

                let role = service.role(role_id).await.expect("role should load");
                assert!(matches!(role, RoleState::Present { actors, .. } if actors.is_empty()));
            }

            #[tokio::test]
            async fn roles_returns_all_roles_for_actor() {
                let fixture = $fixture.await;
                let service = &fixture.service;
                let authority = Authority::Direct(Actor::System);
                let actor = Actor::User(UserId::new());
                let first_role_id = service
                    .create_role(
                        authority.clone(),
                        Name::try_new("Administrator".to_string()).expect("valid name"),
                    )
                    .await
                    .expect("first create should succeed");
                let second_role_id = service
                    .create_role(
                        authority.clone(),
                        Name::try_new("Auditor".to_string()).expect("valid name"),
                    )
                    .await
                    .expect("second create should succeed");

                service
                    .add_role_actor(authority.clone(), first_role_id, actor.clone())
                    .await
                    .expect("first add should succeed");
                service
                    .add_role_actor(authority.clone(), second_role_id, actor.clone())
                    .await
                    .expect("second add should succeed");

                let roles = service
                    .roles(authority, &actor)
                    .await
                    .expect("lookup should succeed");

                assert_eq!(roles, HashSet::from([first_role_id, second_role_id]));
            }

            #[tokio::test]
            async fn all_roles_returns_created_roles() {
                let fixture = $fixture.await;
                let service = &fixture.service;
                let authority = Authority::Direct(Actor::System);
                let first_role_id = service
                    .create_role(
                        authority.clone(),
                        Name::try_new("Administrator".to_string()).expect("valid name"),
                    )
                    .await
                    .expect("first create should succeed");
                let second_role_id = service
                    .create_role(
                        authority,
                        Name::try_new("Auditor".to_string()).expect("valid name"),
                    )
                    .await
                    .expect("second create should succeed");

                let roles = service.all_roles().await.expect("roles should load");
                let role_ids = roles
                    .into_iter()
                    .map(|(role_id, _)| role_id)
                    .collect::<HashSet<_>>();

                assert_eq!(role_ids, HashSet::from([first_role_id, second_role_id]));
            }

            #[tokio::test]
            async fn revoke_grant_succeeds() {
                let fixture = $fixture.await;
                let service = &fixture.service;
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

            #[tokio::test]
            async fn revoke_grant_fails_for_missing_grant() {
                let fixture = $fixture.await;
                let service = &fixture.service;
                let authority = Authority::Direct(Actor::System);

                let result = service.revoke_grant(authority, GrantId::new()).await;

                assert!(matches!(result, Err(AuthzError::GrantNotFound(_))));
            }

            #[tokio::test]
            async fn revoke_grant_is_idempotent() {
                let fixture = $fixture.await;
                let service = &fixture.service;
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
                    .revoke_grant(authority.clone(), grant_id)
                    .await
                    .expect("first revoke should succeed");
                service
                    .revoke_grant(authority, grant_id)
                    .await
                    .expect("second revoke should be a no-op");
            }
        };
    }

    mod memory_contract {
        use super::*;

        authz_service_contract_tests!(memory_fixture());
    }

    mod sqlite_contract {
        use super::*;

        authz_service_contract_tests!(sqlite_fixture());
    }

    #[tokio::test]
    async fn sqlite_records_events_and_updates_projection() {
        let fixture = sqlite_fixture().await;
        let authority = Authority::Direct(Actor::System);
        let actor = Actor::User(UserId::new());

        let role_id = fixture
            .service
            .create_role(
                authority.clone(),
                Name::try_new("Administrator".to_string()).expect("valid name"),
            )
            .await
            .expect("create should succeed");
        fixture
            .service
            .add_role_actor(authority.clone(), role_id, actor.clone())
            .await
            .expect("add should succeed");

        let roles = fixture
            .service
            .roles(authority, &actor)
            .await
            .expect("roles should load");
        assert_eq!(roles, HashSet::from([role_id]));

        let events = fixture
            .store
            .observe(After::Start, 10)
            .await
            .expect("events should load");
        assert_eq!(events.items.len(), 2);
    }

    #[tokio::test]
    async fn sqlite_grant_and_revoke_persist_event_history() {
        let fixture = sqlite_fixture().await;
        let authority = Authority::Direct(Actor::System);

        let role_id = fixture
            .service
            .create_role(
                authority.clone(),
                Name::try_new("Administrator".to_string()).expect("valid name"),
            )
            .await
            .expect("create should succeed");
        let grant_id = fixture
            .service
            .grant_role(authority.clone(), role_id)
            .await
            .expect("grant should succeed");
        fixture
            .service
            .revoke_grant(authority.clone(), grant_id)
            .await
            .expect("revoke should succeed");
        fixture
            .service
            .revoke_grant(authority, grant_id)
            .await
            .expect("second revoke should be a no-op");

        let grant_events = fixture
            .store
            .review(AuthzId::Grant(grant_id), After::Start, 10)
            .await
            .expect("grant events should load");

        assert_eq!(grant_events.items.len(), 2);
        assert!(matches!(
            grant_events.items[0],
            AuthzEvent::Grant(Event {
                payload: GrantPayload::Created,
                ..
            })
        ));
        assert!(matches!(
            grant_events.items[1],
            AuthzEvent::Grant(Event {
                payload: GrantPayload::Revoked,
                ..
            })
        ));
    }
}
