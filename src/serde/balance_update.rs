use crate::journal::transaction::{BalanceUpdate, EntryType, TransactionEntries};
use crate::serde::error::ProtoError;
use crate::serde::error::ProtoError::FieldRequired;
use proto::balance_update::proto_balance_update::{ProtoEntryType, proto_entry_type};
use proto::balance_update::{ProtoBalanceUpdate, RepeatedBalanceUpdates};

impl TryFrom<RepeatedBalanceUpdates> for TransactionEntries {
    type Error = ProtoError;

    fn try_from(value: RepeatedBalanceUpdates) -> Result<Self, Self::Error> {
        let mut translated_updates = Vec::new();

        for entry in value.updates {
            translated_updates.push(BalanceUpdate {
                account_id: entry.account_id.ok_or(FieldRequired)?.try_into()?,
                amount: entry.amount,
                entry_type: match entry
                    .entry_type
                    .ok_or(FieldRequired)?
                    .entry_type
                    .ok_or(FieldRequired)?
                {
                    proto_entry_type::EntryType::Credit(_) => EntryType::Credit,
                    proto_entry_type::EntryType::Debit(_) => EntryType::Debit,
                },
            })
        }

        Ok(TransactionEntries(translated_updates))
    }
}

impl From<TransactionEntries> for RepeatedBalanceUpdates {
    fn from(updates: TransactionEntries) -> Self {
        RepeatedBalanceUpdates {
            updates: updates
                .0
                .iter()
                .map(|u| ProtoBalanceUpdate {
                    account_id: Some(u.account_id.into()),
                    amount: u.amount,
                    entry_type: Some(match u.entry_type {
                        EntryType::Credit => ProtoEntryType {
                            entry_type: Some(proto_entry_type::EntryType::Credit(())),
                        },
                        EntryType::Debit => ProtoEntryType {
                            entry_type: Some(proto_entry_type::EntryType::Debit(())),
                        },
                    }),
                })
                .collect(),
        }
    }
}
