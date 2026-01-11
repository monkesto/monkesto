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
pub enum Authority {
    #[expect(unused)]
    Direct(Actor),
    // Delegated { authorizer: Actor, executor: Actor },
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
}
