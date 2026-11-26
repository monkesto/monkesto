use crate::journal::account::AccountId;
use crate::journal::activity::ActivityId;
use crate::journal::activity::scope::ActivityScope;
use crate::journal::fund::FundId;
use prost::Message;
use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::Display;
use std::ops::Deref;
use thiserror::Error;

#[repr(i8)]
#[derive(Copy, Clone, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum EntrySide {
    #[default]
    Debit = 1,
    Credit,
}

#[derive(Debug, Error, PartialEq)]
#[error("{0}")]
pub struct EntrySideFromIntError(pub i8);

impl TryFrom<i8> for EntrySide {
    type Error = EntrySideFromIntError;

    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            x if x == EntrySide::Debit as i8 => Ok(EntrySide::Debit),
            x if x == EntrySide::Credit as i8 => Ok(EntrySide::Credit),
            _ => Err(EntrySideFromIntError(value)),
        }
    }
}

impl Type<Postgres> for EntrySide {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <i16 as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for EntrySide {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <i16 as Encode<Postgres>>::encode(*self as i16, buf)
    }
}

impl<'r> Decode<'r, Postgres> for EntrySide {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let int = <i16 as Decode<Postgres>>::decode(value)?;
        Ok(EntrySide::try_from(int as i8)?)
    }
}

impl Display for EntrySide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            EntrySide::Debit => "Dr",
            EntrySide::Credit => "Cr",
        };
        write!(f, "{}", str)
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Account {
        account_id: AccountId,
    },
    Activity {
        activity_id: ActivityId,
        fund_id: FundId,
        scope: ActivityScope,
    },
}

impl Default for EntryKind {
    fn default() -> Self {
        EntryKind::Account {
            account_id: AccountId::default(),
        }
    }
}

impl Type<Postgres> for EntryKind {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&[u8] as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for EntryKind {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <Vec<u8> as Encode<Postgres>>::encode(ProtoEntryKind::from(*self).encode_to_vec(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for EntryKind {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <Vec<u8> as Decode<Postgres>>::decode(value)?;
        Ok(ProtoEntryKind::decode(bytes.as_slice())?.try_into()?)
    }
}

use crate::proto::DecodeError;
use crate::proto::DecodeError::FieldRequired;
use crate::proto::journal::transaction::entry::entry::proto_entry_kind::{
    ProtoActivityEntryKind, ProtoEntryKindVariant,
};
use crate::proto::journal::transaction::entry::entry::{
    ProtoEntryKind, ProtoTransactionEntries, ProtoTransactionEntry,
};

impl From<EntryKind> for ProtoEntryKind {
    fn from(entry_kind: EntryKind) -> Self {
        let variant = match entry_kind {
            EntryKind::Account { account_id } => ProtoEntryKindVariant::Account(account_id.into()),
            EntryKind::Activity {
                activity_id,
                fund_id,
                scope,
            } => ProtoEntryKindVariant::Activity(ProtoActivityEntryKind {
                activity_id: Some(activity_id.into()),
                fund_id: Some(fund_id.into()),
                scope: scope as i32,
            }),
        };

        ProtoEntryKind {
            proto_entry_kind_variant: Some(variant),
        }
    }
}

impl TryFrom<ProtoEntryKind> for EntryKind {
    type Error = DecodeError;
    fn try_from(proto_entry_kind: ProtoEntryKind) -> Result<Self, Self::Error> {
        let kind = match proto_entry_kind
            .proto_entry_kind_variant
            .ok_or(FieldRequired)?
        {
            ProtoEntryKindVariant::Account(id) => EntryKind::Account {
                account_id: id.try_into()?,
            },
            ProtoEntryKindVariant::Activity(ek) => EntryKind::Activity {
                activity_id: ek.activity_id.try_into()?,
                fund_id: ek.fund_id.try_into()?,
                scope: (ek.scope as i8).try_into()?,
            },
        };

        Ok(kind)
    }
}

impl TryFrom<Option<ProtoEntryKind>> for EntryKind {
    type Error = DecodeError;
    fn try_from(proto_entry_kind: Option<ProtoEntryKind>) -> Result<Self, Self::Error> {
        proto_entry_kind.ok_or(FieldRequired)?.try_into()
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TransactionEntry {
    pub entry_id: i64,
    pub amount: u64,
    pub entry_side: EntrySide,
    pub entry_kind: EntryKind,
}

impl From<TransactionEntry> for ProtoTransactionEntry {
    fn from(entry: TransactionEntry) -> Self {
        ProtoTransactionEntry {
            entry_id: entry.entry_id,
            amount: entry.amount,
            entry_side: entry.entry_side as i32,
            entry_kind: Some(entry.entry_kind.into()),
        }
    }
}

impl TryFrom<ProtoTransactionEntry> for TransactionEntry {
    type Error = DecodeError;
    fn try_from(entry: ProtoTransactionEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            entry_id: entry.entry_id,
            amount: entry.amount,
            entry_side: EntrySide::try_from(entry.entry_side as i8)?,
            entry_kind: entry.entry_kind.try_into()?,
        })
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TransactionEntries(pub Vec<TransactionEntry>);

impl From<TransactionEntries> for ProtoTransactionEntries {
    fn from(entries: TransactionEntries) -> Self {
        ProtoTransactionEntries {
            entries: entries.0.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<ProtoTransactionEntries> for TransactionEntries {
    type Error = DecodeError;
    fn try_from(proto_entries: ProtoTransactionEntries) -> Result<Self, Self::Error> {
        let mut entries = Vec::with_capacity(proto_entries.entries.len());

        for entry in proto_entries.entries {
            entries.push(TransactionEntry::try_from(entry)?);
        }

        Ok(Self(entries))
    }
}

impl TryFrom<Option<ProtoTransactionEntries>> for TransactionEntries {
    type Error = DecodeError;
    fn try_from(proto_entries: Option<ProtoTransactionEntries>) -> Result<Self, Self::Error> {
        proto_entries.ok_or(FieldRequired)?.try_into()
    }
}

impl Deref for TransactionEntries {
    type Target = Vec<TransactionEntry>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Type<Postgres> for TransactionEntries {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&[u8] as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for TransactionEntries {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <Vec<u8> as Encode<Postgres>>::encode(
            ProtoTransactionEntries::from(self.clone()).encode_to_vec(),
            buf,
        )
    }
}

impl<'r> Decode<'r, Postgres> for TransactionEntries {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <&[u8] as Decode<Postgres>>::decode(value)?;
        Ok(ProtoTransactionEntries::decode(bytes)?.try_into()?)
    }
}
