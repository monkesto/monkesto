use crate::error::DecodeError;
use crate::error::DecodeError::FieldRequired;
use crate::id;
use crate::id::Ident;
use crate::journal::account::AccountId;
use crate::journal::activity::ActivityId;
use crate::journal::fund::FundId;
use prost::Message;
use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::Display;
use thiserror::Error;

id!(EntryId, Ident::new16());

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
        transfer: bool,
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

use crate::journal::transaction::{TransactionEntries, TransactionEntry, TransactionEntryIds};
use crate::proto::journal::entry::entry::proto_entry_kind::{
    ProtoActivityEntryKind, ProtoEntryKindVariant,
};
use crate::proto::journal::entry::entry::{
    ProtoEntryKind, ProtoRepeatedTransactionEntries, ProtoRepeatedTransactionEntryIds,
    ProtoTransactionEntry,
};

impl From<EntryKind> for ProtoEntryKind {
    fn from(entry_kind: EntryKind) -> Self {
        let variant = match entry_kind {
            EntryKind::Account { account_id } => ProtoEntryKindVariant::Account(account_id.into()),
            EntryKind::Activity {
                activity_id,
                fund_id,
                transfer,
            } => ProtoEntryKindVariant::Activity(ProtoActivityEntryKind {
                activity_id: Some(activity_id.into()),
                fund_id: Some(fund_id.into()),
                transfer,
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
                transfer: ek.transfer,
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

impl From<TransactionEntry> for ProtoTransactionEntry {
    fn from(value: TransactionEntry) -> Self {
        ProtoTransactionEntry {
            amount: value.amount,
            entry_side: value.entry_side as i32,
            entry_kind: Some(ProtoEntryKind::from(value.entry_kind)),
        }
    }
}

impl TryFrom<ProtoTransactionEntry> for TransactionEntry {
    type Error = DecodeError;

    fn try_from(value: ProtoTransactionEntry) -> Result<Self, Self::Error> {
        let entry = TransactionEntry {
            amount: value.amount,
            entry_side: EntrySide::try_from(value.entry_side as i8)?,
            entry_kind: EntryKind::try_from(value.entry_kind)?,
        };

        Ok(entry)
    }
}

impl TryFrom<Option<ProtoTransactionEntry>> for TransactionEntry {
    type Error = DecodeError;

    fn try_from(value: Option<ProtoTransactionEntry>) -> Result<Self, Self::Error> {
        value.ok_or(FieldRequired)?.try_into()
    }
}

impl TryFrom<ProtoRepeatedTransactionEntries> for TransactionEntries {
    type Error = DecodeError;

    fn try_from(value: ProtoRepeatedTransactionEntries) -> Result<Self, Self::Error> {
        let mut entries = Vec::with_capacity(value.entries.len());

        for entry in value.entries {
            entries.push(entry.try_into()?);
        }

        Ok(Self(entries))
    }
}

impl From<TransactionEntries> for ProtoRepeatedTransactionEntries {
    fn from(entries: TransactionEntries) -> Self {
        Self {
            entries: entries.0.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<ProtoRepeatedTransactionEntryIds> for TransactionEntryIds {
    type Error = DecodeError;

    fn try_from(value: ProtoRepeatedTransactionEntryIds) -> Result<Self, Self::Error> {
        let mut entries = Vec::with_capacity(value.entries.len());

        for entry in value.entries {
            entries.push(entry.try_into()?);
        }

        Ok(Self(entries))
    }
}

impl TryFrom<Option<ProtoRepeatedTransactionEntryIds>> for TransactionEntryIds {
    type Error = DecodeError;

    fn try_from(value: Option<ProtoRepeatedTransactionEntryIds>) -> Result<Self, Self::Error> {
        value.ok_or(FieldRequired)?.try_into()
    }
}

impl From<TransactionEntryIds> for ProtoRepeatedTransactionEntryIds {
    fn from(value: TransactionEntryIds) -> Self {
        ProtoRepeatedTransactionEntryIds {
            entries: value.0.into_iter().map(Into::into).collect(),
        }
    }
}
