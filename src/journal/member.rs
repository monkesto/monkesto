use crate::authn::UserId;
use crate::authority::Authority;
use crate::journal::domain::{JournalDomainEvent, MemberEvent};
use crate::journal::{Journal, JournalError, JournalId, Permissions, validate_permissions};
use crate::status::Status;
use crate::time_provider::Timestamp;
use axum_test::expect_json::__private::serde_trampoline::{Deserialize, Serialize};
use disintegrate::{Decision, StateMutate, StateQuery};
use std::collections::HashMap;

#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(MemberEvent)]
pub struct JournalMember {
    #[id]
    journal_id: JournalId,
    #[id]
    user_id: UserId,
    pub permissions: Permissions,
    pub status: Status,
}

impl JournalMember {
    pub(crate) fn new(journal_id: JournalId, user_id: UserId) -> Self {
        Self {
            journal_id,
            user_id,
            ..Default::default()
        }
    }
}

impl StateMutate for JournalMember {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            MemberEvent::MemberAdded { permissions, .. } => {
                self.permissions = permissions;
                self.status = Status::Valid;
            }
            MemberEvent::MemberPermissionsUpdated { permissions, .. } => {
                self.permissions = permissions;
            }
            MemberEvent::MemberRemoved { .. } => {
                self.status = Status::Deleted;
            }
        }
    }
}

#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(MemberEvent)]
pub struct JournalMemberList {
    #[id]
    journal_id: JournalId,
    members: HashMap<UserId, Permissions>,
}

#[expect(unused)]
impl JournalMemberList {
    fn new(journal_id: JournalId) -> Self {
        Self {
            journal_id,
            ..Default::default()
        }
    }
}

impl StateMutate for JournalMemberList {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            MemberEvent::MemberAdded {
                user_id,
                permissions,
                ..
            } => _ = self.members.insert(user_id, permissions),
            MemberEvent::MemberPermissionsUpdated {
                user_id,
                permissions,
                ..
            } => _ = self.members.insert(user_id, permissions),
            MemberEvent::MemberRemoved { user_id, .. } => _ = self.members.remove(&user_id),
        }
    }
}

pub struct AddJournalMember {
    journal_id: JournalId,
    user_id: UserId,
    permissions: Permissions,
    authority: Authority,
    timestamp: Timestamp,
}

impl AddJournalMember {
    pub(crate) fn new(
        journal_id: JournalId,
        user_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            journal_id,
            user_id,
            permissions,
            authority,
            timestamp,
        }
    }
}

impl Decision for AddJournalMember {
    type Event = JournalDomainEvent;
    type StateQuery = (Journal, JournalMember, JournalMember);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Journal::new(self.journal_id),
            JournalMember::new(self.journal_id, self.user_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
        )
    }

    fn process(
        &self,
        (journal, member, actor): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if member.status.valid() || journal.owner == self.user_id {
            return Err(JournalError::UserAlreadyHasAccess(self.user_id));
        }

        if !validate_permissions(
            actor,
            &self.authority,
            journal.owner,
            Permissions::INVITE.union(self.permissions),
        ) {
            return Err(JournalError::Permissions(
                Permissions::INVITE.union(self.permissions),
            ));
        }

        Ok(vec![JournalDomainEvent::MemberAdded {
            journal_id: self.journal_id,
            user_id: self.user_id,
            permissions: self.permissions,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

pub struct UpdateJournalMember {
    journal_id: JournalId,
    user_id: UserId,
    permissions: Permissions,
    authority: Authority,
    timestamp: Timestamp,
}

impl UpdateJournalMember {
    pub(crate) fn new(
        journal_id: JournalId,
        user_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            journal_id,
            user_id,
            permissions,
            authority,
            timestamp,
        }
    }
}

impl Decision for UpdateJournalMember {
    type Event = JournalDomainEvent;
    type StateQuery = (Journal, JournalMember, JournalMember);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Journal::new(self.journal_id),
            JournalMember::new(self.journal_id, self.user_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
        )
    }

    fn process(
        &self,
        (journal, member, actor): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !member.status.valid() {
            return Err(JournalError::UserDoesntHaveAccess(self.user_id));
        }

        if !validate_permissions(
            actor,
            &self.authority,
            journal.owner,
            Permissions::OWNER.union(self.permissions),
        ) {
            return Err(JournalError::Permissions(
                Permissions::OWNER.union(self.permissions),
            ));
        }

        Ok(vec![JournalDomainEvent::MemberPermissionsUpdated {
            journal_id: self.journal_id,
            permissions: self.permissions,
            user_id: self.user_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

pub struct RemoveJournalMember {
    journal_id: JournalId,
    user_id: UserId,
    authority: Authority,
    timestamp: Timestamp,
}

impl RemoveJournalMember {
    pub(crate) fn new(
        journal_id: JournalId,
        user_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            journal_id,
            user_id,
            authority,
            timestamp,
        }
    }
}

impl Decision for RemoveJournalMember {
    type Event = JournalDomainEvent;
    type StateQuery = (Journal, JournalMember, JournalMember);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Journal::new(self.journal_id),
            JournalMember::new(self.journal_id, self.user_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
        )
    }

    fn process(
        &self,
        (journal, member, actor): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !member.status.valid() {
            return Err(JournalError::UserDoesntHaveAccess(self.user_id));
        }

        if !validate_permissions(actor, &self.authority, journal.owner, Permissions::OWNER) {
            return Err(JournalError::Permissions(Permissions::OWNER));
        }

        Ok(vec![JournalDomainEvent::MemberRemoved {
            journal_id: self.journal_id,
            user_id: self.user_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}
