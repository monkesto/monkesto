use cuid::Cuid2Constructor;
use nutype::nutype;

#[nutype(
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Serialize,
        Deserialize,
        AsRef,
        Display,
        TryFrom
    ),
    validate(len_char_min = 16, len_char_max = 16)
)]
pub struct UserId(String);

impl UserId {
    #[allow(unused)]
    pub fn new() -> Self {
        UserId::try_from(Cuid2Constructor::new().with_length(16).create_id())
            .expect("Generated cuid2 should always be valid")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Actor {
    #[allow(unused)]
    User(UserId),
    #[allow(unused)]
    System,
    #[allow(unused)]
    Anonymous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authority {
    pub authorizer: Actor,
    pub executor: Actor,
}

impl Authority {
    #[allow(unused)]
    pub fn direct(actor: &Actor) -> Self {
        Self {
            authorizer: actor.clone(),
            executor: actor.clone(),
        }
    }

    #[allow(unused)]
    pub fn delegated(authorizer: &Actor, executor: &Actor) -> Self {
        Self {
            authorizer: authorizer.clone(),
            executor: executor.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_userid_new() {
        let user_id = UserId::new();
        assert_eq!(user_id.as_ref().len(), 16);
    }

    #[test]
    fn test_multiple_generated_ids_are_unique() {
        let id1 = UserId::new();
        let id2 = UserId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_authority_new() {
        let actor = Actor::User(UserId::new());
        let authority = Authority::direct(&actor);
        assert_eq!(authority.authorizer, actor);
        assert_eq!(authority.executor, actor);
    }

    #[test]
    fn test_authority_delegated() {
        let actor = Actor::User(UserId::new());
        let system = Actor::System;
        let authority = Authority::delegated(&system, &actor);
        assert_eq!(authority.authorizer, system);
        assert_eq!(authority.executor, actor);
    }
}
