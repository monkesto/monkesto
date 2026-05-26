use super::role::RoleState;
use crate::role::RoleId;
use std::error::Error as StdError;

pub trait AuthzProjection: Send + Sync {
    type Error: StdError + Send + Sync + 'static;

    async fn role(&self, role_id: RoleId) -> Result<RoleState, Self::Error>;
}
