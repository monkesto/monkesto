use super::*;
use crate::authn::UserId;
use crate::authority::{Actor, Authority};
use crate::journal::JournalDomainEvent::*;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::file::{FileId, ObjectStore};
use crate::journal::service::stream_lock::{
    append_creation_event_in_tx, batch_append_creation_event_in_tx,
};
use crate::journal::transaction::TransactionId;
use crate::journal::transaction::entry::{EntryId, EntrySide};
use crate::name::Name;
use crate::time::Timestamp;
use sqlx::PgTransaction;
use tokio::time::timeout;

async fn append(
    tx: &mut PgTransaction<'_>,
    event: JournalDomainEvent,
) -> Result<EventId, crate::journal::error::JournalError> {
    append_creation_event_in_tx(
        tx,
        event,
        None,
        Authority::Direct(Actor::System),
        Timestamp(chrono::DateTime::UNIX_EPOCH),
        None,
    )
    .await
}

fn journal_created(journal_id: JournalId) -> JournalDomainEvent {
    JournalCreated {
        journal_id,
        owner: UserId::new(),
        name: Name::default(),
    }
}

fn account_created(journal_id: JournalId, account_id: AccountId) -> JournalDomainEvent {
    AccountCreated {
        journal_id,
        account_id,
        name: Name::default(),
        account_type: AccountType::Asset,
    }
}

