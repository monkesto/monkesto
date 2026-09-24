use crate::error::DecodeError;
use crate::journal::transaction::memo::memo_error::MemoError;
use crate::proto::journal::transaction::memo::memo::ProtoMemo;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::Display;

pub mod memo_error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Memo(String);

impl Memo {
    pub fn try_new(n: String) -> Result<Memo, MemoError> {
        if n.len() > 100 {
            Err(MemoError(n))
        } else {
            Ok(Memo(n))
        }
    }
}

impl Display for Memo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Memo {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Default for Memo {
    fn default() -> Self {
        Self::try_new("example memo".to_string()).expect("valid default memo")
    }
}

impl Type<Postgres> for Memo {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&str as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for Memo {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.as_ref(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for Memo {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let str = <String as Decode<Postgres>>::decode(value)?;
        Ok(Memo::try_new(str)?)
    }
}

impl From<Memo> for ProtoMemo {
    fn from(value: Memo) -> Self {
        ProtoMemo { memo: value.0 }
    }
}

impl TryFrom<ProtoMemo> for Memo {
    type Error = DecodeError;
    fn try_from(value: ProtoMemo) -> Result<Self, Self::Error> {
        Ok(Memo::try_new(value.memo)?)
    }
}
