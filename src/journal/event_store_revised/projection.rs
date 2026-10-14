use crate::journal::JournalDomainEvent;
use crate::journal::event_store_revised::EventId;
use crate::journal::service::handle_incoming_event;
use sqlx::postgres::{PgAdvisoryLock, PgListener};
use sqlx::{Either, PgPool};
use std::time::Duration;
use tokio::sync::watch;
use tokio::sync::watch::Sender;
use tokio::time::sleep;

#[derive(Clone)]
pub struct JournalProjection {
    pool: PgPool,
    last_processed_event: Sender<EventId>,
}

impl JournalProjection {
    pub fn new(pool: PgPool) -> Self {
        let (tx, rx) = watch::channel(0);

        Box::leak(Box::new(rx));

        Self {
            pool,
            last_processed_event: tx,
        }
    }

    pub async fn run(&self) -> Result<!, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        let mut lock_conn = self.pool.acquire().await?;

        let lock = PgAdvisoryLock::new("Journal Projection");

        loop {
            let lock_guard = match lock.try_acquire(&mut lock_conn).await? {
                Either::Left(guard) => Some(guard),
                Either::Right(_) => None,
            };

            if lock_guard.is_some() {
                let mut listener = PgListener::connect_with(&self.pool).await?;
                listener.listen("journal_events").await?;
                let mut last_processed_event = 0;

                loop {
                    let notification = listener.recv().await?;
                    let event_id: EventId = notification
                        .payload()
                        .parse()
                        .expect("Failed to parse event id");

                    // notifications are queued, so we might get a notification for an event we already took processed
                    // the event also may have been processed by another node before we got the notification
                    if event_id <= last_processed_event {
                        continue;
                    }

                    loop {
                        let mut tx = self.pool.begin().await?;

                        let unprocessed_events = sqlx::query!(
                            r#"
                            SELECT event_id, payload as "payload: JournalDomainEvent" FROM journal_events WHERE event_id > (SELECT last_processed_event_id FROM journal_event_listener WHERE id = 'Journal Projection')
                            ORDER BY event_id
                            LIMIT 100
                            "#
                        )
                            .fetch_all(&mut *conn)
                            .await?;

                        let saturated = unprocessed_events.len() > 99;

                        for event in unprocessed_events {
                            handle_incoming_event(event.payload, &mut tx).await?;
                            last_processed_event = event.event_id;
                        }

                        sqlx::query!(
                        r#"
                            UPDATE journal_event_listener SET last_processed_event_id = $1 WHERE id = 'Journal Projection'
                            "#,
                        last_processed_event
                        ).execute(&mut *tx).await?;

                        // notify other nodes that we have handled the events
                        sqlx::query!(
                            r#"
                            SELECT pg_notify('journal_projection', $1)
                            "#,
                            last_processed_event.to_string()
                        )
                        .execute(&mut *tx)
                        .await?;

                        tx.commit().await?;

                        if !saturated {
                            break;
                        }
                    }
                }
            } else {
                // try to acquire the lock again in case the leader goes offline
                sleep(Duration::from_secs(30)).await;
            }
        }
    }

    pub async fn update_last_processed_event(&self) -> Result<!, sqlx::Error> {
        let mut listener = PgListener::connect_with(&self.pool).await?;
        listener.listen("journal_projection").await?;

        loop {
            let notification = listener.recv().await?;
            let event_id: EventId = notification
                .payload()
                .parse()
                .expect("Failed to parse event id");
            self.last_processed_event
                .send(event_id)
                .expect("receiver dropped");
        }
    }

    pub async fn wait_for(&self, event_id: EventId) {
        self.last_processed_event
            .subscribe()
            .wait_for(|id| *id >= event_id)
            .await
            .expect("sender dropped");
    }
}