fn entry_created(journal_id: JournalId, account_id: AccountId, amount: u64) -> JournalDomainEvent {
    EntryCreated {
        journal_id,
        entry_id: EntryId::new(),
        transaction_id: TransactionId::new(),
        amount,
        entry_side: EntrySide::Credit,
        entry_kind: EntryKind::Account { account_id },
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test server with database creation privileges"]
async fn claims_entire_journal_across_batch_boundary(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    install_sql(&pool).await?;
    let first_journal = JournalId::new();
    let second_journal = JournalId::new();
    let account_id = AccountId::new();
    let mut writer = pool.begin().await?;
    append(&mut writer, journal_created(first_journal)).await?;
    batch_append_creation_event_in_tx(
        &mut writer,
        (0..BATCH_SIZE - 2).map(|_| {
            (
                FileUploaded {
                    journal_id: first_journal,
                    file_id: FileId::new(),
                    hash: [0xff; 16],
                    file_name: "receipt".into(),
                },
                None,
                Authority::Direct(Actor::System),
                Timestamp(chrono::DateTime::UNIX_EPOCH),
                None,
            )
        }),
    )
    .await?;
    let creation_id = append(&mut writer, account_created(first_journal, account_id)).await?;
    let entry_id = append(&mut writer, entry_created(first_journal, account_id, 125)).await?;
    writer.commit().await?;

    let mut writer = pool.begin().await?;
    let second_id = append(&mut writer, journal_created(second_journal)).await?;
    writer.commit().await?;
    install_sql(&pool).await?;

    let mut listener = PgListener::connect_with(&pool).await?;
    listener.listen(APPLICATION_CHANNEL).await?;
    let (projection_applications, _) = broadcast::channel(128);
    let forwarder = tokio::spawn(forward_applications(
        listener,
        projection_applications.clone(),
    ));

    let service = JournalService {
        pool: pool.clone(),
        object_store: ObjectStore::Local {
            storage_directory: std::env::temp_dir(),
        },
        projection_applications,
        jewel_imports: Default::default(),
    };

    let mut first_worker = pool.begin().await?;
    sqlx::query!(
        "SELECT journal_id FROM journal_projection_checkpoint WHERE journal_id = $1 FOR UPDATE",
        first_journal as JournalId,
    )
    .fetch_one(&mut *first_worker)
    .await?;
    assert!(project_incoming_events(&pool).await?);
    timeout(Duration::from_secs(2), service.wait_for(second_id)).await?;
    assert!(
        timeout(Duration::from_millis(100), service.wait_for(creation_id))
            .await
            .is_err()
    );
    assert!(!project_incoming_events(&pool).await?);
    first_worker.commit().await?;

    assert!(project_incoming_events(&pool).await?);
    timeout(Duration::from_secs(2), service.wait_for(creation_id)).await?;
    assert!(
        timeout(Duration::from_millis(100), service.wait_for(entry_id))
            .await
            .is_err()
    );

    let (wait, projected) = tokio::join!(
        timeout(Duration::from_millis(750), service.wait_for(entry_id)),
        async {
            sleep(Duration::from_millis(50)).await;
            project_incoming_events(&pool).await
        },
    );
    wait?;
    assert!(projected?);

    // Retry the same event through the worker and verify the balance is applied once.
    sqlx::query!(
        "UPDATE journal_projection_checkpoint SET last_processed_event_id = $1 WHERE journal_id = $2",
        creation_id,
        first_journal as JournalId,
    )
    .execute(&pool)
    .await?;
    assert!(project_incoming_events(&pool).await?);

    let balance = sqlx::query_scalar!(
        "SELECT balance FROM accounts WHERE journal_id = $1",
        first_journal as JournalId,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(balance, 125);
    let stored_kind = sqlx::query_scalar!(
        r#"SELECT entry_kind AS "entry_kind: EntryKind" FROM entries WHERE journal_id = $1"#,
        first_journal as JournalId,
    )
    .fetch_one(&pool)
    .await?;
    assert!(matches!(stored_kind, EntryKind::Account { account_id: id } if id == account_id));
    forwarder.abort();
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test server with database creation privileges"]
async fn journal_append_lock_prevents_late_lower_ids(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    install_sql(&pool).await?;
    let journal_id = JournalId::new();
    let other_journal = JournalId::new();
    let account_id = AccountId::new();

    let mut seed = pool.begin().await?;
    append(&mut seed, journal_created(journal_id)).await?;
    seed.commit().await?;
    assert!(project_incoming_events(&pool).await?);

    let mut slow = pool.begin().await?;
    let creation_id = append(&mut slow, account_created(journal_id, account_id)).await?;

    let mut same_journal = pool.begin().await?;
    sqlx::query!("SET LOCAL lock_timeout = '100ms'")
        .execute(&mut *same_journal)
        .await?;
    let err = append(&mut same_journal, entry_created(journal_id, account_id, 90))
        .await
        .expect_err("appending to the same journal must wait");
    assert!(
        matches!(err, crate::journal::error::JournalError::Sqlx { source, .. }
        if source.as_database_error().and_then(|db| db.code()).as_deref() == Some("55P03"))
    );
    same_journal.rollback().await?;

    let mut unrelated = pool.begin().await?;
    let unrelated_id = append(&mut unrelated, journal_created(other_journal)).await?;
    unrelated.commit().await?;
    assert!(project_incoming_events(&pool).await?);
    assert_eq!(checkpoint(&pool, other_journal).await?, unrelated_id);
    assert!(checkpoint(&pool, journal_id).await? < creation_id);

    slow.commit().await?;
    let mut writer = pool.begin().await?;
    let entry_id = append(&mut writer, entry_created(journal_id, account_id, 90)).await?;
    writer.commit().await?;
    assert!(creation_id < unrelated_id && unrelated_id < entry_id);
    assert!(project_incoming_events(&pool).await?);
    assert_eq!(checkpoint(&pool, journal_id).await?, entry_id);
    let balance = sqlx::query_scalar!(
        "SELECT balance FROM accounts WHERE journal_id = $1",
        journal_id as JournalId,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(balance, 90);
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a PostgreSQL test server with database creation privileges"]
async fn missing_balance_target_rolls_back_entire_batch(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    install_sql(&pool).await?;
    let journal_id = JournalId::new();
    let other_journal = JournalId::new();
    let account_id = AccountId::new();
    let mut writer = pool.begin().await?;
    append(&mut writer, journal_created(journal_id)).await?;
    append(&mut writer, account_created(journal_id, account_id)).await?;
    writer.commit().await?;
    assert!(project_incoming_events(&pool).await?);

    let mut writer = pool.begin().await?;
    let valid_id = append(&mut writer, entry_created(journal_id, account_id, 32)).await?;
    append(&mut writer, entry_created(journal_id, AccountId::new(), 10)).await?;
    writer.commit().await?;

    assert!(project_incoming_events(&pool).await?);
    assert!(checkpoint(&pool, journal_id).await? < valid_id);
    assert!(!project_incoming_events(&pool).await?);
    let balance = sqlx::query_scalar!(
        "SELECT balance FROM accounts WHERE journal_id = $1",
        journal_id as JournalId,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(balance, 0);
    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM entries WHERE journal_id = $1"#,
        journal_id as JournalId,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(count, 0);

    let mut writer = pool.begin().await?;
    let unrelated_id = append(&mut writer, journal_created(other_journal)).await?;
    writer.commit().await?;
    assert!(project_incoming_events(&pool).await?);
    assert_eq!(checkpoint(&pool, other_journal).await?, unrelated_id);
    Ok(())
}

async fn checkpoint(pool: &PgPool, journal_id: JournalId) -> Result<EventId, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT last_processed_event_id FROM journal_projection_checkpoint WHERE journal_id = $1",
        journal_id as JournalId,
    )
    .fetch_one(pool)
    .await
}
