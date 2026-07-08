use super::GrantId;
use super::GrantPayload;
use super::RoleId;
use super::RolePayload;
use super::projection::AuthzProjection;
use super::role::RoleState;
use super::store::AuthzEvent;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::id::IdentError;
use crate::name::Name;
use crate::store::Event;
use crate::store::EventFor;
use crate::store::EventId;
use crate::store::Stream;
use crate::store::When;
use crate::store::sqlite::SqliteEvent;
use crate::store::sqlite::SqliteEventCodec;
use crate::store::sqlite::SqliteProjection;
use crate::store::sqlite::SqliteStatement;
use crate::store::sqlite::SqliteStore;
use crate::store::sqlite::SqliteValue;
use sqlx::Row;
use sqlx::SqlitePool;
use std::any::Any;
use std::collections::HashSet;
use thiserror::Error;

pub type AuthzSqliteStore = SqliteStore<AuthzEvent, AuthzSqliteProjection>;
pub type AuthzSqliteService = super::service::AuthzService<AuthzSqliteStore, AuthzSqliteProjection>;

pub async fn connect_service(pool: SqlitePool) -> Result<AuthzSqliteService, sqlx::Error> {
    let projection = AuthzSqliteProjection::new(pool.clone());
    let store = SqliteStore::new(pool, projection.clone()).await?;
    Ok(super::service::AuthzService::new(store, projection))
}

#[derive(Clone)]
pub struct AuthzSqliteProjection {
    pool: SqlitePool,
}

