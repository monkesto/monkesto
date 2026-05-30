use super::RoleId;
use super::role::RoleState;
use crate::authority::Actor;
use std::collections::HashSet;
use std::error::Error as StdError;

pub trait AuthzProjection: Send + Sync {
    type Error: StdError + Send + Sync + 'static;

    async fn role(&self, role_id: RoleId) -> Result<RoleState, Self::Error>;

    async fn roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, Self::Error>;
}
