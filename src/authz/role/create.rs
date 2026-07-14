use super::{CreateRole, Role, RoleDecisionError};
use crate::authz::event::AuthzEvent;
use disintegrate::Decision;

impl Decision for CreateRole {
    type Event = AuthzEvent;
    type StateQuery = Role;
    type Error = RoleDecisionError;

    fn state_query(&self) -> Role {
        Role::new(self.role_id)
    }

    fn process(&self, role: &Role) -> Result<Vec<AuthzEvent>, Self::Error> {
        if role.exists() {
            return Err(RoleDecisionError::Exists(self.role_id));
        }
        Ok(vec![AuthzEvent::RoleCreated {
            role_id: self.role_id,
            name: self.name.clone(),
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::{Actor, Authority};
    use crate::authz::RoleId;
    use crate::name::Name;
    use chrono::Utc;

    #[test]
    fn creating_a_role_emits_role_created() {
        let role_id = RoleId::new();
        let decision = CreateRole::new(
            role_id,
            Name::try_new("Administrator".into()).expect("valid name"),
            Authority::Direct(Actor::System),
            Utc::now(),
        );
        let events = decision
            .process(&decision.state_query())
            .expect("valid decision");
        assert!(
            matches!(&events[..], [AuthzEvent::RoleCreated { role_id: id, .. }] if *id == role_id)
        );
    }
}
