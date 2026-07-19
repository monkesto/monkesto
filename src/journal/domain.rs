use crate::authn::UserId;
use crate::authority::Authority;
use crate::journal::account::AccountId;
use crate::journal::store::JournalEventStore;
use crate::journal::transaction::{BalanceUpdate, TransactionId};
use crate::journal::{JournalId, JournalService, Permissions};
use crate::name::Name;
use crate::shutdown;
use crate::time_provider::Timestamp;
use axum_login::tracing;
use axum_test::expect_json::__private::serde_trampoline::{Deserialize, Serialize};
use disintegrate::Event;
use disintegrate_postgres::{
    PgEventListener, PgEventListenerConfig, PgEventListenerError, RetryAction,
};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Event, Serialize, Deserialize)]
#[stream(JournalEvent, [JournalCreated, JournalDeleted])]
#[stream(MemberEvent, [MemberAdded, MemberPermissionsUpdated, MemberRemoved])]
#[stream(AccountEvent, [AccountCreated, AccountRenamed, AccountDeleted])]
#[stream(TransactionEvent, [TransactionCreated, TransactionDeleted])]
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
        authority: Authority,
        timestamp: Timestamp,
    },
    AccountRenamed {
        #[id]
        account_id: AccountId,
        new_name: Name,
        authority: Authority,
        timestamp: Timestamp,
    },
    AccountDeleted {
        #[id]
        account_id: AccountId,
        authority: Authority,
        timestamp: Timestamp,
    },
    TransactionCreated {
        #[id]
        transaction_id: TransactionId,
        #[id]
        journal_id: JournalId,
        balance_updates: Vec<BalanceUpdate>,
        authority: Authority,
        timestamp: Timestamp,
    },
    TransactionDeleted {
        #[id]
        transaction_id: TransactionId,
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
