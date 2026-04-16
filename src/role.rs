#![cfg_attr(not(test), expect(dead_code))]

use crate::authority::Actor;
use crate::authority::Authority;
use crate::id;
use crate::ident::EntityId;
use crate::ident::Ident;
use crate::ident::IdentError;
use crate::store::After;
use crate::store::EventId;
use crate::store::Outcome;
use crate::store::Select;
use crate::store::Store;
use crate::store::Stream;
use crate::store::When;
use crate::store::universal::Payload;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error as StdError;
use std::fmt::Display;
use std::ops::Deref;
use std::str::FromStr;
use thiserror::Error;

id!(RoleId, RolePayload, RoleProjection, Ident::new16());

#[derive(Clone)]
pub struct RoleProjection {
    // TODO
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Payload)]
pub enum RolePayload {
    Created,
    ActorAdded(Actor),
    ActorRemoved(Actor),
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
    Store(#[from] E),
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

pub struct RoleService<R: Store<RoleStream>> {
    store: R,
}

impl<R: Store<RoleStream>> RoleService<R> {
    const MAX_CREATE_ATTEMPTS: usize = 8;
    const MAX_UPDATE_ATTEMPTS: usize = 8;
    const REVIEW_PAGE_SIZE: usize = 128;

    pub fn new(store: R) -> Self {
        Self { store }
    }

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
                Outcome::Recorded(_) => return Ok(role_id),
                Outcome::Skipped => continue,
            }
        }

        Err(RoleCreateError::AttemptsExceeded)
    }

    pub async fn roles(
        &self,
        authority: Authority,
        actor: &Actor,
    ) -> Result<HashSet<RoleId>, RoleLookupError<R::Error>> {
        if !matches!(authority.actor(), Actor::System) {
            return Err(RoleLookupError::RequiresSystemAuthority);
        }

        let mut after = After::Start;
        let mut roles = HashMap::<RoleId, Option<HashSet<Actor>>>::new();

        loop {
            let page = self
                .store
                .review(Select::All, after, Self::REVIEW_PAGE_SIZE)
                .await?;

            for event in page.items {
                let role_id = event.id;
                let state = roles.entry(role_id).or_default();
                Self::apply_payload(state, event.payload);
            }

            if !page.more {
                break;
            }

            after = After::Specific(page.next);
        }

        Ok(roles
            .into_iter()
            .filter_map(|(role_id, state)| match state {
                Some(actors) if actors.contains(actor) => Some(role_id),
                _ => None,
            })
            .collect())
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
            } = match self.load(role_id).await {
                Ok(Some(loaded)) => loaded,
                Ok(None) => return Err(RoleAddError::InvalidRole),
                Err(RoleLookupError::Store(err)) => return Err(RoleAddError::Store(err)),
                Err(RoleLookupError::RequiresSystemAuthority) => {
                    unreachable!("load does not validate authority")
                }
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
                Outcome::Recorded(_) => return Ok(()),
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
            } = match self.load(role_id).await {
                Ok(Some(loaded)) => loaded,
                Ok(None) => return Err(RoleRemoveError::InvalidRole),
                Err(RoleLookupError::Store(err)) => return Err(RoleRemoveError::Store(err)),
                Err(RoleLookupError::RequiresSystemAuthority) => {
                    unreachable!("load does not validate authority")
                }
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
                Outcome::Recorded(_) => return Ok(()),
                Outcome::Skipped => continue,
            }
        }

        Err(RoleRemoveError::AttemptsExceeded)
    }

    async fn load(&self, role_id: RoleId) -> Result<Option<LoadedRole>, RoleLookupError<R::Error>> {
        let mut after = After::Start;
        let mut actors = None;
        let mut latest_event_id = None;

        loop {
            let page = self
                .store
                .review(Select::One(role_id), after, Self::REVIEW_PAGE_SIZE)
                .await?;

            if page.items.is_empty() {
                return Ok(None);
            }

            for event in page.items {
                latest_event_id = Some(event.event_id);
                Self::apply_payload(&mut actors, event.payload);
            }

            if !page.more {
                break;
            }

            after = After::Specific(page.next);
        }

        Ok(match (actors, latest_event_id) {
            (Some(actors), Some(latest_event_id)) => Some(LoadedRole {
                actors,
                latest_event_id,
            }),
            _ => None,
        })
    }

    fn apply_payload(state: &mut Option<HashSet<Actor>>, payload: RolePayload) {
        match payload {
            RolePayload::Created if state.is_none() => {
                *state = Some(HashSet::new());
            }
            RolePayload::ActorAdded(actor) => {
                if let Some(actors) = state.as_mut() {
                    actors.insert(actor);
                }
            }
            RolePayload::ActorRemoved(actor) => {
                if let Some(actors) = state.as_mut() {
                    actors.remove(&actor);
                }
            }
            RolePayload::Created => {}
        }
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
    use crate::store::MemoryStore;
    #[tokio::test]
    async fn returns_empty_roles_for_actor_without_membership() {
        let service = RoleService::new(MemoryStore::<RoleId, RolePayload>::default());
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
        let service = RoleService::new(MemoryStore::<RoleId, RolePayload>::default());
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
        let service = RoleService::new(MemoryStore::<RoleId, RolePayload>::default());
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
        let service = RoleService::new(MemoryStore::<RoleId, RolePayload>::default());
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
        let store = MemoryStore::<RoleId, RolePayload>::default();
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
        let service = RoleService::new(MemoryStore::<RoleId, RolePayload>::default());
        let actor = Actor::User(UserId::new());
        let authority = Authority::Direct(Actor::User(UserId::new()));

        let result = service.roles(authority, &actor).await;

        assert!(matches!(
            result,
            Err(RoleLookupError::RequiresSystemAuthority)
        ));
    }
}
