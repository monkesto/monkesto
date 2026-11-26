use crate::authn::UserId;
use crate::id::Ident;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::activity::{ActivityId, ActivityKind};
use crate::journal::donation::DonationId;
use crate::journal::donation::donation_entry::DonationEntries;
use crate::journal::donation::donor::DonorId;
use crate::journal::file::FileId;
use crate::journal::fund::FundId;
use crate::journal::service::{EventAction, EventType, StreamType};
use crate::journal::transaction::entry::TransactionEntries;
use crate::journal::transaction::memo::Memo;
use crate::journal::transaction::{FinancialPeriod, TransactionId};
use crate::journal::{JournalId, Permissions};
use crate::name::Name;
use crate::proto::DecodeError;
use crate::proto::DecodeError::FieldRequired;
use crate::proto::journal::event::journal_event::ProtoJournalDomainEvent;
use crate::proto::journal::event::journal_event::proto_journal_domain_event::{
    JournalDomainEventType, ProtoAccountCreated, ProtoAccountDeleted, ProtoAccountRenamed,
    ProtoActivityCreated, ProtoDonationCreated, ProtoDonorCreated, ProtoFileDeleted,
    ProtoFileUploaded, ProtoFundCreated, ProtoJournalCreated, ProtoJournalDeleted,
    ProtoMemberAdded, ProtoMemberPermissionsUpdated, ProtoMemberRemoved, ProtoTransactionCreated,
};
use prost::Message;
use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JournalDomainEvent {
    JournalCreated {
        journal_id: JournalId,
        owner: UserId,
        name: Name,
    },
    JournalDeleted {
        journal_id: JournalId,
    },
    MemberAdded {
        journal_id: JournalId,
        user_id: UserId,
        permissions: Permissions,
    },
    MemberPermissionsUpdated {
        journal_id: JournalId,
        user_id: UserId,
        permissions: Permissions,
    },
    MemberRemoved {
        journal_id: JournalId,
        user_id: UserId,
    },
    AccountCreated {
        account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        account_type: AccountType,
    },
    AccountRenamed {
        account_id: AccountId,
        journal_id: JournalId,
        new_name: Name,
    },
    AccountDeleted {
        account_id: AccountId,
        journal_id: JournalId,
    },

    FundCreated {
        fund_id: FundId,
        journal_id: JournalId,
        fund_name: Name,
    },

    ActivityCreated {
        activity_id: ActivityId,
        journal_id: JournalId,
        activity_name: Name,
        activity_kind: ActivityKind,
    },

    TransactionCreated {
        transaction_id: TransactionId,
        journal_id: JournalId,
        entries: TransactionEntries,
        period: FinancialPeriod,
        memo: Option<Memo>,
    },

    FileUploaded {
        file_id: FileId,
        journal_id: JournalId,
        hash: [u8; 16],
        file_name: String,
    },

    FileDeleted {
        file_id: FileId,
        journal_id: JournalId,
    },

    DonorCreated {
        donor_id: DonorId,
        journal_id: JournalId,
        name: Name,
    },

    DonationCreated {
        donation_id: DonationId,
        journal_id: JournalId,
        deposit_account_id: AccountId,
        entries: DonationEntries,
        donation_total: u64,
    },
}

impl JournalDomainEvent {
    pub fn stream_type(&self) -> StreamType {
        use JournalDomainEvent::*;
        match self {
            JournalCreated { .. }
            | JournalDeleted { .. }
            | MemberAdded { .. }
            | MemberPermissionsUpdated { .. }
            | MemberRemoved { .. } => StreamType::Journal,
            AccountCreated { .. } | AccountRenamed { .. } | AccountDeleted { .. } => {
                StreamType::Account
            }
            FundCreated { .. } => StreamType::Fund,
            ActivityCreated { .. } => StreamType::Activity,
            TransactionCreated { .. } => StreamType::Transaction,
            FileUploaded { .. } => StreamType::File,
            FileDeleted { .. } => StreamType::File,
            DonorCreated { .. } => StreamType::Donor,
            DonationCreated { .. } => StreamType::Donation,
        }
    }

