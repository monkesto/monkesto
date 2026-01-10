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
    sanitize(trim, lowercase),
    validate(predicate = |email| email.contains('@'))
)]
pub struct Email(String);

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
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::try_from(Cuid2Constructor::new().with_length(16).create_id())
            .expect("Generated cuid2 should always be valid")
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct User {
    id: UserId,
    email: Email,
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
