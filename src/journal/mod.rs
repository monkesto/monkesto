pub mod commands;
pub mod layout;
pub mod person;
pub mod service;
pub mod views;

use crate::ident::Ident;
use crate::store::universal::{DieselExecute, EventId, GetPayloadUsage, PayloadUsage};
pub use service::JournalService;
use std::collections::HashMap;

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

use crate::auth::user::UserStoreError;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::email::Email;
use crate::event::Event;
use crate::event::EventStore;
use crate::ident::IdentError;
use crate::journal::JournalStoreError::InvalidJournal;
use crate::name::Name;
use crate::schema::journal_members;
use crate::schema::journals;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::{entity, payload, state};
use bitflags::bitflags;
use chrono::DateTime;
use chrono::Utc;
use dashmap::DashMap;
use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::Integer;
use diesel::{
    AsExpression, BoolExpressionMethods, FromSqlRow, Insertable, Queryable, Selectable,
    SqliteConnection, deserialize, insert_into, serialize,
};
use diesel::{QueryResult, delete, update};
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Display;
use std::fmt::Formatter;
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

pub trait JournalStore:
    Clone
    + Send
    + Sync
    + 'static
    + EventStore<Id = JournalId, Payload = JournalPayload, Error = JournalStoreError>
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
        authority: &Authority,
    ) -> JournalStoreResult<Permissions>;

    async fn get_name(&self, journal_id: JournalId) -> JournalStoreResult<Name> {
        self.get_journal(journal_id)
            .await?
            .ok_or(InvalidJournal(journal_id))
            .map(|s| s.name)
    }

    async fn get_owner(&self, journal_id: JournalId) -> JournalStoreResult<UserId> {
        self.get_journal(journal_id)
            .await?
            .ok_or(InvalidJournal(journal_id))
            .map(|s| s.owner)
    }

    async fn get_creation_timestamp(
        &self,
        journal_id: JournalId,
    ) -> JournalStoreResult<Option<DateTime<Utc>>>;

    async fn get_creator(&self, journal_id: JournalId) -> JournalStoreResult<Authority>;

    async fn get_members(
        &self,
        journal_id: JournalId,
    ) -> JournalStoreResult<HashMap<UserId, Permissions>>;
}

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct JournalMemoryStore {
    global_events: Arc<RwLock<Vec<Arc<Event<JournalPayload, JournalId>>>>>,
    local_events: Arc<DashMap<JournalId, Vec<Arc<Event<JournalPayload, JournalId>>>>>,

    journal_table: Arc<DashMap<JournalId, JournalState>>,
    journal_members: Arc<DashMap<(JournalId, UserId), Permissions>>,
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
            journal_members: Arc::new(DashMap::new()),
            user_journals: Arc::new(DashMap::new()),
            subjournals: Arc::new(DashMap::new()),
        }
    }
}

impl EventStore for JournalMemoryStore {
    type Id = JournalId;
    type EventId = u64;
    type Payload = JournalPayload;
    type Error = JournalStoreError;

    async fn record(
        &self,
        id: JournalId,
        authority: Authority,
        payload: JournalPayload,
    ) -> JournalStoreResult<u64> {
        let (event_id, event) = {
            let mut global_events = self.global_events.write().await;
            let event_id = global_events.len() as u64;
            let event = Arc::new(Event::new(payload.clone(), id, event_id, authority));
            global_events.push(event.clone());
            (event_id, event)
        };

        match payload.clone().usage(id, EventId(0)) {
            PayloadUsage::CreatesState(state) => {
                self.local_events.insert(id, vec![event.clone()]);

                // Add creator to the user_journals index
                self.user_journals
                    .entry(state.owner)
                    .or_default()
                    .insert(id);

                // Add to subjournal index if this is a child journal
                if let Some(parent_id) = state.parent_journal_id {
                    self.subjournals.entry(parent_id).or_default().insert(id);
                }

                self.journal_table.insert(id, state);
            }
            PayloadUsage::ModifiesState(mod_fn) => {
                if let Some(mut local_events) = self.local_events.get_mut(&id)
                    && let Some(mut state) = self.journal_table.get_mut(&id)
                {
                    match payload {
                        JournalPayload::Modified(JournalModifiedPayload::AddedTenant {
                            id: user_id,
                            permissions,
                        }) => {
                            self.user_journals.entry(user_id).or_default().insert(id);
                            self.journal_members.insert((id, user_id), permissions);
                        }
                        JournalPayload::Modified(JournalModifiedPayload::RemovedTenant {
                            id: user_id,
                        }) => {
                            self.user_journals.entry(user_id).or_default().remove(&id);
                            self.journal_members.remove(&(id, user_id));
                        }
                        _ => {}
                    }

                    local_events.push(event.clone());
                    mod_fn(&mut state);
                } else {
                    return Err(InvalidJournal(id));
                }
            }
        }

        Ok(event_id)
    }