    pub fn event_type(&self) -> EventType {
        use EventType as ET;
        use JournalDomainEvent::*;
        match self {
            JournalCreated { .. } => ET::JournalCreated,
            JournalDeleted { .. } => ET::JournalDeleted,
            MemberAdded { .. } => ET::MemberAdded,
            MemberPermissionsUpdated { .. } => ET::MemberPermissionsUpdated,
            MemberRemoved { .. } => ET::MemberRemoved,
            AccountCreated { .. } => ET::AccountCreated,
            AccountRenamed { .. } => ET::AccountRenamed,
            AccountDeleted { .. } => ET::AccountDeleted,
            FundCreated { .. } => ET::FundCreated,
            ActivityCreated { .. } => ET::ActivityCreated,
            TransactionCreated { .. } => ET::TransactionCreated,
            FileUploaded { .. } => ET::FileUploaded,
            FileDeleted { .. } => ET::FileDeleted,
            DonorCreated { .. } => ET::DonorCreated,
            DonationCreated { .. } => EventType::DonationCreated,
        }
    }

    pub fn event_action(&self) -> EventAction {
        use EventAction::*;
        use JournalDomainEvent::*;

        match self {
            JournalCreated { .. } => Created,
            JournalDeleted { .. } => Deleted,

            // member events modify the journal stream
            MemberAdded { .. } => Modified,
            MemberPermissionsUpdated { .. } => Modified,
            MemberRemoved { .. } => Modified,

            AccountCreated { .. } => Created,
            AccountRenamed { .. } => Modified,
            AccountDeleted { .. } => Deleted,

            FundCreated { .. } => Created,

            ActivityCreated { .. } => Created,

            TransactionCreated { .. } => Created,

            FileUploaded { .. } => Created,
            FileDeleted { .. } => Deleted,

            DonorCreated { .. } => Created,

            DonationCreated { .. } => Created,
        }
    }

    pub fn journal_id(&self) -> JournalId {
        use JournalDomainEvent::*;

        match self {
            JournalCreated { journal_id, .. }
            | JournalDeleted { journal_id, .. }
            | MemberAdded { journal_id, .. }
            | MemberPermissionsUpdated { journal_id, .. }
            | MemberRemoved { journal_id, .. }
            | AccountCreated { journal_id, .. }
            | AccountRenamed { journal_id, .. }
            | AccountDeleted { journal_id, .. }
            | FundCreated { journal_id, .. }
            | ActivityCreated { journal_id, .. }
            | TransactionCreated { journal_id, .. }
            | FileUploaded { journal_id, .. }
            | FileDeleted { journal_id, .. }
            | DonorCreated { journal_id, .. }
            | DonationCreated { journal_id, .. } => *journal_id,
        }
    }

    pub fn stream_id(&self) -> Ident {
        use JournalDomainEvent::*;

        match self {
            JournalCreated { journal_id, .. }
            | JournalDeleted { journal_id, .. }
            | MemberAdded { journal_id, .. }
            | MemberPermissionsUpdated { journal_id, .. }
            | MemberRemoved { journal_id, .. } => **journal_id,

            AccountCreated { account_id, .. }
            | AccountRenamed { account_id, .. }
            | AccountDeleted { account_id, .. } => **account_id,

            FundCreated { fund_id, .. } => **fund_id,

            ActivityCreated { activity_id, .. } => **activity_id,

            TransactionCreated { transaction_id, .. } => **transaction_id,

            FileUploaded { file_id, .. } | FileDeleted { file_id, .. } => **file_id,

            DonorCreated { donor_id, .. } => **donor_id,

            DonationCreated { donation_id, .. } => **donation_id,
        }
    }
}

