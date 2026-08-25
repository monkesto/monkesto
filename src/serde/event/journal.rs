use crate::journal::Permissions;
use crate::journal::activity::ActivityType;
use crate::journal::domain::JournalDomainEvent;
use crate::name::Name;
use crate::serde::error::ProtoError;
use crate::serde::error::ProtoError::FieldRequired;
use proto::event::journal::ProtoJournalDomainEvent;
use proto::event::journal::proto_journal_domain_event::{
    JournalDomainEventType, ProtoAccountCreated, ProtoAccountDeleted, ProtoAccountRenamed,
    ProtoActivityCreated, ProtoEntryCreated, ProtoFileUploaded, ProtoFundCreated,
    ProtoJournalCreated, ProtoJournalDeleted, ProtoMemberAdded, ProtoMemberPermissionsUpdated,
    ProtoMemberRemoved, ProtoTransactionCreated,
};

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
                account_type,
                timestamp,
            } => JournalDomainEventType::AccountCreated(ProtoAccountCreated {
                account_id: Some(account_id.into()),
                journal_id: Some(journal_id.into()),
                name: name.to_string(),
                account_type: account_type as i32,
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::AccountRenamed {
                account_id,
                journal_id,
                new_name,
                authority,
                timestamp,
            } => JournalDomainEventType::AccountRenamed(ProtoAccountRenamed {
                account_id: Some(account_id.into()),
                journal_id: Some(journal_id.into()),
                new_name: new_name.to_string(),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::AccountDeleted {
                account_id,
                journal_id,
                authority,
                timestamp,
            } => JournalDomainEventType::AccountDeleted(ProtoAccountDeleted {
                account_id: Some(account_id.into()),
                journal_id: Some(journal_id.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::FundCreated {
                fund_id,
                journal_id,
                fund_name,
                authority,
                timestamp,
            } => JournalDomainEventType::FundCreated(ProtoFundCreated {
                fund_id: Some(fund_id.into()),
                journal_id: Some(journal_id.into()),
                fund_name: fund_name.to_string(),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::TransactionCreated {
                transaction_id,
                journal_id,
                entries,
                financial_period,
                authority,
                timestamp,
            } => JournalDomainEventType::TransactionCreated(ProtoTransactionCreated {
                transaction_id: Some(transaction_id.into()),
                journal_id: Some(journal_id.into()),
                entries: Some(entries.into()),
                financial_period: financial_period as i32,
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::FileUploaded {
                file_id,
                journal_id,
                hash,
                file_name,
                authority,
                timestamp,
            } => JournalDomainEventType::FileUploaded(ProtoFileUploaded {
                file_id: Some(file_id.into()),
                journal_id: Some(journal_id.into()),
                hash: hash.into(),
                file_name,
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::ActivityCreated {
                activity_id,
                journal_id,
                activity_name,
                activity_type,
                authority,
                timestamp,
            } => JournalDomainEventType::ActivityCreated(ProtoActivityCreated {
                activity_id: Some(activity_id.into()),
                journal_id: Some(journal_id.into()),
                activity_name: activity_name.to_string(),
                activity_type: activity_type as i32,
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            JournalDomainEvent::EntryCreated {
                entry_id,
                journal_id,
                amount,
                entry_side,
                entry_kind,
                authority,
                timestamp,
            } => JournalDomainEventType::EntryCreated(ProtoEntryCreated {
                entry_id: Some(entry_id.into()),
                journal_id: Some(journal_id.into()),
                entry_kind: Some(entry_kind.into()),
                amount,
                entry_side: entry_side as i32,
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
                account_type: (ev.account_type as i8).try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::AccountRenamed(ev) => JournalDomainEvent::AccountRenamed {
                account_id: ev.account_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                new_name: Name::try_new(ev.new_name)?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::AccountDeleted(ev) => JournalDomainEvent::AccountDeleted {
                account_id: ev.account_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::FundCreated(ev) => JournalDomainEvent::FundCreated {
                fund_id: ev.fund_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                fund_name: Name::try_new(ev.fund_name)?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::TransactionCreated(ev) => {
                JournalDomainEvent::TransactionCreated {
                    transaction_id: ev.transaction_id.try_into()?,
                    journal_id: ev.journal_id.try_into()?,
                    entries: ev.entries.ok_or(FieldRequired)?.try_into()?,
                    financial_period: (ev.financial_period as i8).try_into()?,
                    authority: ev.authority.try_into()?,
                    timestamp: ev.timestamp.try_into()?,
                }
            }
            JournalDomainEventType::FileUploaded(ev) => JournalDomainEvent::FileUploaded {
                file_id: ev.file_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                hash: ev.hash.try_into().map_err(|_| ProtoError::Deserialize)?,
                file_name: ev.file_name,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::ActivityCreated(ev) => JournalDomainEvent::ActivityCreated {
                activity_id: ev.activity_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                activity_name: Name::try_new(ev.activity_name)?,
                activity_type: ActivityType::try_from(ev.activity_type as i8)?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            JournalDomainEventType::EntryCreated(ev) => JournalDomainEvent::EntryCreated {
                entry_id: ev.entry_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                amount: ev.amount,
                entry_side: (ev.entry_side as i8).try_into()?,
                entry_kind: ev.entry_kind.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
        };

        Ok(event)
    }
}