    async fn get_events(
        &self,
        id: JournalId,
        after: u64,
        limit: u64,
    ) -> Result<Vec<Event<JournalPayload, JournalId>>, Self::Error> {
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

    async fn get_creator(&self, journal_id: JournalId) -> JournalStoreResult<Authority> {
        self.local_events
            .get(&journal_id)
            .and_then(|j| j.first().map(|e| e.authority.clone()))
            .ok_or(InvalidJournal(journal_id))
    }
    async fn get_permissions(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> JournalStoreResult<Permissions> {
        match authority {
            Authority::Direct(actor) => match actor {
                Actor::User(user_id) => {
                    if self.get_owner(journal_id).await? == *user_id {
                        return Ok(Permissions::all());
                    }

                    if let Some(perms) = self.journal_members.get(&(journal_id, *user_id)) {
                        Ok(*perms)
                    } else {
                        Ok(Permissions::empty())
                    }
                }
                Actor::System => Ok(Permissions::all()),
                Actor::Anonymous => Ok(Permissions::empty()),
            },
            Authority::Delegated { .. } => {
                todo!("handle delegated permissions")
            }
        }
    }

    async fn get_members(
        &self,
        journal_id: JournalId,
    ) -> JournalStoreResult<HashMap<UserId, Permissions>> {
        Ok(self
            .journal_members
            .iter()
            .filter(|ref_multi| ref_multi.key().0 == journal_id)
            .map(|ref_multi| (ref_multi.key().1, *ref_multi.value()))
            .collect())
    }
}

bitflags! {
    #[derive(Hash, Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, AsExpression, FromSqlRow)]
    #[diesel(sql_type = diesel::sql_types::Integer)]
    pub struct Permissions: i32 {
        const READ = 1 << 0;
        const ADD_ACCOUNT = 1 << 1;
        const APPEND_TRANSACTION = 1 << 2;
        const INVITE = 1 << 3;
        const CREATE_SUBJOURNAL = 1 << 4;
        const OWNER = 1 << 5;
    }
}

#[derive(Debug, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::journal_members)]
pub struct JournalMember {
    user_id: UserId,
    journal_id: JournalId,
    permissions: Permissions,
}

impl ToSql<Integer, diesel::sqlite::Sqlite> for Permissions {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::sqlite::Sqlite>) -> serialize::Result {
        out.set_value(self.bits());
        Ok(serialize::IsNull::No)
    }
}

impl ToSql<Integer, diesel::pg::Pg> for Permissions {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::pg::Pg>) -> serialize::Result {
        <i32 as ToSql<Integer, diesel::pg::Pg>>::to_sql(&(self.bits()), &mut out.reborrow())
    }
}

impl<DB: Backend> FromSql<Integer, DB> for Permissions
where
    i32: FromSql<Integer, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        let val = i32::from_sql(value)?;
        Ok(Permissions::from_bits(val).ok_or(PermissionDecodeError(val))?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
struct PermissionDecodeError(i32);

impl Display for PermissionDecodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "an unknown bit was set in the permission value: {}",
            self.0
        )
    }
}

