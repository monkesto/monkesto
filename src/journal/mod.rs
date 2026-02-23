pub mod commands;
pub mod layout;
pub mod person;
pub mod service;
pub mod views;

pub use service::JournalService;

use axum::Router;
use axum::routing::get;
use axum_login::login_required;

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
            axum::routing::post(commands::invite_user),
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
            axum::routing::post(commands::remove_tenant),
        )
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use crate::authority::Authority;
use crate::authority::UserId;
use crate::event::EventStore;
use crate::ident::JournalId;
use crate::known_errors::KnownErrors;
use crate::known_errors::MonkestoResult;
use bitflags::bitflags;
use chrono::DateTime;
use chrono::Utc;
use dashmap::DashMap;
use serde::Deserialize;
use serde::Serialize;
use sqlx::Decode;
use sqlx::Encode;
use sqlx::Type;
use sqlx::postgres::PgValueRef;
use std::collections::HashMap;
use std::sync::Arc;

#[expect(dead_code)]
pub trait JournalStore:
    Clone
    + Send
    + Sync
    + 'static
    + EventStore<Id = JournalId, EventId = (), Event = JournalEvent, Error = KnownErrors>
{
    /// returns the cached state of the journal
    async fn get_journal(&self, journal_id: JournalId) -> MonkestoResult<Option<JournalState>>;

    /// returns all journals that a user is a member of (owner or tenant)
    async fn get_user_journals(&self, user_id: UserId) -> MonkestoResult<Vec<JournalId>>;

    /// returns all direct child journals of the given journal
    async fn get_subjournals(&self, journal_id: JournalId) -> MonkestoResult<Vec<JournalId>>;

    async fn get_permissions(
        &self,
        journal_id: JournalId,
        user_id: UserId,
    ) -> MonkestoResult<Option<Permissions>> {
        if let Some(state) = self.get_journal(journal_id).await? {
            if state.owner == user_id {
                return Ok(Some(Permissions::all()));
            }
            return Ok(Some(
                state
                    .tenants
                    .get(&user_id)
                    .map(|t| t.tenant_permissions)
                    .unwrap_or(Permissions::empty()),
            ));
        }
        Ok(None)
    }

    async fn get_name(&self, journal_id: JournalId) -> MonkestoResult<Option<String>> {
        Ok(self.get_journal(journal_id).await?.map(|s| s.name))
    }

    async fn get_creator(&self, journal_id: JournalId) -> MonkestoResult<Option<UserId>> {
        Ok(self.get_journal(journal_id).await?.map(|s| s.creator))
    }

    async fn get_created_at(&self, journal_id: JournalId) -> MonkestoResult<Option<DateTime<Utc>>> {
        Ok(self.get_journal(journal_id).await?.map(|s| s.created_at))
    }

    async fn get_deleted(&self, journal_id: JournalId) -> MonkestoResult<Option<bool>> {
        Ok(self.get_journal(journal_id).await?.map(|s| s.deleted))
    }
}

#[derive(Clone)]
pub struct JournalMemoryStore {
    events: Arc<DashMap<JournalId, Vec<JournalEvent>>>,
    journal_table: Arc<DashMap<JournalId, JournalState>>,
    /// Index of user_id -> set of journal_ids they belong to
    user_journals: Arc<DashMap<UserId, std::collections::HashSet<JournalId>>>,
    /// Index of parent_journal_id -> set of child journal_ids
    subjournals: Arc<DashMap<JournalId, std::collections::HashSet<JournalId>>>,
}

impl JournalMemoryStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(DashMap::new()),
            journal_table: Arc::new(DashMap::new()),
            user_journals: Arc::new(DashMap::new()),
            subjournals: Arc::new(DashMap::new()),
        }
    }
}

impl EventStore for JournalMemoryStore {
    type Id = JournalId;
    type EventId = ();
    type Event = JournalEvent;
    type Error = KnownErrors;

    async fn record(
        &self,
        id: JournalId,
        by: Authority,
        event: JournalEvent,
        _tx: Option<&mut sqlx::PgTransaction<'_>>,
    ) -> MonkestoResult<()> {
        _ = by; // Store doesn't use authority yet, but will for audit trail

        if let JournalEvent::Created {
            name,
            created_at,
            creator,
            parent_journal_id,
        } = event.clone()
        {
            self.events.insert(id, vec![event]);

            let state = JournalState {
                id,
                name,
                created_at,
                creator,
                owner: creator,
                tenants: HashMap::new(),
                deleted: false,
                parent_journal_id,
            };
            self.journal_table.insert(id, state);

            // Add creator to the user_journals index
            self.user_journals.entry(creator).or_default().insert(id);

            // Add to subjournal index if this is a child journal
            if let Some(parent_id) = parent_journal_id {
                self.subjournals.entry(parent_id).or_default().insert(id);
            }

            Ok(())
        } else if let Some(mut events) = self.events.get_mut(&id)
            && let Some(mut state) = self.journal_table.get_mut(&id)
        {
            // Update user_journals index for membership changes
            if let JournalEvent::AddedTenant { id: user_id, .. } = &event {
                self.user_journals.entry(*user_id).or_default().insert(id);
            } else if let JournalEvent::RemovedTenant { id: user_id } = &event {
                self.user_journals.entry(*user_id).or_default().remove(&id);
            }
            state.apply(event.clone());
            events.push(event);
            Ok(())
        } else {
            Err(KnownErrors::InvalidJournal)
        }
    }
}

