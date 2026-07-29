use crate::journal::Permissions;
use crate::journal::domain::JournalDomainEvent;
use crate::name::Name;
use crate::proto::event::journal::ProtoJournalDomainEvent;
use crate::proto::event::journal::proto_journal_domain_event::{
    JournalDomainEventType, ProtoAccountCreated, ProtoAccountDeleted, ProtoAccountRenamed,
    ProtoJournalCreated, ProtoJournalDeleted, ProtoMemberAdded, ProtoMemberPermissionsUpdated,
    ProtoMemberRemoved, ProtoTransactionCreated, ProtoTransactionDeleted,
};
use crate::serde::error::ProtoError;
use crate::serde::error::ProtoError::FieldRequired;

impl From<JournalDomainEvent> for ProtoJournalDomainEvent {
    fn from(event: JournalDomainEvent) -> Self {
        let event = match event {
            JournalDomainEvent::JournalCreated {
                journal_id,
                owner,
                name,
                authority,
                timestamp,
            } => JournalDomainEventType::JournalCreated(ProtoJournalCreated {
                journal_id: Some(journal_id.into()),
                owner_id: Some(owner.into()),
                name: name.to_string(),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::JournalDeleted {
                journal_id,
                authority,
                timestamp,
            } => JournalDomainEventType::JournalDeleted(ProtoJournalDeleted {
                journal_id: Some(journal_id.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::MemberAdded {
                journal_id,
                user_id,
                permissions,
                authority,
                timestamp,
            } => JournalDomainEventType::MemberAdded(ProtoMemberAdded {
                journal_id: Some(journal_id.into()),
                user_id: Some(user_id.into()),
                permissions: permissions.bits(),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::MemberPermissionsUpdated {
                journal_id,
                user_id,
                permissions,
                authority,
                timestamp,
            } => JournalDomainEventType::MemberPermissionsUpdated(ProtoMemberPermissionsUpdated {
                journal_id: Some(journal_id.into()),
                user_id: Some(user_id.into()),
                permissions: permissions.bits(),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::MemberRemoved {
                journal_id,
                user_id,
                authority,
                timestamp,
            } => JournalDomainEventType::MemberRemoved(ProtoMemberRemoved {
                journal_id: Some(journal_id.into()),
                user_id: Some(user_id.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::AccountCreated {
                account_id,
                journal_id,
                name,
                authority,
                timestamp,
            } => JournalDomainEventType::AccountCreated(ProtoAccountCreated {
                account_id: Some(account_id.into()),
                journal_id: Some(journal_id.into()),
                name: name.to_string(),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::AccountRenamed {
                account_id,
                new_name,
                authority,
                timestamp,
            } => JournalDomainEventType::AccountRenamed(ProtoAccountRenamed {
                account_id: Some(account_id.into()),
                new_name: new_name.to_string(),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::AccountDeleted {
                account_id,
                authority,
                timestamp,
            } => JournalDomainEventType::AccountDeleted(ProtoAccountDeleted {
                account_id: Some(account_id.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::TransactionCreated {
                transaction_id,
                journal_id,
                balance_updates,
                authority,
                timestamp,
            } => JournalDomainEventType::TransactionCreated(ProtoTransactionCreated {
                transaction_id: Some(transaction_id.into()),
                journal_id: Some(journal_id.into()),
                entries: Some(balance_updates.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::TransactionDeleted {
                transaction_id,
                authority,
                timestamp,
            } => JournalDomainEventType::TransactionDeleted(ProtoTransactionDeleted {
                transaction_id: Some(transaction_id.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
        };

        ProtoJournalDomainEvent {
            journal_domain_event_type: Some(event),
        }
    }
}

impl TryFrom<ProtoJournalDomainEvent> for JournalDomainEvent {
    type Error = ProtoError;

    fn try_from(event: ProtoJournalDomainEvent) -> Result<Self, Self::Error> {
        let event = match event.journal_domain_event_type.ok_or(FieldRequired)? {
            JournalDomainEventType::JournalCreated(ev) => JournalDomainEvent::JournalCreated {
                journal_id: ev.journal_id.try_into()?,
                owner: ev.owner_id.try_into()?,
                name: Name::try_new(ev.name)?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::JournalDeleted(ev) => JournalDomainEvent::JournalDeleted {
                journal_id: ev.journal_id.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::MemberAdded(ev) => JournalDomainEvent::MemberAdded {
                journal_id: ev.journal_id.try_into()?,
                user_id: ev.user_id.try_into()?,
                permissions: Permissions::from_bits(ev.permissions)
                    .ok_or(ProtoError::PermissionDecode(ev.permissions))?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::MemberPermissionsUpdated(ev) => {
                JournalDomainEvent::MemberPermissionsUpdated {
                    journal_id: ev.journal_id.try_into()?,
                    user_id: ev.user_id.try_into()?,
                    permissions: Permissions::from_bits(ev.permissions)
                        .ok_or(ProtoError::PermissionDecode(ev.permissions))?,
                    authority: ev.authority.try_into()?,
                    timestamp: ev.timestamp.try_into()?,
                }
            }
            JournalDomainEventType::MemberRemoved(ev) => JournalDomainEvent::MemberRemoved {
                journal_id: ev.journal_id.try_into()?,
                user_id: ev.user_id.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::AccountCreated(ev) => JournalDomainEvent::AccountCreated {
                account_id: ev.account_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                name: Name::try_new(ev.name)?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::AccountRenamed(ev) => JournalDomainEvent::AccountRenamed {
                account_id: ev.account_id.try_into()?,
                new_name: Name::try_new(ev.new_name)?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::AccountDeleted(ev) => JournalDomainEvent::AccountDeleted {
                account_id: ev.account_id.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::TransactionCreated(ev) => {
                JournalDomainEvent::TransactionCreated {
                    transaction_id: ev.transaction_id.try_into()?,
                    journal_id: ev.journal_id.try_into()?,
                    balance_updates: ev.entries.ok_or(FieldRequired)?.try_into()?,
                    authority: ev.authority.try_into()?,
                    timestamp: ev.timestamp.try_into()?,
                }
            }
            JournalDomainEventType::TransactionDeleted(ev) => {
                JournalDomainEvent::TransactionDeleted {
                    transaction_id: ev.transaction_id.try_into()?,
                    authority: ev.authority.try_into()?,
                    timestamp: ev.timestamp.try_into()?,
                }
            }
        };

        Ok(event)
    }
}
