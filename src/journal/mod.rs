pub mod commands;
pub mod layout;
pub mod person;
pub mod service;
pub mod views;

use crate::ident::Ident;
use crate::store::universal::{GetPayloadUsage, PayloadUsage, SequenceId};
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
use crate::journal::JournalStoreError::InvalidJournal;
use crate::name::Name;
use crate::postcard::Postcard;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::{entity, payload, state};
use bitflags::bitflags;
use chrono::DateTime;
use chrono::Utc;
use dashmap::DashMap;
use serde::Deserialize;
use serde::Serialize;
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
    global_events: Arc<RwLock<Vec<Arc<Event<JournalPayload, JournalId>>>>>,
    local_events: Arc<DashMap<JournalId, Vec<Arc<Event<JournalPayload, JournalId>>>>>,

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

        match payload.clone().usage(id, SequenceId(0)) {
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
                            ..
                        }) => _ = self.user_journals.entry(user_id).or_default().insert(id),
                        JournalPayload::Modified(JournalModifiedPayload::RemovedTenant {
                            id: user_id,
                        }) => _ = self.user_journals.entry(user_id).or_default().remove(&id),
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

#[expect(dead_code)]
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
        pub members: Postcard<HashMap<UserId, Permissions>>,
        pub deleted: bool,
        pub parent_journal_id: Option<JournalId>,
        pub as_of: SequenceId
    }
}

impl GetPayloadUsage<JournalEntity> for JournalPayload {
    fn usage<T: Into<JournalId>>(
        self,
        entity_id: T,
        sequence_id: SequenceId,
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
                members: Postcard(HashMap::new()),
                deleted: false,
                parent_journal_id,
                as_of: sequence_id,
            }),
            JournalPayload::Modified(modified_payload) => {
                PayloadUsage::ModifiesState(Box::new(move |state: &mut JournalState| {
                    match modified_payload {
                        JournalModifiedPayload::Renamed { name } => state.name = name,
                        JournalModifiedPayload::AddedTenant { id, permissions } => {
                            _ = state.members.insert(id, permissions)
                        }
                        JournalModifiedPayload::TransferredOwnership { new_owner } => {
                            state.owner = new_owner
                        }
                        JournalModifiedPayload::RemovedTenant { id } => {
                            _ = state.members.remove(&id)
                        }
                        JournalModifiedPayload::UpdatedTenantPermissions { id, permissions } => {
                            if let std::collections::hash_map::Entry::Occupied(mut e) =
                                state.members.entry(id)
                            {
                                _ = Some(e.insert(permissions))
                            }
                        }
                        JournalModifiedPayload::Deleted => state.deleted = true,
                    }
                    state.as_of = sequence_id;
                }))
            }
        }
    }
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
