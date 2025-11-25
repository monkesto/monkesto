use super::journal::JournalEvent;
use super::user::UserEvent;
use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "payload")]
pub enum DomainEvent {
    User(UserEvent),
    Journal(JournalEvent),
}

impl DomainEvent {
    pub fn to_user_event(&self) -> Result<UserEvent, ServerFnError> {
        if let Self::User(s) = self {
            return Ok(s.clone());
        }

        Err(ServerFnError::ServerError(
            "unable to convert domain event to user event".to_string(),
        ))
    }
    pub fn to_journal_event(&self) -> Result<JournalEvent, ServerFnError> {
        if let Self::Journal(s) = self {
            return Ok(s.clone());
        }
        Err(ServerFnError::ServerError(
            "unable to convert domain event to account event".to_string(),
        ))
    }
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum AggregateType {
    User = 1,
    Journal = 2,
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum EventType {
    // User events (1-99)
    UserCreated = 1,
    UsernameUpdated = 2,
    UserPasswordUpdated = 3,
    UserLoggedIn = 4,
    UserLoggedOut = 5,
    UserCreatedJournal = 6,
    UserInvitedToJournal = 7,
    UserAcceptedJournalInvite = 8,
    UserDeclinedJournalInvite = 9,
    UserRemovedFromJournal = 10,
    UserSelectedJournal = 11,
    UserDeleted = 12,

    // Journal events (100-199)
    JournalCreated = 100,
    JournalRenamed = 101,
    JournalAccountCreated = 102,
    JournalAccountDeleted = 103,
    JournalAddedEntry = 104,
    JournalDeleted = 105,
}

impl EventType {
    pub fn from_user_event(user_event: &UserEvent) -> Self {
        match user_event {
            UserEvent::Created { .. } => Self::UserCreated,
            UserEvent::UsernameUpdated { .. } => Self::UsernameUpdated,
            UserEvent::PasswordUpdated { .. } => Self::UserPasswordUpdated,
            UserEvent::LoggedIn { .. } => Self::UserLoggedIn,
            UserEvent::LoggedOut { .. } => Self::UserLoggedOut,
            UserEvent::CreatedJournal { .. } => Self::UserCreatedJournal,
            UserEvent::InvitedToJournal { .. } => Self::UserInvitedToJournal,
            UserEvent::AcceptedJournalInvite { .. } => Self::UserAcceptedJournalInvite,
            UserEvent::DeclinedJournalInvite { .. } => Self::UserDeclinedJournalInvite,
            UserEvent::RemovedFromJournal { .. } => Self::UserRemovedFromJournal,
            UserEvent::SelectedJournal { .. } => Self::UserSelectedJournal,
            UserEvent::Deleted => Self::UserDeleted,
        }
    }

    pub fn from_journal_event(journal_event: &JournalEvent) -> Self {
        match journal_event {
            JournalEvent::Created { .. } => Self::JournalCreated,
            JournalEvent::Renamed { .. } => Self::JournalRenamed,
            JournalEvent::CreatedAccount { .. } => Self::JournalAccountCreated,
            JournalEvent::DeletedAccount { .. } => Self::JournalAccountDeleted,
            JournalEvent::AddedEntry { .. } => Self::JournalAddedEntry,
            JournalEvent::Deleted => Self::JournalDeleted,
        }
    }
}
