use super::{Grant, GrantDecisionError, RevokeGrant};
use crate::authz::event::AuthzEvent;
use disintegrate::Decision;

impl Decision for RevokeGrant {
    type Event = AuthzEvent;
    type StateQuery = Grant;
    type Error = GrantDecisionError;

    fn state_query(&self) -> Grant {
        Grant::new(self.grant_id)
    }

    fn process(&self, grant: &Grant) -> Result<Vec<AuthzEvent>, Self::Error> {
        if !grant.found {
            return Err(GrantDecisionError::NotFound(self.grant_id));
        }
        if grant.revoked {
            return Ok(Vec::new());
        }
        Ok(vec![AuthzEvent::GrantRevoked {
            grant_id: self.grant_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::{Actor, Authority};
    use crate::authz::GrantId;
    use chrono::Utc;

    fn authority() -> Authority {
        Authority::Direct(Actor::System)
    }

    #[test]
    fn revoking_a_missing_grant_fails() {
        let grant_id = GrantId::new();
        let decision = RevokeGrant::new(grant_id, authority(), Utc::now());
        assert_eq!(
            decision.process(&decision.state_query()),
            Err(GrantDecisionError::NotFound(grant_id))
        );
    }

    #[test]
    fn revoking_an_already_revoked_grant_is_idempotent() {
        let grant_id = GrantId::new();
        let grant = Grant {
            grant_id,
            found: true,
            revoked: true,
        };
        let decision = RevokeGrant::new(grant_id, authority(), Utc::now());
        assert!(decision.process(&grant).expect("valid decision").is_empty());
    }
}
