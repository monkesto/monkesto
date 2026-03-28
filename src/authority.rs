pub use crate::auth::user::UserId;
pub use crate::grant::GrantId;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Actor {
    User(UserId),
    System,
    Anonymous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Authority {
    Direct(Actor),
    Delegated {
        grantor: Actor,
        grant: GrantId,
        grantee: Actor,
    },
}

impl Authority {
    pub fn actor(&self) -> &Actor {
        match self {
            Authority::Direct(actor) => actor,
            Authority::Delegated { grantee, .. } => grantee,
        }
    }
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
