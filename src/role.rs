#![cfg_attr(not(test), expect(dead_code))]

use crate::authority::Actor;
use crate::authority::Authority;
use crate::ident::Ident;
use crate::store::After;
use crate::store::Event;
use crate::store::EventId;
use crate::store::Observe;
use crate::store::Outcome;
use crate::store::Store;
use crate::store::Stream;
use crate::store::When;
use crate::store::memory::memory_store;
use crate::store::universal::registry::AnyPayload;
use crate::{id, payload};
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::Infallible;
use std::error::Error as StdError;
use std::sync::Arc;
use std::sync::Mutex;
use thiserror::Error;

id!(RoleId, Ident::new16());

payload! {
    AnyPayload::Role,

    pub enum RolePayload {
        Created,
        ActorAdded(Actor),
        ActorRemoved(Actor),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RoleState {
    Absent,
    Present {
        actors: HashSet<Actor>,
        when: When<EventId>,
    },
}

#[derive(Debug, Error)]
#[error("invalid role state transition")]
struct RoleStateError;

impl RoleState {
    fn apply(
        &mut self,
        event: Event<Authority, RoleId, RolePayload>,
    ) -> Result<(), RoleStateError> {
        let when = When::Within(event.event_id);
        *self = match (self.clone(), event.payload) {
            (RoleState::Absent, RolePayload::Created) => RoleState::Present {
                actors: HashSet::new(),
                when,
            },
            (RoleState::Present { mut actors, .. }, RolePayload::ActorAdded(actor)) => {
                actors.insert(actor);
                RoleState::Present { actors, when }
            }
            (RoleState::Present { mut actors, .. }, RolePayload::ActorRemoved(actor)) => {
                actors.remove(&actor);
                RoleState::Present { actors, when }
            }
            _ => return Err(RoleStateError),
        };

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum RoleCreateError<E: StdError + Send + Sync + 'static> {
    #[error("role creation exceeded the maximum number of attempts")]
    AttemptsExceeded,

    #[error(transparent)]
    Store(#[from] E),
}

#[derive(Debug, Error)]
pub enum RoleLookupError<E: StdError + Send + Sync + 'static> {
    #[error("role lookup requires system authority")]
    RequiresSystemAuthority,

    #[error(transparent)]
    Projection(#[from] E),
}

#[derive(Debug, Error)]
pub enum RoleAddError<E: StdError + Send + Sync + 'static> {
    #[error("invalid role")]
    InvalidRole,

    #[error("role update exceeded the maximum number of attempts")]
    AttemptsExceeded,

    #[error(transparent)]
    Store(#[from] E),
}

#[derive(Debug, Error)]
pub enum RoleRemoveError<E: StdError + Send + Sync + 'static> {
    #[error("invalid role")]
    InvalidRole,

    #[error("role update exceeded the maximum number of attempts")]
    AttemptsExceeded,

    #[error(transparent)]
    Store(#[from] E),
}

pub struct RoleStream;
impl Stream for RoleStream {
    type Id = RoleId;
    type Payload = RolePayload;
}

memory_store! {
    type RoleMemoryStore = MemoryStore<Authority, RoleStream>
}

/// Read-side role lookup abstraction.
///
/// Implementations may derive their answers in any way.
/// How those answers stay current is implementation-specific.
pub trait RoleProjection: Send + Sync {
    type Error: StdError + Send + Sync + 'static;

    async fn roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, Self::Error>;
}

/// Resolve lookups by replaying the role stream.
pub struct ObserveRoleProjection<R: Observe<Event = Event<Authority, RoleId, RolePayload>>> {
    store: R,
}

impl<R: Observe<Event = Event<Authority, RoleId, RolePayload>>> ObserveRoleProjection<R> {
    const REVIEW_PAGE_SIZE: usize = 128;

    pub fn new(store: R) -> Self {
        Self { store }
    }
}

impl<R: Observe<Event = Event<Authority, RoleId, RolePayload>>> RoleProjection
    for ObserveRoleProjection<R>
{
    type Error = R::Error;

    async fn roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, Self::Error> {
        let mut after = After::Start;
        let mut roles = HashMap::<RoleId, RoleState>::new();

        loop {
            let page = self.store.observe(after, Self::REVIEW_PAGE_SIZE).await?;

            for event in page.items {
                let role_id = event.id;
                let state = roles.entry(role_id).or_insert(RoleState::Absent);
                state
                    .apply(event)
                    .expect("all role events should form valid role state");
            }

            if !page.more {
                break;
            }

            after = After::Specific(page.next);
        }

        Ok(roles
            .into_iter()
            .filter_map(|(role_id, state)| match state {
                RoleState::Present { actors, .. } if actors.contains(actor) => Some(role_id),
                _ => None,
            })
            .collect())
    }
}

pub struct MemoryRoleProjection {
    state: Arc<Mutex<HashMap<Actor, HashSet<RoleId>>>>,
}

impl MemoryRoleProjection {
    pub async fn new(store: &RoleMemoryStore) -> Result<Arc<Self>, Infallible> {
        let projection = Arc::new(Self {
            state: Arc::new(Mutex::new(HashMap::new())),
        });
        let registered = projection.clone();
        store.register_callback(After::Start, move |event| registered.recorded(event));

        Ok(projection)
    }

    fn recorded(&self, event: &Event<Authority, RoleId, RolePayload>) {
        let mut state = self.state.lock().expect("poisoned");
        match &event.payload {
            RolePayload::Created => {}
            RolePayload::ActorAdded(actor) => {
                state.entry(actor.clone()).or_default().insert(event.id);
            }
            RolePayload::ActorRemoved(actor) => {
                if let Some(roles) = state.get_mut(actor) {
                    roles.remove(&event.id);
                    if roles.is_empty() {
                        state.remove(actor);
                    }
                }
            }
        }
    }
}

impl RoleProjection for MemoryRoleProjection {
    type Error = Infallible;

    async fn roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, Self::Error> {
        let state = self.state.lock().expect("poisoned");
        Ok(state.get(actor).cloned().unwrap_or_default())
    }
}

pub struct RoleService<R: Store<Authority, RoleStream>, P: RoleProjection> {
    store: R,
    projection: Arc<P>,
}

impl<R, P> RoleService<R, P>
where
    R: Store<Authority, RoleStream>,
    P: RoleProjection,
{
    pub fn with_projection(store: R, projection: P) -> Self {
        Self {
            store,
            projection: Arc::new(projection),
        }
    }
}

impl<R> RoleService<R, ObserveRoleProjection<R>>
where
    R: Store<Authority, RoleStream>
        + Observe<Event = Event<Authority, RoleId, RolePayload>>
        + Clone,
{
    pub fn new(store: R) -> Self {
        let projection = ObserveRoleProjection::new(store.clone());
        Self::with_projection(store, projection)
    }
}

impl RoleService<RoleMemoryStore, MemoryRoleProjection> {
    pub async fn with_memory_projection(store: RoleMemoryStore) -> Result<Self, Infallible> {
        let projection = MemoryRoleProjection::new(&store).await?;
        Ok(Self { store, projection })
    }
}

impl<R, P> RoleService<R, P>
where
    R: Store<Authority, RoleStream>,
    P: RoleProjection,
{
    const MAX_CREATE_ATTEMPTS: usize = 8;
    const MAX_UPDATE_ATTEMPTS: usize = 8;
    const REVIEW_PAGE_SIZE: usize = 128;

    pub async fn create(&self, authority: Authority) -> Result<RoleId, RoleCreateError<R::Error>> {
        for _ in 0..Self::MAX_CREATE_ATTEMPTS {
            let role_id = RoleId::new();
            let outcome = self
                .store
                .record(
                    authority.clone(),
                    Utc::now(),
                    role_id,
                    RolePayload::Created,
                    When::Empty,
                )
                .await?;

            match outcome {
                Outcome::Recorded(_) => {
                    return Ok(role_id);
                }
                Outcome::Skipped => continue,
            }
        }

        Err(RoleCreateError::AttemptsExceeded)
    }

    pub async fn roles(
        &self,
        authority: Authority,
        actor: &Actor,
    ) -> Result<HashSet<RoleId>, RoleLookupError<P::Error>> {
        if !matches!(authority.actor(), Actor::System) {
            return Err(RoleLookupError::RequiresSystemAuthority);
        }

        self.projection
            .as_ref()
            .roles(actor)
            .await
            .map_err(RoleLookupError::Projection)
    }

    pub async fn add(
        &self,
        authority: Authority,
        role_id: RoleId,
        actor: Actor,
    ) -> Result<(), RoleAddError<R::Error>> {
        for _ in 0..Self::MAX_UPDATE_ATTEMPTS {
            let LoadedRole {
                mut actors,
                latest_event_id,
            } = match self.load(role_id).await? {
                Some(loaded) => loaded,
                None => return Err(RoleAddError::InvalidRole),
            };

            if actors.contains(&actor) {
                return Ok(());
            }

            actors.insert(actor.clone());

            let outcome = self
                .store
                .record(
                    authority.clone(),
                    Utc::now(),
                    role_id,
                    RolePayload::ActorAdded(actor.clone()),
                    When::Within(latest_event_id),
                )
                .await?;

            match outcome {
                Outcome::Recorded(_) => {
                    return Ok(());
                }
                Outcome::Skipped => continue,
            }
        }

        Err(RoleAddError::AttemptsExceeded)
    }

    pub async fn remove(
        &self,
        authority: Authority,
        role_id: RoleId,
        actor: Actor,
    ) -> Result<(), RoleRemoveError<R::Error>> {
        for _ in 0..Self::MAX_UPDATE_ATTEMPTS {
            let LoadedRole {
                mut actors,
                latest_event_id,
            } = match self.load(role_id).await? {
                Some(loaded) => loaded,
                None => return Err(RoleRemoveError::InvalidRole),
            };

            if !actors.contains(&actor) {
                return Ok(());
            }

            actors.remove(&actor);

            let outcome = self
                .store
                .record(
                    authority.clone(),
                    Utc::now(),
                    role_id,
                    RolePayload::ActorRemoved(actor.clone()),
                    When::Within(latest_event_id),
                )
                .await?;

            match outcome {
                Outcome::Recorded(_) => {
                    return Ok(());
                }
                Outcome::Skipped => continue,
            }
        }

        Err(RoleRemoveError::AttemptsExceeded)
    }

    async fn load(&self, role_id: RoleId) -> Result<Option<LoadedRole>, R::Error> {
        let mut after = After::Start;
        let mut state = RoleState::Absent;
        let mut saw_event = false;

        loop {
            let page = self
                .store
                .review(role_id, after, Self::REVIEW_PAGE_SIZE)
                .await?;

            if page.items.is_empty() {
                return Ok(None);
            }

            for event in page.items {
                saw_event = true;
                state
                    .apply(event)
                    .expect("all role events should form valid role state");
            }

            if !page.more {
                break;
            }

            after = After::Specific(page.next);
        }

        if !saw_event {
            return Ok(None);
        }

        Ok(match state {
            RoleState::Present {
                actors,
                when: When::Within(latest_event_id),
            } => Some(LoadedRole {
                actors,
                latest_event_id,
            }),
            _ => None,
        })
    }
}

struct LoadedRole {
    actors: HashSet<Actor>,
    latest_event_id: EventId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::user::UserId;

    #[tokio::test]
    async fn returns_empty_roles_for_actor_without_membership() {
        let service = RoleService::new(RoleMemoryStore::new());
        let authority = Authority::Direct(Actor::System);
        let actor = Actor::User(UserId::new());

        service
            .create(authority.clone())
            .await
            .expect("create should succeed");

        let roles = service
            .roles(authority.clone(), &actor)
            .await
            .expect("lookup should succeed");

        assert!(roles.is_empty());
    }

    #[tokio::test]
    async fn adds_many_actors() {
        let service = RoleService::new(RoleMemoryStore::new());
        let authority = Authority::Direct(Actor::System);
        let role_id = service
            .create(authority.clone())
            .await
            .expect("create should succeed");
        let first_user = Actor::User(UserId::new());
        let second_user = Actor::User(UserId::new());

        service
            .add(authority.clone(), role_id, first_user.clone())
            .await
            .expect("first add should succeed");
        service
            .add(authority.clone(), role_id, second_user.clone())
            .await
            .expect("second add should succeed");
        service
            .add(authority.clone(), role_id, Actor::System)
            .await
            .expect("system add should succeed");

        let roles = service
            .roles(authority.clone(), &first_user)
            .await
            .expect("lookup should succeed");

        assert!(roles.contains(&role_id));
        assert_eq!(roles.len(), 1);

        let second_user_roles = service
            .roles(authority.clone(), &second_user)
            .await
            .expect("lookup should succeed");

        assert!(second_user_roles.contains(&role_id));

        let system_roles = service
            .roles(authority.clone(), &Actor::System)
            .await
            .expect("lookup should succeed");

        assert!(system_roles.contains(&role_id));
    }

    #[tokio::test]
    async fn deduplicates_adds() {
        let service = RoleService::new(RoleMemoryStore::new());
        let authority = Authority::Direct(Actor::System);
        let role_id = service
            .create(authority.clone())
            .await
            .expect("create should succeed");
        let user = Actor::User(UserId::new());

        service
            .add(authority.clone(), role_id, user.clone())
            .await
            .expect("first add should succeed");
        service
            .add(authority.clone(), role_id, user.clone())
            .await
            .expect("duplicate add should be a no-op");

        let roles = service
            .roles(authority.clone(), &user)
            .await
            .expect("lookup should succeed");

        assert!(roles.contains(&role_id));
        assert_eq!(roles.len(), 1);
    }

    #[tokio::test]
    async fn removes_actor() {
        let service = RoleService::new(RoleMemoryStore::new());
        let authority = Authority::Direct(Actor::System);
        let role_id = service
            .create(authority.clone())
            .await
            .expect("create should succeed");
        let user = Actor::User(UserId::new());

        service
            .add(authority.clone(), role_id, user.clone())
            .await
            .expect("add should succeed");
        service
            .remove(authority.clone(), role_id, user.clone())
            .await
            .expect("remove should succeed");

        let roles = service
            .roles(authority.clone(), &user)
            .await
            .expect("lookup should succeed");

        assert!(roles.is_empty());
    }

    #[tokio::test]
    async fn returns_all_roles_for_actor() {
        let store = RoleMemoryStore::new();
        let service = RoleService::new(store);
        let authority = Authority::Direct(Actor::System);
        let actor = Actor::User(UserId::new());
        let first_role_id = service
            .create(authority.clone())
            .await
            .expect("first create should succeed");
        let second_role_id = service
            .create(authority.clone())
            .await
            .expect("second create should succeed");

        service
            .add(authority.clone(), first_role_id, actor.clone())
            .await
            .expect("first add should succeed");
        service
            .add(authority.clone(), second_role_id, actor.clone())
            .await
            .expect("second add should succeed");

        let roles = service
            .roles(authority.clone(), &actor)
            .await
            .expect("lookup should succeed");

        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&first_role_id));
        assert!(roles.contains(&second_role_id));
    }

    #[tokio::test]
    async fn roles_requires_system_authority() {
        let service = RoleService::new(RoleMemoryStore::new());
        let actor = Actor::User(UserId::new());
        let authority = Authority::Direct(Actor::User(UserId::new()));

        let result = service.roles(authority, &actor).await;

        assert!(matches!(
            result,
            Err(RoleLookupError::RequiresSystemAuthority)
        ));
    }

    #[tokio::test]
    async fn memory_projection_stays_current_after_writes() {
        let service = RoleService::with_memory_projection(RoleMemoryStore::new())
            .await
            .expect("memory projection should initialize");
        let authority = Authority::Direct(Actor::System);
        let actor = Actor::User(UserId::new());
        let role_id = service
            .create(authority.clone())
            .await
            .expect("create should succeed");

        service
            .add(authority.clone(), role_id, actor.clone())
            .await
            .expect("add should succeed");

        let roles = service
            .roles(authority.clone(), &actor)
            .await
            .expect("lookup should succeed");

        assert_eq!(roles, HashSet::from([role_id]));
    }
}
