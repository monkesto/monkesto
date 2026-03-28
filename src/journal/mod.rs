pub mod commands;
pub mod layout;
pub mod person;
pub mod service;
pub mod views;

pub use service::JournalService;

use axum::Router;
use axum::routing::get;
use axum_login::login_required;

#[derive(Error, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum JournalStoreError {
    #[error("invalid journal: {0}")]
    InvalidJournal(JournalId),

    #[error("user doesn't exist: {0}")]
    InvalidUser(UserId),

    #[error("The user doesn't have the {:?} permission", .0)]
    PermissionError(Permissions),

    #[error("The user store returned an error {0}")]
    UserError(#[from] UserStoreError),

    #[error("Unable to find a user id for {0}")]
    UserLookupFailed(Email),

    #[error("The user {0} already has access to this journal")]
    UserAlreadyHasAccess(Email),

    #[error("The user doesn't have access to this journal")]
    UserDoesntHaveAccess,

    #[error("Failed to create an Ident: {0}")]
    IdentCreation(#[from] IdentError),
}

pub type JournalStoreResult<T> = Result<T, JournalStoreError>;

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .route("/journal", get(views::journal_list))
        .route(
            "/createjournal",
            axum::routing::post(commands::create_journal),
        )
        .route("/journal/{id}", get(views::journal_detail))
        .route("/journal/{id}/person", get(person::people_list_page))
        .route(
            "/journal/{id}/subjournals",
            get(views::sub_journal_list_page),
        )
        .route(
            "/journal/{id}/createsubjournal",
            axum::routing::post(commands::create_sub_journal),
        )
        .route(
            "/journal/{id}/invite",
            axum::routing::post(commands::invite_member),
        )
        .route(
            "/journal/{id}/person/{person_id}",
            get(person::person_detail_page),
        )
        .route(
            "/journal/{id}/person/{person_id}/update",
            axum::routing::post(commands::update_permissions),
        )
        .route(
            "/journal/{id}/person/{person_id}/remove",
            axum::routing::post(commands::remove_member),
        )
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use crate::auth::user::Email;
use crate::auth::user::UserStoreError;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::event::Event;
use crate::event::EventStore;
use crate::ident::IdentError;
use crate::ident::JournalId;
use crate::journal::JournalStoreError::InvalidJournal;
use crate::name::Name;
use bitflags::bitflags;
use chrono::DateTime;
use chrono::Utc;
use dashmap::DashMap;
use serde::Deserialize;
use serde::Serialize;
use sqlx::Database;
use sqlx::Decode;
use sqlx::Encode;
use sqlx::Type;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::postgres::PgValueRef;
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::Formatter;
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[expect(dead_code)]
pub trait JournalStore:
    Clone
    + Send
    + Sync
    + 'static
    + EventStore<Id = JournalId, Payload = JounalPayload, Error = JournalStoreError>
{
    /// returns the cached state of the journal
    async fn get_journal(&self, journal_id: JournalId) -> JournalStoreResult<Option<JournalState>>;

    /// returns all journals that a user is a member of (owner or tenant)
    async fn get_user_journals(&self, user_id: UserId) -> JournalStoreResult<Vec<JournalId>>;

    /// returns all direct child journals of the given journal
    async fn get_subjournals(&self, journal_id: JournalId) -> JournalStoreResult<Vec<JournalId>>;

    async fn get_permissions(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalStoreResult<Option<Permissions>> {
        if let Some(state) = self.get_journal(journal_id).await? {
            return match authority {
                Authority::Direct(actor) => match actor {
                    Actor::User(user_id) => {
                        if state.owner == user_id {
                            return Ok(Some(Permissions::all()));
                        }
                        Ok(Some(
                            *state.members.get(&user_id).unwrap_or(&Permissions::empty()),
                        ))
                    }
                    Actor::System => Ok(Some(Permissions::all())),
                    Actor::Anonymous => Ok(None),
                },
                // TODO: handle delegated permissions
                _ => Ok(Some(Permissions::empty())),
            };
        }
        Ok(None)
    }

    async fn get_name(&self, journal_id: JournalId) -> JournalStoreResult<Option<Name>> {
        Ok(self.get_journal(journal_id).await?.map(|s| s.name))
    }

    async fn get_owner(&self, journal_id: JournalId) -> JournalStoreResult<Option<UserId>> {
        Ok(self.get_journal(journal_id).await?.map(|s| s.owner))
    }

    async fn get_creation_timestamp(
        &self,
        journal_id: JournalId,
    ) -> JournalStoreResult<Option<DateTime<Utc>>>;

    async fn get_creator(&self, journal_id: JournalId) -> JournalStoreResult<Option<Authority>>;

    async fn get_deleted(&self, journal_id: JournalId) -> JournalStoreResult<Option<bool>> {
        Ok(self.get_journal(journal_id).await?.map(|s| s.deleted))
    }
}

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct JournalMemoryStore {
    global_events: Arc<RwLock<Vec<Arc<Event<JounalPayload, JournalId>>>>>,
    local_events: Arc<DashMap<JournalId, Vec<Arc<Event<JounalPayload, JournalId>>>>>,

    journal_table: Arc<DashMap<JournalId, JournalState>>,
    /// Index of user_id -> set of journal_ids they belong to
    user_journals: Arc<DashMap<UserId, std::collections::HashSet<JournalId>>>,
    /// Index of parent_journal_id -> set of child journal_ids
    subjournals: Arc<DashMap<JournalId, std::collections::HashSet<JournalId>>>,
}

impl JournalMemoryStore {
    pub fn new() -> Self {
        Self {
            global_events: Arc::new(RwLock::new(Vec::new())),
            local_events: Arc::new(DashMap::new()),
            journal_table: Arc::new(DashMap::new()),
            user_journals: Arc::new(DashMap::new()),
            subjournals: Arc::new(DashMap::new()),
        }
    }
}

impl EventStore for JournalMemoryStore {
    type Id = JournalId;
    type EventId = u64;
    type Payload = JounalPayload;
    type Error = JournalStoreError;

    async fn record(
        &self,
        id: JournalId,
        authority: Authority,
        payload: JounalPayload,
    ) -> JournalStoreResult<u64> {
        let (event_id, event) = {
            let mut global_events = self.global_events.write().await;
            let event_id = global_events.len() as u64;
            let event = Arc::new(Event::new(payload.clone(), id, event_id, authority));
            global_events.push(event.clone());
            (event_id, event)
        };

        if let JounalPayload::Created {
            name,
            owner,
            parent_journal_id,
        } = payload
        {
            self.local_events.insert(id, vec![event.clone()]);

            let state = JournalState {
                id,
                name,
                owner,
                members: HashMap::new(),
                deleted: false,
                parent_journal_id,
            };
            self.journal_table.insert(id, state);

            // Add creator to the user_journals index
            self.user_journals.entry(owner).or_default().insert(id);

            // Add to subjournal index if this is a child journal
            if let Some(parent_id) = parent_journal_id {
                self.subjournals.entry(parent_id).or_default().insert(id);
            }

            Ok(event_id)
        } else if let Some(mut local_events) = self.local_events.get_mut(&id)
            && let Some(mut state) = self.journal_table.get_mut(&id)
        {
            // Update user_journals index for membership changes
            if let JounalPayload::AddedTenant { id: user_id, .. } = &payload {
                self.user_journals.entry(*user_id).or_default().insert(id);
            } else if let JounalPayload::RemovedTenant { id: user_id } = &payload {
                self.user_journals.entry(*user_id).or_default().remove(&id);
            }

            local_events.push(event.clone());
            state.apply(payload);

            Ok(event_id)
        } else {
            Err(InvalidJournal(id))
        }
    }

    async fn get_events(
        &self,
        id: JournalId,
        after: u64,
        limit: u64,
    ) -> Result<Vec<Event<JounalPayload, JournalId>>, Self::Error> {
        let events = self.local_events.get(&id).ok_or(InvalidJournal(id))?;

        // avoid a panic fn start > len
        if after >= events.len() as u64 {
            return Ok(Vec::new());
        }

        // clamp the end value to the vector length
        let end = std::cmp::min(events.len(), (after + limit + 1) as usize);

        Ok(events[(after + 1) as usize..end]
            .iter()
            .map(|j| j.deref().clone())
            .collect())
    }
}

impl JournalStore for JournalMemoryStore {
    async fn get_journal(&self, journal_id: JournalId) -> JournalStoreResult<Option<JournalState>> {
        Ok(self
            .journal_table
            .get(&journal_id)
            .map(|state| (*state).clone()))
    }

    async fn get_user_journals(&self, user_id: UserId) -> JournalStoreResult<Vec<JournalId>> {
        Ok(self
            .user_journals
            .get(&user_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default())
    }

    async fn get_subjournals(&self, journal_id: JournalId) -> JournalStoreResult<Vec<JournalId>> {
        Ok(self
            .subjournals
            .get(&journal_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default())
    }

    async fn get_creation_timestamp(
        &self,
        journal_id: JournalId,
    ) -> JournalStoreResult<Option<DateTime<Utc>>> {
        // get the timestamp of the first event pertaining to the journal

        // TODO: maybe? add a check to make sure that the first event is actually a creation payload; however, it should be enforced when the payload is recorded
        Ok(self
            .local_events
            .get(&journal_id)
            .and_then(|j| j.first().map(|e| e.timestamp)))
    }

    async fn get_creator(&self, journal_id: JournalId) -> JournalStoreResult<Option<Authority>> {
        Ok(self
            .local_events
            .get(&journal_id)
            .and_then(|j| j.first().map(|e| e.authority.clone())))
    }
}

bitflags! {
    #[derive(Hash, Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Permissions: i16 {
        const READ = 1 << 0;
        const ADDACCOUNT = 1 << 1;
        const APPENDTRANSACTION = 1 << 2;
        const INVITE = 1 << 3;
        const DELETE = 1 << 4;
        const OWNER = 1 << 5;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
struct PermissionDecodeError(i16);

impl Display for PermissionDecodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "an unknown bit was set in the permission value: {}",
            self.0
        )
    }
}

impl Type<sqlx::Postgres> for Permissions {
    fn type_info() -> <sqlx::Postgres as Database>::TypeInfo {
        <i16 as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for Permissions {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <i16 as Encode<sqlx::Postgres>>::encode(self.bits(), buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for Permissions {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        let bits = <i16 as Decode<sqlx::Postgres>>::decode(value)?;
        Self::from_bits(bits).ok_or(BoxDynError::from(PermissionDecodeError(bits)))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum JounalPayload {
    Created {
        name: Name,
        owner: UserId,
        parent_journal_id: Option<JournalId>,
    },
    Renamed {
        name: Name,
    },
    AddedTenant {
        id: UserId,
        permissions: Permissions,
    },
    TransferredOwnership {
        new_owner: UserId,
    },
    RemovedTenant {
        id: UserId,
    },
    UpdatedTenantPermissions {
        id: UserId,
        permissions: Permissions,
    },
    Deleted,
}

impl Type<sqlx::Postgres> for JounalPayload {
    fn type_info() -> <sqlx::Postgres as Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for JounalPayload {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes: Vec<u8> = postcard::to_allocvec(self)?;
        <&[u8] as Encode<sqlx::Postgres>>::encode(&bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for JounalPayload {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(postcard::from_bytes::<JounalPayload>(bytes)?)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct JournalState {
    pub id: JournalId,
    pub name: Name,
    pub owner: UserId,
    pub members: HashMap<UserId, Permissions>,
    pub deleted: bool,
    pub parent_journal_id: Option<JournalId>,
}

pub trait JournalNameOrUnknown {
    fn or_unknown(&self) -> String;
}

impl<E> JournalNameOrUnknown for Result<Option<JournalState>, E>
where
    E: std::error::Error,
{
    fn or_unknown(&self) -> String {
        match self {
            Ok(Some(journal)) => journal.name.to_string(),
            Ok(None) => "Unknown Journal".into(),
            Err(e) => format!("Error loading journal: {}", e),
        }
    }
}

impl<E> JournalNameOrUnknown for Result<Option<Name>, E>
where
    E: std::error::Error,
{
    fn or_unknown(&self) -> String {
        match self {
            Ok(Some(journal)) => journal.to_string(),
            Ok(None) => "Unknown Journal".into(),
            Err(e) => format!("Error loading journal: {}", e),
        }
    }
}

impl JournalState {
    pub fn apply(&mut self, payload: JounalPayload) {
        match payload {
            JounalPayload::Created {
                name,
                owner,
                parent_journal_id,
            } => {
                self.name = name;
                self.owner = owner;
                self.parent_journal_id = parent_journal_id;
            }

            JounalPayload::Renamed { name } => self.name = name,

            JounalPayload::AddedTenant { id, permissions } => {
                _ = self.members.insert(id, permissions);
            }

            JounalPayload::TransferredOwnership { new_owner } => self.owner = new_owner,

            JounalPayload::RemovedTenant { id } => {
                _ = self.members.remove(&id);
            }
            JounalPayload::UpdatedTenantPermissions { id, permissions } => {
                if let Some(member_permissions) = self.members.get_mut(&id) {
                    *member_permissions = permissions;
                }
            }
            JounalPayload::Deleted => self.deleted = true,
        }
    }

    pub fn get_actor_permissions(&self, authority: &Authority) -> Permissions {
        match authority {
            Authority::Direct(actor) => match actor {
                Actor::User(id) => {
                    if self.owner == *id {
                        Permissions::all()
                    } else if let Some(member_permissions) = self.members.get(id) {
                        *member_permissions
                    } else {
                        Permissions::empty()
                    }
                }
                Actor::System => Permissions::all(),
                Actor::Anonymous => Permissions::empty(),
            },
            // TODO: handle delegated permissions
            _ => Permissions::empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::JounalPayload;
    use super::Permissions;
    use crate::authority::UserId;
    use crate::name::Name;
    use sqlx::PgPool;
    use sqlx::prelude::FromRow;

    #[sqlx::test]
    async fn test_encode_decode_journalevent(pool: PgPool) {
        let original_event = JounalPayload::Created {
            name: Name::try_new("test".to_string()).expect("name creation failed"),
            owner: UserId::new(),
            parent_journal_id: None,
        };

        sqlx::query(
            r#"
            CREATE TABLE test_journal_table (
            event BYTEA
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("failed to create mock journal table");

        sqlx::query(
            r#"
            INSERT INTO test_journal_table(
            event
            )
            VALUES ($1)
            "#,
        )
        .bind(&original_event)
        .execute(&pool)
        .await
        .expect("failed to insert journal into mock table");

        let event: JounalPayload = sqlx::query_scalar(
            r#"
            SELECT event FROM test_journal_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch journal from mock table");

        assert_eq!(event, original_event);

        #[derive(FromRow)]
        struct WrapperType {
            event: JounalPayload,
        }

        let event_wrapper: WrapperType = sqlx::query_as(
            r#"
            SELECT event FROM test_journal_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch journal from mock table");

        assert_eq!(event_wrapper.event, original_event)
    }

    #[sqlx::test]
    pub async fn permissions_serialize_deserialize(pool: PgPool) {
        sqlx::query(
            r#"
            CREATE TABLE test_permissions_table (
            permissions SMALLSERIAL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("failed to create mock table");

        let test_permissions =
            Permissions::INVITE | Permissions::APPENDTRANSACTION | Permissions::READ;

        sqlx::query(
            r#"
                INSERT INTO test_permissions_table (permissions)
                VALUES ($1)
                "#,
        )
        .bind(test_permissions)
        .execute(&pool)
        .await
        .expect("failed to insert permissions into mock table");

        let decoded_permissions: Permissions = sqlx::query_scalar(
            r#"
            SELECT permissions FROM test_permissions_table

                LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch permissions from mock table");

        assert_eq!(test_permissions, decoded_permissions);
    }
}
