use crate::id;
use crate::id::Ident;
use crate::journal::account::AccountId;
use crate::journal::activity::ActivityId;
use crate::journal::fund::FundId;
use axum_test::expect_json::__private::serde_trampoline::{Deserialize, Serialize};
use prost::Message;
use proto::transaction_entry::ProtoEntryKind;
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