impl Type<Postgres> for JournalDomainEvent {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&[u8] as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for JournalDomainEvent {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = ProtoJournalDomainEvent::from(self.clone()).encode_to_vec();
        <Vec<u8> as Encode<Postgres>>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, Postgres> for JournalDomainEvent {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <&[u8] as Decode<Postgres>>::decode(value)?;
        Ok(ProtoJournalDomainEvent::decode(bytes)?.try_into()?)
    }
}

impl From<JournalDomainEvent> for ProtoJournalDomainEvent {
    fn from(event: JournalDomainEvent) -> Self {
        let event = match event {
            JournalDomainEvent::JournalCreated {
                journal_id,
                owner,
                name,
            } => JournalDomainEventType::JournalCreated(ProtoJournalCreated {
                journal_id: Some(journal_id.into()),
                owner_id: Some(owner.into()),
                name: name.to_string(),
            }),
            JournalDomainEvent::JournalDeleted { journal_id } => {
                JournalDomainEventType::JournalDeleted(ProtoJournalDeleted {
                    journal_id: Some(journal_id.into()),
                })
            }
            JournalDomainEvent::MemberAdded {
                journal_id,
                user_id,
                permissions,
            } => JournalDomainEventType::MemberAdded(ProtoMemberAdded {
                journal_id: Some(journal_id.into()),
                user_id: Some(user_id.into()),
                permissions: permissions.bits(),
            }),
            JournalDomainEvent::MemberPermissionsUpdated {
                journal_id,
                user_id,
                permissions,
            } => JournalDomainEventType::MemberPermissionsUpdated(ProtoMemberPermissionsUpdated {
                journal_id: Some(journal_id.into()),
                user_id: Some(user_id.into()),
                permissions: permissions.bits(),
            }),
            JournalDomainEvent::MemberRemoved {
                journal_id,
                user_id,
            } => JournalDomainEventType::MemberRemoved(ProtoMemberRemoved {
                journal_id: Some(journal_id.into()),
                user_id: Some(user_id.into()),
            }),
            JournalDomainEvent::AccountCreated {
                account_id,
                journal_id,
                name,
                account_type,
            } => JournalDomainEventType::AccountCreated(ProtoAccountCreated {
                account_id: Some(account_id.into()),
                journal_id: Some(journal_id.into()),
                name: name.to_string(),
                account_type: account_type as i32,
            }),
            JournalDomainEvent::AccountRenamed {
                account_id,
                journal_id,
                new_name,
            } => JournalDomainEventType::AccountRenamed(ProtoAccountRenamed {
                account_id: Some(account_id.into()),
                journal_id: Some(journal_id.into()),
                new_name: new_name.to_string(),
            }),
            JournalDomainEvent::AccountDeleted {
                account_id,
                journal_id,
            } => JournalDomainEventType::AccountDeleted(ProtoAccountDeleted {
                account_id: Some(account_id.into()),
                journal_id: Some(journal_id.into()),
            }),
            JournalDomainEvent::FundCreated {
                fund_id,
                journal_id,
                fund_name,
            } => JournalDomainEventType::FundCreated(ProtoFundCreated {
                fund_id: Some(fund_id.into()),
                journal_id: Some(journal_id.into()),
                fund_name: fund_name.to_string(),
            }),
            JournalDomainEvent::TransactionCreated {
                transaction_id,
                journal_id,
                period,
                entries,
                memo,
            } => JournalDomainEventType::TransactionCreated(ProtoTransactionCreated {
                transaction_id: Some(transaction_id.into()),
                journal_id: Some(journal_id.into()),
                financial_period: period as i32,
                memo: memo.map(|memo| memo.into()),
                entries: Some(entries.into()),
            }),
            JournalDomainEvent::FileUploaded {
                file_id,
                journal_id,
                hash,
                file_name,
            } => JournalDomainEventType::FileUploaded(ProtoFileUploaded {
                file_id: Some(file_id.into()),
                journal_id: Some(journal_id.into()),
                hash: hash.into(),
                file_name,
            }),
            JournalDomainEvent::ActivityCreated {
                activity_id,
                journal_id,
                activity_name,
                activity_kind,
            } => JournalDomainEventType::ActivityCreated(ProtoActivityCreated {
                activity_id: Some(activity_id.into()),
                journal_id: Some(journal_id.into()),
                activity_name: activity_name.to_string(),
                activity_kind: activity_kind as i32,
            }),
            JournalDomainEvent::FileDeleted {
                file_id,
                journal_id,
            } => JournalDomainEventType::FileDeleted(ProtoFileDeleted {
                file_id: Some(file_id.into()),
                journal_id: Some(journal_id.into()),
            }),
            JournalDomainEvent::DonorCreated {
                donor_id,
                journal_id,
                name,
            } => JournalDomainEventType::DonorCreated(ProtoDonorCreated {
                donor_id: Some(donor_id.into()),
                journal_id: Some(journal_id.into()),
                name: name.to_string(),
            }),
            JournalDomainEvent::DonationCreated {
                donation_id,
                journal_id,
                deposit_account_id,
                entries,
                donation_total,
            } => JournalDomainEventType::DonationCreated(ProtoDonationCreated {
                donation_id: Some(donation_id.into()),
                journal_id: Some(journal_id.into()),
                deposit_account_id: Some(deposit_account_id.into()),
                entries: Some(entries.into()),
                donation_total,
            }),
        };

        ProtoJournalDomainEvent {
            journal_domain_event_type: Some(event),
        }
    }
}

impl TryFrom<ProtoJournalDomainEvent> for JournalDomainEvent {
    type Error = DecodeError;

