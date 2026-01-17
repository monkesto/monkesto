use crate::AuthSession;
use crate::ident::{JournalId, UserId};
use crate::known_errors::KnownErrors;
use crate::known_errors::RedirectOnError;
use crate::webauthn::user::Email;
use axum::response::Redirect;
use axum_login::AuthUser;
use serde::{Deserialize, Serialize};
use sqlx::Decode;
use sqlx::Encode;
use sqlx::Type;
use sqlx::postgres::PgValueRef;
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum UserEvent {
    Created {
        id: UserId,
        email: Email,
        pw_hash: String,
    },
    UpdatedEmail {
        email: Email,
    },
    CreatedJournal {
        journal_id: JournalId,
    },
    InvitedToJournal {
        journal_id: JournalId,
    },
    AcceptedJournalInvite {
        journal_id: JournalId,
    },
    DeclinedJournalInvite {
        journal_id: JournalId,
    },
    RemovedFromJournal {
        id: JournalId,
    },
    Deleted,
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

#[derive(Clone, Debug)]
pub struct UserState {
    pub id: UserId,
    pub email: Email,
    pub pw_hash: String,
    pub session_hash: [u8; 16],
    pub pending_journal_invites: HashSet<JournalId>,
    pub associated_journals: HashSet<JournalId>,
    pub deleted: bool,
}

impl AuthUser for UserState {
    type Id = UserId;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        &self.session_hash
    }
}

impl UserState {
    pub fn apply(&mut self, event: UserEvent) {
        match event {
            UserEvent::Created { id, email, pw_hash } => {
                self.id = id;
                self.email = email;
                self.pw_hash = pw_hash;
            }
            UserEvent::UpdatedEmail { email } => self.email = email,
            UserEvent::CreatedJournal { journal_id } => {
                _ = self.associated_journals.insert(journal_id)
            }
            UserEvent::InvitedToJournal { journal_id } => {
                _ = self.pending_journal_invites.insert(journal_id)
            }
            UserEvent::DeclinedJournalInvite { journal_id } => {
                _ = self.pending_journal_invites.remove(&journal_id)
            }
            UserEvent::AcceptedJournalInvite { journal_id } => {
                if self.pending_journal_invites.contains(&journal_id) {
                    _ = self.associated_journals.insert(journal_id);
                }
            }
            UserEvent::RemovedFromJournal { id } => _ = self.associated_journals.remove(&id),
            UserEvent::Deleted => self.deleted = true,
        }
    }
}

pub fn get_user(session: AuthSession) -> Result<UserState, Redirect> {
    session
        .user
        .ok_or(KnownErrors::NotLoggedIn)
        .or_redirect("/login")
}

#[cfg(test)]
mod test_user {
    use crate::ident::JournalId;
    use sqlx::{PgPool, prelude::FromRow};

    use super::UserEvent;

    #[sqlx::test]
    async fn test_encode_decode_userevent(pool: PgPool) {
        let original_event = UserEvent::CreatedJournal {
            journal_id: JournalId::new(),
        };

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
