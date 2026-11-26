use crate::journal::donation::CheckNum;
use crate::journal::donation::donor::DonorId;
use crate::journal::fund::FundId;
use crate::proto::DecodeError;
use crate::proto::DecodeError::FieldRequired;
use crate::proto::journal::donation::donation_entry::donation_entry::proto_donation_instrument::{
    ProtoCheckDonationInstrument, ProtoDonationInstrumentType, ProtoMixedDonationInstrument,
};
use crate::proto::journal::donation::donation_entry::donation_entry::{
    ProtoDonationEntries, ProtoDonationEntry, ProtoDonationInstrument,
};
use prost::Message;
use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::ops::Deref;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DonationInstrument {
    Cash {
        amount: u64,
    },
    Check {
        amount: u64,
        check_num: Option<CheckNum>,
    },
    Mixed {
        cash_amount: u64,
        check_amount: u64,
        check_num: Option<CheckNum>,
    },
    Online {
        amount: u64,
    },
    Unknown {
        amount: u64,
    },
}

impl From<DonationInstrument> for ProtoDonationInstrument {
    fn from(instrument: DonationInstrument) -> Self {
        let instrument = match instrument {
            DonationInstrument::Cash { amount } => ProtoDonationInstrumentType::Cash(amount),
            DonationInstrument::Check { amount, check_num } => {
                ProtoDonationInstrumentType::Check(ProtoCheckDonationInstrument {
                    amount,
                    check_num: check_num.map(|c| c.to_string()),
                })
            }
            DonationInstrument::Mixed {
                cash_amount,
                check_amount,
                check_num,
            } => ProtoDonationInstrumentType::Mixed(ProtoMixedDonationInstrument {
                cash_amount,
                check_amount,
                check_num: check_num.map(|c| c.to_string()),
            }),
            DonationInstrument::Online { amount } => ProtoDonationInstrumentType::Online(amount),
            DonationInstrument::Unknown { amount } => ProtoDonationInstrumentType::Unknown(amount),
        };

        Self {
            proto_donation_instrument_type: Some(instrument),
        }
    }
}

impl TryFrom<ProtoDonationInstrument> for DonationInstrument {
    type Error = DecodeError;

    fn try_from(instrument: ProtoDonationInstrument) -> Result<Self, Self::Error> {
        Ok(
            match instrument
                .proto_donation_instrument_type
                .ok_or(FieldRequired)?
            {
                ProtoDonationInstrumentType::Cash(amount) => DonationInstrument::Cash { amount },
                ProtoDonationInstrumentType::Check(check_instrument) => DonationInstrument::Check {
                    amount: check_instrument.amount,
                    check_num: check_instrument
                        .check_num
                        .map(CheckNum::try_new)
                        .transpose()?,
                },
                ProtoDonationInstrumentType::Mixed(mixed_instrument) => DonationInstrument::Mixed {
                    cash_amount: mixed_instrument.cash_amount,
                    check_amount: mixed_instrument.check_amount,
                    check_num: mixed_instrument
                        .check_num
                        .map(CheckNum::try_new)
                        .transpose()?,
                },
                ProtoDonationInstrumentType::Online(amount) => {
                    DonationInstrument::Online { amount }
                }
                ProtoDonationInstrumentType::Unknown(amount) => {
                    DonationInstrument::Unknown { amount }
                }
            },
        )
    }
}

impl TryFrom<Option<ProtoDonationInstrument>> for DonationInstrument {
    type Error = DecodeError;
    fn try_from(instrument: Option<ProtoDonationInstrument>) -> Result<Self, Self::Error> {
        instrument.ok_or(FieldRequired)?.try_into()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DonationEntry {
    pub id: i64,
    pub fund_id: FundId,
    pub donor_id: DonorId,
    pub instrument: DonationInstrument,
}

impl DonationEntry {
    pub fn amount(&self) -> u64 {
        match self.instrument {
            DonationInstrument::Cash { amount } => amount,
            DonationInstrument::Check { amount, .. } => amount,
            DonationInstrument::Mixed {
                cash_amount,
                check_amount,
                ..
            } => cash_amount + check_amount,
            DonationInstrument::Online { amount, .. } => amount,
            DonationInstrument::Unknown { amount } => amount,
        }
    }
}

impl From<DonationEntry> for ProtoDonationEntry {
    fn from(entry: DonationEntry) -> Self {
        ProtoDonationEntry {
            id: entry.id,
            fund_id: Some(entry.fund_id.into()),
            donor_id: Some(entry.donor_id.into()),
            instrument: Some(entry.instrument.into()),
        }
    }
}

impl TryFrom<ProtoDonationEntry> for DonationEntry {
    type Error = DecodeError;
    fn try_from(entry: ProtoDonationEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            id: entry.id,
            fund_id: entry.fund_id.try_into()?,
            donor_id: entry.donor_id.try_into()?,
            instrument: entry.instrument.try_into()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DonationEntries(pub Vec<DonationEntry>);

impl Deref for DonationEntries {
    type Target = Vec<DonationEntry>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<DonationEntries> for ProtoDonationEntries {
    fn from(entries: DonationEntries) -> Self {
        Self {
            entries: entries.0.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<ProtoDonationEntries> for DonationEntries {
    type Error = DecodeError;
    fn try_from(proto_entries: ProtoDonationEntries) -> Result<Self, Self::Error> {
        let mut entries = Vec::with_capacity(proto_entries.entries.len());

        for entry in proto_entries.entries {
            entries.push(entry.try_into()?);
        }

        Ok(Self(entries))
    }
}

impl TryFrom<Option<ProtoDonationEntries>> for DonationEntries {
    type Error = DecodeError;
    fn try_from(proto_entries: Option<ProtoDonationEntries>) -> Result<Self, Self::Error> {
        proto_entries.ok_or(FieldRequired)?.try_into()
    }
}

impl Type<Postgres> for DonationEntries {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&[u8] as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for DonationEntries {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <Vec<u8> as Encode<Postgres>>::encode(
            ProtoDonationEntries::from(self.clone()).encode_to_vec(),
            buf,
        )
    }
}

impl<'r> Decode<'r, Postgres> for DonationEntries {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <&[u8] as Decode<Postgres>>::decode(value)?;
        Ok(ProtoDonationEntries::decode(bytes)?.try_into()?)
    }
}