    fn try_from(event: ProtoJournalDomainEvent) -> Result<Self, Self::Error> {
        let event = match event.journal_domain_event_type.ok_or(FieldRequired)? {
            JournalDomainEventType::JournalCreated(ev) => JournalDomainEvent::JournalCreated {
                journal_id: ev.journal_id.try_into()?,
                owner: ev.owner_id.try_into()?,
                name: Name::try_new(ev.name)?,
            },
            JournalDomainEventType::JournalDeleted(ev) => JournalDomainEvent::JournalDeleted {
                journal_id: ev.journal_id.try_into()?,
            },
            JournalDomainEventType::MemberAdded(ev) => JournalDomainEvent::MemberAdded {
                journal_id: ev.journal_id.try_into()?,
                user_id: ev.user_id.try_into()?,
                permissions: Permissions::from_bits(ev.permissions)
                    .ok_or(DecodeError::PermissionDecode(ev.permissions))?,
            },
            JournalDomainEventType::MemberPermissionsUpdated(ev) => {
                JournalDomainEvent::MemberPermissionsUpdated {
                    journal_id: ev.journal_id.try_into()?,
                    user_id: ev.user_id.try_into()?,
                    permissions: Permissions::from_bits(ev.permissions)
                        .ok_or(DecodeError::PermissionDecode(ev.permissions))?,
                }
            }
            JournalDomainEventType::MemberRemoved(ev) => JournalDomainEvent::MemberRemoved {
                journal_id: ev.journal_id.try_into()?,
                user_id: ev.user_id.try_into()?,
            },
            JournalDomainEventType::AccountCreated(ev) => JournalDomainEvent::AccountCreated {
                account_id: ev.account_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                name: Name::try_new(ev.name)?,
                account_type: (ev.account_type as i8).try_into()?,
            },
            JournalDomainEventType::AccountRenamed(ev) => JournalDomainEvent::AccountRenamed {
                account_id: ev.account_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                new_name: Name::try_new(ev.new_name)?,
            },
            JournalDomainEventType::AccountDeleted(ev) => JournalDomainEvent::AccountDeleted {
                account_id: ev.account_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
            },
            JournalDomainEventType::FundCreated(ev) => JournalDomainEvent::FundCreated {
                fund_id: ev.fund_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                fund_name: Name::try_new(ev.fund_name)?,
            },
            JournalDomainEventType::TransactionCreated(ev) => {
                JournalDomainEvent::TransactionCreated {
                    transaction_id: ev.transaction_id.try_into()?,
                    journal_id: ev.journal_id.try_into()?,
                    period: (ev.financial_period as i8).try_into()?,
                    entries: ev.entries.try_into()?,
                    memo: ev.memo.map(|m| m.try_into()).transpose()?,
                }
            }
            JournalDomainEventType::FileUploaded(ev) => JournalDomainEvent::FileUploaded {
                file_id: ev.file_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                hash: ev.hash.try_into().map_err(|_| DecodeError::Deserialize)?,
                file_name: ev.file_name,
            },
            JournalDomainEventType::ActivityCreated(ev) => JournalDomainEvent::ActivityCreated {
                activity_id: ev.activity_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                activity_name: Name::try_new(ev.activity_name)?,
                activity_kind: ActivityKind::try_from(ev.activity_kind as i8)?,
            },
            JournalDomainEventType::FileDeleted(ev) => JournalDomainEvent::FileDeleted {
                file_id: ev.file_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
            },
            JournalDomainEventType::DonorCreated(ev) => JournalDomainEvent::DonorCreated {
                donor_id: ev.donor_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                name: Name::try_new(ev.name)?,
            },
            JournalDomainEventType::DonationCreated(ev) => JournalDomainEvent::DonationCreated {
                donation_id: ev.donation_id.try_into()?,
                journal_id: ev.journal_id.try_into()?,
                deposit_account_id: ev.deposit_account_id.try_into()?,
                entries: ev.entries.try_into()?,
                donation_total: ev.donation_total,
            },
        };

        Ok(event)
    }
}
