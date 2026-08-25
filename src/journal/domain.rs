use crate::authn::UserId;
use crate::authority::Authority;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::activity::{ActivityId, ActivityType};
use crate::journal::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::file::FileId;
use crate::journal::fund::FundId;
use crate::journal::store::JournalEventStore;
use crate::journal::transaction::{FinancialPeriod, TransactionEntryIds, TransactionId};
use crate::journal::{JournalId, JournalService, Permissions};
use crate::name::Name;
use crate::shutdown;
use crate::time_provider::Timestamp;
use axum_login::tracing;
use disintegrate::Event;
use disintegrate_postgres::{
    PgEventListener, PgEventListenerConfig, PgEventListenerError, RetryAction,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Event, Serialize, Deserialize)]
#[stream(JournalEvent, [JournalCreated, JournalDeleted])]
#[stream(MemberEvent, [MemberAdded, MemberPermissionsUpdated, MemberRemoved])]
#[stream(AccountEvent, [AccountCreated, AccountRenamed, AccountDeleted])]
#[stream(FundEvent, [FundCreated])]
#[stream(ActivityEvent, [ActivityCreated])]
// unified stream for account, fund, and activity creation and deletion events
#[stream(AFTEvent, [AccountCreated, AccountDeleted, FundCreated, ActivityCreated])]
#[stream(EntryEvent, [EntryCreated])]
#[stream(TransactionEvent, [TransactionCreated, TransactionDeleted])]
#[stream(FileEvent, [FileUploaded])]
pub enum JournalDomainEvent {
    JournalCreated {
        #[id]
        journal_id: JournalId,
        owner: UserId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    },
    JournalDeleted {
        #[id]
        journal_id: JournalId,
        authority: Authority,
        timestamp: Timestamp,
    },
    MemberAdded {
        #[id]
        journal_id: JournalId,
        #[id]
        user_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    },
    MemberPermissionsUpdated {
        #[id]
        journal_id: JournalId,
        #[id]
        user_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    },
    MemberRemoved {
        #[id]
        journal_id: JournalId,
        #[id]
        user_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    },
    AccountCreated {
        #[id]
        account_id: AccountId,
        #[id]
        journal_id: JournalId,
        name: Name,
        account_type: AccountType,
        authority: Authority,
        timestamp: Timestamp,
    },
    AccountRenamed {
        #[id]
        account_id: AccountId,
        #[id]
        journal_id: JournalId,
        new_name: Name,
        authority: Authority,
        timestamp: Timestamp,
    },
    AccountDeleted {
        #[id]
        account_id: AccountId,
        #[id]
        journal_id: JournalId,
        authority: Authority,
        timestamp: Timestamp,
    },

    FundCreated {
        #[id]
        fund_id: FundId,
        #[id]
        journal_id: JournalId,
        fund_name: Name,
        authority: Authority,
        timestamp: Timestamp,
    },

    ActivityCreated {
        #[id]
        activity_id: ActivityId,
        #[id]
        journal_id: JournalId,
        activity_name: Name,
        activity_type: ActivityType,
        authority: Authority,
        timestamp: Timestamp,
    },

    EntryCreated {
        #[id]
        entry_id: EntryId,
        #[id]
        journal_id: JournalId,
        amount: u64,
        entry_side: EntrySide,
        entry_kind: EntryKind,
        authority: Authority,
        timestamp: Timestamp,
    },
    TransactionCreated {
        #[id]
        transaction_id: TransactionId,
        #[id]
        journal_id: JournalId,
        entries: TransactionEntryIds,
        financial_period: FinancialPeriod,
        authority: Authority,
        timestamp: Timestamp,
    },
    FileUploaded {
        #[id]
        file_id: FileId,
        #[id]
        journal_id: JournalId,
        hash: [u8; 16],
        file_name: String,
        authority: Authority,
        timestamp: Timestamp,
    },
}

pub(crate) async fn event_listener(event_store: JournalEventStore, service: JournalService) {
    PgEventListener::builder(event_store.event_store)
        .register_listener(
            service,
            PgEventListenerConfig::poller(Duration::from_secs(60))
                .with_notifier()
                .fetch_size(100)
                .with_retry(handle_event_listener_retry),
        )
        .start_with_shutdown(shutdown())
        .await
        .expect("event listener failed");
}

fn handle_event_listener_retry(
    error: PgEventListenerError<sqlx::Error>,
    _attempts: usize,
) -> RetryAction {
    tracing::error!(?error, "read model listener failed");
    RetryAction::Abort
}
