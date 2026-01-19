use crate::ident::Ident;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display},
    ops::Deref,
    str::FromStr,
};

use crate::id;
use crate::known_errors::KnownErrors;

id!(UserId, Ident::new16());

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
    Direct(Actor),
    // Delegated { authorizer: Actor, executor: Actor },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_userid_new() {
        let user_id = UserId::new();
        assert_eq!(user_id.to_string().len(), 16);
    }

    #[test]
    fn test_multiple_generated_ids_are_unique() {
        let id1 = UserId::new();
        let id2 = UserId::new();
        assert_ne!(id1, id2);
    }
}
