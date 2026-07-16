pub(crate) use crate::authn::user::UserId;
pub use crate::authz::GrantId;
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
            Authority::Delegated { grantor, .. } => grantor,
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("Failed to serialize authority")
    }
}

impl TryFrom<&[u8]> for Authority {
    type Error = postcard::Error;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        postcard::from_bytes(value)
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