impl JournalStore for JournalMemoryStore {
    async fn get_journal(&self, journal_id: JournalId) -> MonkestoResult<Option<JournalState>> {
        Ok(self
            .journal_table
            .get(&journal_id)
            .map(|state| (*state).clone()))
    }

    async fn get_user_journals(&self, user_id: UserId) -> MonkestoResult<Vec<JournalId>> {
        Ok(self
            .user_journals
            .get(&user_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default())
    }

    async fn get_subjournals(&self, journal_id: JournalId) -> MonkestoResult<Vec<JournalId>> {
        Ok(self
            .subjournals
            .get(&journal_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default())
    }
}

bitflags! {
    #[derive(Serialize, Deserialize, Hash, Default, Debug, Clone, Copy, PartialEq)]
    pub struct Permissions: i16 {
        const READ = 1 << 0;
        const ADDACCOUNT = 1 << 1;
        const APPENDTRANSACTION = 1 << 2;
        const INVITE = 1 << 3;
        const DELETE = 1 << 4;
        const OWNER = 1 << 5;
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Copy, PartialEq)]
pub struct JournalTenantInfo {
    pub tenant_permissions: Permissions,
    pub inviting_user: UserId,
    pub invited_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum JournalEvent {
    Created {
        name: String,
        created_at: DateTime<Utc>,
        creator: UserId,
        parent_journal_id: Option<JournalId>,
    },
    Renamed {
        name: String,
    },
    AddedTenant {
        id: UserId,
        tenant_info: JournalTenantInfo,
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

impl Type<sqlx::Postgres> for JournalEvent {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for JournalEvent {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let bytes: Vec<u8> = postcard::to_allocvec(self)?;
        <&[u8] as Encode<sqlx::Postgres>>::encode(&bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for JournalEvent {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(postcard::from_bytes::<JournalEvent>(bytes)?)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct JournalState {
    pub id: JournalId,
    pub name: String,
    pub creator: UserId,
    pub created_at: DateTime<Utc>,
    pub owner: UserId,
    pub tenants: HashMap<UserId, JournalTenantInfo>,
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
            Ok(Some(journal)) => journal.name.to_owned(),
            Ok(None) => "Unknown Journal".into(),
            Err(e) => format!("Error loading journal: {}", e),
        }
    }
}

impl<E> JournalNameOrUnknown for Result<Option<String>, E>
where
    E: std::error::Error,
{
    fn or_unknown(&self) -> String {
        match self {
            Ok(Some(journal)) => journal.to_owned(),
            Ok(None) => "Unknown Journal".into(),
            Err(e) => format!("Error loading journal: {}", e),
        }
    }
}

impl JournalState {
    pub fn apply(&mut self, event: JournalEvent) {
        match event {
            JournalEvent::Created {
                name,
                creator,
                created_at,
                parent_journal_id,
            } => {
                self.name = name;
                self.created_at = created_at;
                self.creator = creator;
                self.owner = creator;
                self.parent_journal_id = parent_journal_id;
            }

            JournalEvent::Renamed { name } => self.name = name,

            JournalEvent::AddedTenant { id, tenant_info } => {
                _ = self.tenants.insert(id, tenant_info);
            }

            JournalEvent::TransferredOwnership { new_owner } => self.owner = new_owner,

            JournalEvent::RemovedTenant { id } => {
                _ = self.tenants.remove(&id);
            }
            JournalEvent::UpdatedTenantPermissions { id, permissions } => {
                if let Some(tenant_info) = self.tenants.get_mut(&id) {
                    tenant_info.tenant_permissions = permissions;
                }
            }
            JournalEvent::Deleted => self.deleted = true,
        }
    }

    pub fn get_user_permissions(&self, user_id: UserId) -> Permissions {
        if self.owner == user_id {
            Permissions::all()
        } else if let Some(tenant_info) = self.tenants.get(&user_id) {
            tenant_info.tenant_permissions
        } else {
            Permissions::empty()
        }
    }
}

#[cfg(test)]
mod test_user {
    use crate::authority::UserId;
    use chrono::Utc;
    use sqlx::PgPool;
    use sqlx::prelude::FromRow;

    use super::JournalEvent;

    #[sqlx::test]
    async fn test_encode_decode_journalevent(pool: PgPool) {
        let original_event = JournalEvent::Created {
            name: "test".into(),
            creator: UserId::new(),
            created_at: Utc::now(),
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

        let event: JournalEvent = sqlx::query_scalar(
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
            event: JournalEvent,
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
}