impl AuthzSqliteProjection {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Error)]
pub enum AuthzSqliteError {
    #[error("sqlite error")]
    Sqlite(#[from] sqlx::Error),
    #[error("postcard error")]
    Postcard(#[from] postcard::Error),
    #[error("ident error")]
    Ident(#[from] IdentError),
    #[error("unexpected authz stream type: {0}")]
    UnexpectedStreamType(i64),
    #[error("unexpected authz payload type")]
    UnexpectedPayloadType,
    #[error("invalid event id: {0}")]
    InvalidEventId(i64),
}

impl SqliteEventCodec<AuthzEvent> for AuthzEvent {
    type Error = AuthzSqliteError;

    fn encode_authority(authority: &Authority) -> Result<Vec<u8>, Self::Error> {
        Ok(postcard::to_allocvec(authority)?)
    }

    fn encode_payload<S>(payload: &S::Payload) -> Result<Vec<u8>, Self::Error>
    where
        S: Stream,
        AuthzEvent: EventFor<S>,
    {
        let payload = payload as &dyn Any;
        if let Some(payload) = payload.downcast_ref::<RolePayload>() {
            Ok(postcard::to_allocvec(payload)?)
        } else if let Some(payload) = payload.downcast_ref::<GrantPayload>() {
            Ok(postcard::to_allocvec(payload)?)
        } else {
            Err(AuthzSqliteError::UnexpectedPayloadType)
        }
    }

    fn decode_event(event: SqliteEvent) -> Result<AuthzEvent, Self::Error> {
        let authority = postcard::from_bytes::<Authority>(&event.authority)?;
        Ok(match event.stream_type {
            1 => AuthzEvent::Role(Event {
                event_id: event.event_id,
                timestamp: event.timestamp,
                authority,
                id: RoleId::try_from(event.stream_id.as_slice())?,
                payload: postcard::from_bytes::<RolePayload>(&event.payload)?,
            }),
            2 => AuthzEvent::Grant(Event {
                event_id: event.event_id,
                timestamp: event.timestamp,
                authority,
                id: GrantId::try_from(event.stream_id.as_slice())?,
                payload: postcard::from_bytes::<GrantPayload>(&event.payload)?,
            }),
            stream_type => return Err(AuthzSqliteError::UnexpectedStreamType(stream_type)),
        })
    }
}

impl SqliteProjection<AuthzEvent> for AuthzSqliteProjection {
    type Error = AuthzSqliteError;

    fn schema(&self) -> &'static str {
        r#"
            CREATE TABLE IF NOT EXISTS authz_role (
                id BLOB NOT NULL PRIMARY KEY,
                name BLOB NOT NULL,
                latest_event_id INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS authz_role_actor (
                role_id BLOB NOT NULL,
                actor BLOB NOT NULL,
                PRIMARY KEY (role_id, actor)
            );

            CREATE INDEX IF NOT EXISTS authz_role_actor_actor_idx
            ON authz_role_actor (actor);
            "#
    }

    fn plan(&self, event: &AuthzEvent) -> Result<Vec<SqliteStatement>, Self::Error> {
        let AuthzEvent::Role(event) = event else {
            return Ok(Vec::new());
        };

        match &event.payload {
            RolePayload::Created(name) => Ok(vec![SqliteStatement {
                sql: r#"
                INSERT INTO authz_role (id, name, latest_event_id)
                VALUES (?, ?, ?)
                "#,
                binds: vec![
                    SqliteValue::Bytes(event.id.as_bytes().to_vec()),
                    SqliteValue::Bytes(postcard::to_allocvec(name)?),
                    SqliteValue::I64(event_id_i64(event.event_id)?),
                ],
            }]),
            RolePayload::ActorAdded(actor) => Ok(vec![
                SqliteStatement {
                    sql: r#"
                    INSERT OR IGNORE INTO authz_role_actor (role_id, actor)
                    VALUES (?, ?)
                    "#,
                    binds: vec![
                        SqliteValue::Bytes(event.id.as_bytes().to_vec()),
                        SqliteValue::Bytes(postcard::to_allocvec(actor)?),
                    ],
                },
                update_latest_statement(event.id, event.event_id)?,
            ]),
            RolePayload::ActorRemoved(actor) => Ok(vec![
                SqliteStatement {
                    sql: r#"
                    DELETE FROM authz_role_actor
                    WHERE role_id = ? AND actor = ?
                    "#,
                    binds: vec![
                        SqliteValue::Bytes(event.id.as_bytes().to_vec()),
                        SqliteValue::Bytes(postcard::to_allocvec(actor)?),
                    ],
                },
                update_latest_statement(event.id, event.event_id)?,
            ]),
        }
    }
}

impl AuthzProjection for AuthzSqliteProjection {
    type Error = AuthzSqliteError;

    async fn role(&self, role_id: RoleId) -> Result<RoleState, Self::Error> {
        let role = sqlx::query(
            r#"
            SELECT name, latest_event_id
            FROM authz_role
            WHERE id = ?
            "#,
        )
        .bind(role_id.as_bytes().to_vec())
        .fetch_optional(&self.pool)
        .await?;

        let Some(role) = role else {
            return Ok(RoleState::Absent);
        };

        let actor_rows = sqlx::query(
            r#"
            SELECT actor
            FROM authz_role_actor
            WHERE role_id = ?
            "#,
        )
        .bind(role_id.as_bytes().to_vec())
        .fetch_all(&self.pool)
        .await?;

        let actors = actor_rows
            .into_iter()
            .map(|row| {
                let actor: Vec<u8> = row.get("actor");
                postcard::from_bytes::<Actor>(&actor).map_err(AuthzSqliteError::from)
            })
            .collect::<Result<HashSet<_>, _>>()?;

        let name: Vec<u8> = role.get("name");
        let latest_event_id: i64 = role.get("latest_event_id");

        Ok(RoleState::Present {
            name: postcard::from_bytes::<Name>(&name)?,
            actors,
            when: When::Within(EventId::from(
                u64::try_from(latest_event_id)
                    .map_err(|_| AuthzSqliteError::InvalidEventId(latest_event_id))?,
            )),
        })
    }

    async fn roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, Self::Error> {
        let rows = sqlx::query(
            r#"
            SELECT role_id
            FROM authz_role_actor
            WHERE actor = ?
            "#,
        )
        .bind(postcard::to_allocvec(actor)?)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let role_id: Vec<u8> = row.get("role_id");
                RoleId::try_from(role_id.as_slice()).map_err(AuthzSqliteError::from)
            })
            .collect()
    }

    async fn all_roles(&self) -> Result<Vec<(RoleId, RoleState)>, Self::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id
            FROM authz_role
            ORDER BY id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut roles = Vec::with_capacity(rows.len());
        for row in rows {
            let role_id_bytes: Vec<u8> = row.get("id");
            let role_id = RoleId::try_from(role_id_bytes.as_slice())?;
            roles.push((role_id, self.role(role_id).await?));
        }
        Ok(roles)
    }
}

fn update_latest_statement(
    role_id: RoleId,
    event_id: EventId,
) -> Result<SqliteStatement, AuthzSqliteError> {
    Ok(SqliteStatement {
        sql: r#"
        UPDATE authz_role
        SET latest_event_id = ?
        WHERE id = ?
        "#,
        binds: vec![
            SqliteValue::I64(event_id_i64(event_id)?),
            SqliteValue::Bytes(role_id.as_bytes().to_vec()),
        ],
    })
}

fn event_id_i64(event_id: EventId) -> Result<i64, AuthzSqliteError> {
    i64::try_from(*event_id).map_err(|_| AuthzSqliteError::InvalidEventId(i64::MAX))
}