entity!(
    JournalEntity,
    EntityType::Journal,
    JournalId,
    JournalPayload,
    JournalState,
    Ident::new10()
);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum JournalModifiedPayload {
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

payload! {
    AnyPayload::Journal,

    pub enum JournalPayload {
        Created {
            name: Name,
            owner: UserId,
            parent_journal_id: Option<JournalId>,
        },
        Modified(JournalModifiedPayload)
    }
}

state! {
    #[diesel(table_name = crate::schema::journals)]
    pub struct JournalState {
        pub id: JournalId,
        pub name: Name,
        pub owner: UserId,
        // implementations must find a way to store this separately
        // pub members: Postcard<HashMap<UserId, Permissions>>,
        pub parent_journal_id: Option<JournalId>,
        pub as_of: EventId
    }
}

impl DieselExecute for JournalPayload {
    fn execute_sql(
        &self,
        entity_id: Ident,
        event_id: EventId,
        conn: &mut SqliteConnection,
    ) -> QueryResult<()> {
        match self {
            JournalPayload::Created {
                name,
                owner,
                parent_journal_id,
            } => {
                insert_into(journals::table)
                    .values(JournalState {
                        id: entity_id.into(),
                        name: name.clone(),
                        owner: *owner,
                        parent_journal_id: *parent_journal_id,
                        as_of: event_id,
                    })
                    .execute(conn)?;
                insert_into(journal_members::table)
                    .values(JournalMember {
                        user_id: *owner,
                        journal_id: entity_id.into(),
                        permissions: Permissions::all(),
                    })
                    .execute(conn)
                    .map(drop)
            }
            JournalPayload::Modified(modified_payload) => match modified_payload {
                JournalModifiedPayload::Renamed { name } => update(journals::table)
                    .set((journals::name.eq(name), journals::as_of.eq(event_id)))
                    .execute(conn)
                    .map(drop),
                JournalModifiedPayload::TransferredOwnership { new_owner } => {
                    update(journals::table)
                        .set((journals::owner.eq(new_owner), journals::as_of.eq(event_id)))
                        .execute(conn)
                        .map(drop)
                }
                JournalModifiedPayload::Deleted => {
                    delete(journals::table.filter(journals::id.eq(entity_id))).execute(conn)?;
                    delete(journal_members::table.filter(journal_members::journal_id.eq(entity_id)))
                        .execute(conn)
                        .map(drop)
                }
                JournalModifiedPayload::AddedTenant { id, permissions } => {
                    insert_into(journal_members::table)
                        .values(JournalMember {
                            user_id: *id,
                            journal_id: entity_id.into(),
                            permissions: *permissions,
                        })
                        .execute(conn)?;
                    update(journals::table)
                        .set(journals::as_of.eq(event_id))
                        .execute(conn)
                        .map(drop)
                }
                JournalModifiedPayload::UpdatedTenantPermissions { id, permissions } => {
                    update(
                        journal_members::table.filter(
                            journal_members::journal_id
                                .eq(entity_id)
                                .and(journal_members::user_id.eq(id)),
                        ),
                    )
                    .set(journal_members::permissions.eq(permissions))
                    .execute(conn)?;

                    update(journals::table)
                        .set(journals::as_of.eq(event_id))
                        .execute(conn)
                        .map(drop)
                }
                JournalModifiedPayload::RemovedTenant { id } => {
                    delete(
                        journal_members::table.filter(
                            journal_members::user_id
                                .eq(id)
                                .and(journal_members::journal_id.eq(entity_id)),
                        ),
                    )
                    .execute(conn)?;
                    update(journals::table)
                        .set(journals::as_of.eq(event_id))
                        .execute(conn)
                        .map(drop)
                }
            },
        }
    }
}

impl GetPayloadUsage<JournalEntity> for JournalPayload {
    fn usage<T: Into<JournalId>>(
        self,
        entity_id: T,
        event_id: EventId,
    ) -> PayloadUsage<JournalEntity> {
        match self {
            JournalPayload::Created {
                name,
                owner,
                parent_journal_id,
            } => PayloadUsage::CreatesState(JournalState {
                id: entity_id.into(),
                name,
                owner,
                parent_journal_id,
                as_of: event_id,
            }),
            JournalPayload::Modified(modified_payload) => {
                PayloadUsage::ModifiesState(Box::new(move |state: &mut JournalState| {
                    match modified_payload {
                        JournalModifiedPayload::Renamed { name } => state.name = name,
                        JournalModifiedPayload::AddedTenant {
                            id: _,
                            permissions: _,
                        } => {}
                        JournalModifiedPayload::TransferredOwnership { new_owner } => {
                            state.owner = new_owner
                        }
                        JournalModifiedPayload::RemovedTenant { id: _ } => {}
                        JournalModifiedPayload::UpdatedTenantPermissions {
                            id: _,
                            permissions: _,
                        } => {}
                        JournalModifiedPayload::Deleted => {}
                    }
                    state.as_of = event_id;
                }))
            }
        }
    }
}

pub trait JournalNameOrUnknown {
    fn or_unknown(&self) -> String;
}

impl<E> JournalNameOrUnknown for Result<JournalState, E>
where
    E: std::error::Error,
{
    fn or_unknown(&self) -> String {
        match self {
            Ok(journal) => journal.name.to_string(),
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

impl<E> JournalNameOrUnknown for Result<Name, E>
where
    E: std::error::Error,
{
    fn or_unknown(&self) -> String {
        match self {
            Ok(journal) => journal.to_string(),
            Err(e) => format!("Error loading journal: {}", e),
        }
    }
}
