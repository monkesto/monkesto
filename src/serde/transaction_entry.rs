use crate::journal::entry::{EntryKind, EntrySide};
use crate::journal::transaction::{TransactionEntries, TransactionEntry, TransactionEntryIds};
use crate::serde::error::ProtoError;
use crate::serde::error::ProtoError::FieldRequired;
use proto::transaction_entry::proto_entry_kind::{ProtoActivityEntryKind, ProtoEntryKindVariant};
use proto::transaction_entry::{
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
    type Error = ProtoError;
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
    type Error = ProtoError;
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
    type Error = ProtoError;

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
    type Error = ProtoError;

    fn try_from(value: Option<ProtoTransactionEntry>) -> Result<Self, Self::Error> {
        value.ok_or(FieldRequired)?.try_into()
    }
}

impl TryFrom<ProtoRepeatedTransactionEntries> for TransactionEntries {
    type Error = ProtoError;

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
    type Error = ProtoError;

    fn try_from(value: ProtoRepeatedTransactionEntryIds) -> Result<Self, Self::Error> {
        let mut entries = Vec::with_capacity(value.entries.len());

        for entry in value.entries {
            entries.push(entry.try_into()?);
        }

        Ok(Self(entries))
    }
}

impl TryFrom<Option<ProtoRepeatedTransactionEntryIds>> for TransactionEntryIds {
    type Error = ProtoError;

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
