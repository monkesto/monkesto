use super::{CreateGrant, Grant, GrantDecisionError};
use crate::authz::event::AuthzEvent;
use crate::authz::role::Role;
use disintegrate::Decision;

impl Decision for CreateGrant {
    type Event = AuthzEvent;
    type StateQuery = (Role, Grant);
    type Error = GrantDecisionError;

    fn state_query(&self) -> Self::StateQuery {
        (Role::new(self.role_id), Grant::new(self.grant_id))
    }

    fn process(&self, (role, grant): &Self::StateQuery) -> Result<Vec<AuthzEvent>, Self::Error> {
        if !role.exists() {
            return Err(GrantDecisionError::RoleNotFound(self.role_id));
        }
        if grant.found {
            return Err(GrantDecisionError::Exists(self.grant_id));
        }
        Ok(vec![AuthzEvent::GrantCreated {
            grant_id: self.grant_id,
            role_id: self.role_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::{Actor, Authority};
    use crate::authz::{GrantId, RoleId};
    use chrono::Utc;

    #[test]
    fn granting_a_missing_role_fails() {
        let role_id = RoleId::new();
        let decision = CreateGrant::new(
            GrantId::new(),
            role_id,
            Authority::Direct(Actor::System),
            Utc::now(),
        );
        assert_eq!(
            decision.process(&decision.state_query()),
            Err(GrantDecisionError::RoleNotFound(role_id))
        );
    }
}
