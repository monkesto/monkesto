use crate::authority::Authority;
use crate::id;
use crate::id::Ident;
use crate::journal::domain::{FundEvent, JournalDomainEvent};
use crate::journal::member::JournalMember;
use crate::journal::{Journal, JournalError, JournalId, Permissions, validate_permissions};
use crate::name::Name;
use crate::status::Status;
use crate::time_provider::Timestamp;
use disintegrate::{Decision, StateMutate, StateQuery};
use serde::{Deserialize, Serialize};

id!(FundId, Ident::new16());

#[derive(Debug, Default, Clone, StateQuery, Serialize, Deserialize)]
#[state_query(FundEvent)]
pub struct Fund {
    pub fund_id: FundId,
    pub journal_id: JournalId,
    pub name: Name,
    pub status: Status,
}

impl Fund {
    fn new(journal_id: JournalId, fund_id: FundId) -> Self {
        Self {
            fund_id,
            journal_id,
            ..Default::default()
        }
    }
}

impl StateMutate for Fund {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            FundEvent::FundCreated {
                fund_id,
                journal_id,
                fund_name,
                ..
            } => {
                self.fund_id = fund_id;
                self.journal_id = journal_id;
                self.name = fund_name;
                self.status = Status::Valid;
            }
        }
    }
}

pub struct CreateFund {
    fund_id: FundId,
    journal_id: JournalId,
    fund_name: Name,
    authority: Authority,
    timestamp: Timestamp,
}

impl CreateFund {
    pub fn new(
        fund_id: FundId,
        journal_id: JournalId,
        fund_name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            fund_id,
            journal_id,
            fund_name,
            authority,
            timestamp,
        }
    }
}

impl Decision for CreateFund {
    type Event = JournalDomainEvent;
    type StateQuery = (Journal, JournalMember, Fund);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Journal::new(self.journal_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
            Fund::new(self.journal_id, self.fund_id),
        )
    }

    fn process(
        &self,
        (journal, actor, fund): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if fund.status.found() {
            return Err(JournalError::FundIdCollision(self.fund_id));
        }

        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !validate_permissions(
            actor,
            self.authority,
            journal.owner,
            Permissions::CREATE_FUND,
        ) {
            return Err(JournalError::Permissions(Permissions::CREATE_FUND));
        }

        Ok(vec![JournalDomainEvent::FundCreated {
            fund_id: self.fund_id,
            journal_id: self.journal_id,
            fund_name: self.fund_name.clone(),
            authority: self.authority,
            timestamp: self.timestamp,
        }])
    }
}
