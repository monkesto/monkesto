use super::{ChangeRoleActor, Role, RoleDecisionError};
use crate::authz::event::AuthzEvent;
use disintegrate::Decision;

impl Decision for ChangeRoleActor {
    type Event = AuthzEvent;
    type StateQuery = Role;
    type Error = RoleDecisionError;

    fn state_query(&self) -> Role {
        Role::new(self.role_id)
    }

    fn process(&self, role: &Role) -> Result<Vec<AuthzEvent>, Self::Error> {
        if !role.exists() {
            return Err(RoleDecisionError::NotFound(self.role_id));
        }
        if role.actors.contains(&self.actor) == self.add {
            return Ok(Vec::new());
        }
        Ok(vec![if self.add {
            AuthzEvent::RoleActorAdded {
                role_id: self.role_id,
                actor: self.actor.clone(),
                authority: self.authority.clone(),
                timestamp: self.timestamp,
            }
        } else {
            AuthzEvent::RoleActorRemoved {
                role_id: self.role_id,
                actor: self.actor.clone(),
                authority: self.authority.clone(),
                timestamp: self.timestamp,
            }
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
    fn adding_an_existing_actor_is_idempotent() {
        let role_id = RoleId::new();
        let actor = Actor::System;
        let mut role = Role::new(role_id);
        role.name = Some(Name::try_new("Administrator".into()).expect("valid name"));
        role.actors.insert(actor.clone());
        let decision = ChangeRoleActor::new(
            role_id,
            actor,
            true,
            Authority::Direct(Actor::System),
            Utc::now(),
        );
        assert!(decision.process(&role).expect("valid decision").is_empty());
    }
}
