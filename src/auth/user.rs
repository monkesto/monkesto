use crate::auth::axum_login::AuthSession;
use crate::cuid::Cuid;
use crate::known_errors::KnownErrors;
use crate::known_errors::RedirectOnError;
use axum::response::Redirect;
use postcard::{from_bytes, to_allocvec};
use serde::{Deserialize, Serialize};
use sqlx::Decode;
use sqlx::Encode;
use sqlx::Type;
use sqlx::postgres::PgValueRef;
use sqlx::{PgPool, query_as};
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum UserEvent {
    CreatedJournal { id: Cuid },
    InvitedToJournal { journal_id: Cuid },
    AcceptedJournalInvite { journal_id: Cuid },
    DeclinedJournalInvite { journal_id: Cuid },
    RemovedFromJournal { id: Cuid },
    Deleted,
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum UserEventType {
    CreatedJournal = 1,
    InvitedToJournal = 2,
    AcceptedJournalInvite = 3,
    DeclinedJournalInvite = 4,
    RemovedFromJournal = 5,
    Deleted = 6,
}

impl UserEvent {
    pub fn get_type(&self) -> UserEventType {
        use UserEventType::*;
        match self {
            Self::CreatedJournal { .. } => CreatedJournal,
            Self::InvitedToJournal { .. } => InvitedToJournal,
            Self::AcceptedJournalInvite { .. } => AcceptedJournalInvite,
            Self::DeclinedJournalInvite { .. } => DeclinedJournalInvite,
            Self::RemovedFromJournal { .. } => RemovedFromJournal,
            Self::Deleted => Deleted,
        }
    }
    pub async fn push_db(&self, id: &Cuid, pool: &PgPool) -> Result<i64, KnownErrors> {
        let event_type = self.get_type();
        let payload: Vec<u8> = to_allocvec(self)?;

        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO user_events (
                user_id,
                event_type,
                payload
            )
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
        )
        .bind(id.as_bytes())
        .bind(event_type)
        .bind(payload)
        .fetch_one(pool)
        .await?;

        Ok(id)
    }
}

impl Type<sqlx::Postgres> for UserEvent {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for UserEvent {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let bytes: Vec<u8> = postcard::to_allocvec(self)?;
        <&[u8] as Encode<sqlx::Postgres>>::encode(&bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for UserEvent {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(postcard::from_bytes::<UserEvent>(bytes)?)
    }
}

#[derive(Default)]
pub struct UserState {
    pub pending_journal_invites: HashSet<Cuid>,
    pub accepted_journal_invites: HashSet<Cuid>,
    pub owned_journals: HashSet<Cuid>,
    pub deleted: bool,
}

impl UserState {
    pub async fn build(
        id: &Cuid,
        event_types: Vec<UserEventType>,
        pool: &PgPool,
    ) -> Result<Self, KnownErrors> {
        let user_events = query_as::<_, (Vec<u8>,)>(
            r#"
            SELECT payload FROM user_events
            WHERE user_id = $1 AND event_type = ANY($2)
            ORDER BY created_at ASC
            "#,
        )
        .bind(id.as_bytes())
        .bind(&event_types)
        .fetch_all(pool)
        .await?;

        let mut aggregate = Self::default();

        user_events
            .into_iter()
            .try_for_each(|(payload,)| -> Result<(), KnownErrors> {
                aggregate.apply(from_bytes::<UserEvent>(&payload)?);
                Ok(())
            })?;

        Ok(aggregate)
    }

    pub fn apply(&mut self, event: UserEvent) {
        match event {
            UserEvent::CreatedJournal { id } => _ = self.owned_journals.insert(id),
            UserEvent::InvitedToJournal { journal_id } => {
                _ = self.pending_journal_invites.insert(journal_id)
            }
            UserEvent::DeclinedJournalInvite { journal_id } => {
                _ = self.pending_journal_invites.remove(&journal_id)
            }
            UserEvent::AcceptedJournalInvite { journal_id } => {
                if self.pending_journal_invites.contains(&journal_id) {
                    _ = self.accepted_journal_invites.insert(journal_id);
                }
            }
            UserEvent::RemovedFromJournal { id } => _ = self.accepted_journal_invites.remove(&id),
            UserEvent::Deleted => self.deleted = true,
        }
    }
}

pub fn get_id(session: AuthSession) -> Result<Cuid, Redirect> {
    Ok(session
        .user
        .ok_or(KnownErrors::NotLoggedIn)
        .or_redirect("/login")?
        .id)
}

#[cfg(test)]
mod test_user {
    use crate::cuid::Cuid;
    use sqlx::{PgPool, prelude::FromRow};

    use super::UserEvent;

    #[sqlx::test]
    async fn test_encode_decode_cuid(pool: PgPool) {
        let original_event = UserEvent::CreatedJournal { id: Cuid::new10() };

        sqlx::query(
            r#"
            CREATE TABLE test_user_table (
            event BYTEA
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("failed to create mock user table");

        sqlx::query(
            r#"
            INSERT INTO test_user_table(
            event
            )
            VALUES ($1)
            "#,
        )
        .bind(&original_event)
        .execute(&pool)
        .await
        .expect("failed to insert user into mock table");

        let event: UserEvent = sqlx::query_scalar(
            r#"
            SELECT event FROM test_user_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch user from mock table");

        assert_eq!(event, original_event);

        #[derive(FromRow)]
        struct WrapperType {
            event: UserEvent,
        }

        let event_wrapper: WrapperType = sqlx::query_as(
            r#"
            SELECT event FROM test_user_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch user from mock table");

        assert_eq!(event_wrapper.event, original_event)
    }
}
