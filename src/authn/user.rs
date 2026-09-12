use crate::email::Email;
use disintegrate::{Decision, StateMutate, StateQuery};
use serde::Deserialize;
use serde::Serialize;
use sqlx::FromRow;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;

#[derive(Debug, Clone, FromRow)]
pub struct UserState {
    pub id: UserId,
    pub email: Email,
    pub webauthn_uuid: Uuid,
}

impl axum_login::AuthUser for UserState {
    type Id = UserId;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        // We don't invalidate sessions based on credential changes
        &[]
    }
}

#[derive(Debug, StateQuery, Clone, Serialize, Deserialize, Default)]
#[state_query(UserEvent)]
pub struct User {
    #[id]
    pub user_id: UserId,
    pub email: Email,
    pub webauthn_uuid: Uuid,
    pub status: Status,
}

#[derive(Debug, StateQuery, Clone, Serialize, Deserialize, Default)]
#[state_query(UserEvent)]
pub struct UserEmail {
    pub user_id: UserId,
    #[id]
    pub email: Email,
    pub webauthn_uuid: Uuid,
    pub status: Status,
}

impl User {
    pub fn new(user_id: UserId) -> Self {
        Self {
            user_id,
            ..Default::default()
        }
    }
}

impl UserEmail {
    pub fn new(email: Email) -> Self {
        Self {
            email,
            ..Default::default()
        }
    }
}

impl StateMutate for User {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            UserEvent::UserCreated {
                email,
                webauthn_uuid,
                ..
            } => {
                self.status = Status::Valid;
                self.email = email;
                self.webauthn_uuid = webauthn_uuid;
            }
            UserEvent::UserDeleted { .. } => self.status = Status::Deleted,
        }
    }
}

impl StateMutate for UserEmail {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            UserEvent::UserCreated {
                webauthn_uuid,
                user_id,
                ..
            } => {
                self.status = Status::Valid;
                self.user_id = user_id;
                self.webauthn_uuid = webauthn_uuid;
            }
            UserEvent::UserDeleted { .. } => self.status = Status::Deleted,
        }
    }
}

#[derive(Debug)]
pub struct CreateUser {
    pub user_id: UserId,
    pub email: Email,
    pub webauthn_uuid: Uuid,
    pub authority: Authority,
    pub timestamp: Timestamp,
}

impl CreateUser {
    pub fn new(
        user_id: UserId,
        email: Email,
        webauthn_uuid: Uuid,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            user_id,
            email,
            webauthn_uuid,
            authority,
            timestamp,
        }
    }
}

impl Decision for CreateUser {
    type Event = AuthnEvent;
    type StateQuery = (User, UserEmail);
    type Error = UserError;

    fn state_query(&self) -> Self::StateQuery {
        (User::new(self.user_id), UserEmail::new(self.email.clone()))
    }

    fn process(
        &self,
        (id_user, email_user): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if id_user.status.found() {
            return Err(UserError::IdCollision(self.user_id));
        }

        // TODO(gabriel) do we want to allow deleted users to create new accounts with the same email?
        if email_user.status.valid() {
            return Err(UserError::EmailConflict(self.email.clone()));
        }

        Ok(vec![AuthnEvent::UserCreated {
            user_id: self.user_id,
            email: self.email.clone(),
            webauthn_uuid: self.webauthn_uuid,
            authority: self.authority,
            timestamp: self.timestamp,
        }])
    }
}

pub struct DeleteUser {
    user_id: UserId,
    authority: Authority,
    timestamp: Timestamp,
}

impl DeleteUser {
    #[expect(unused)]
    fn new(user_id: UserId, authority: Authority, timestamp: Timestamp) -> Self {
        Self {
            user_id,
            authority,
            timestamp,
        }
    }
}

impl Decision for DeleteUser {
    type Event = AuthnEvent;
    type StateQuery = User;
    type Error = UserError;

    fn state_query(&self) -> Self::StateQuery {
        User::new(self.user_id)
    }

    fn process(&self, user: &Self::StateQuery) -> Result<Vec<Self::Event>, Self::Error> {
        if !user.status.valid() {
            return Err(UserError::UserDoesntExist(self.user_id));
        }

        Ok(vec![AuthnEvent::UserDeleted {
            user_id: self.user_id,
            authority: self.authority,
            timestamp: self.timestamp,
        }])
    }
}

use crate::authn::UserId;
pub(crate) use crate::authn::error::UserError;
use crate::authn::event::{AuthnEvent, UserEvent};
use crate::authority::Authority;
use crate::status::Status;
use crate::time::Timestamp;
use webauthn_rs::prelude::Uuid;

/// The list of dev user emails (stable across restarts).
pub static DEV_USERS: LazyLock<HashMap<Email, (UserId, Uuid)>> = LazyLock::new(|| {
    let mut map = HashMap::new();

    map.insert(
        Email::try_new("pacioli@monkesto.com").expect("valid dev email"),
        (
            UserId::from_str("zk8m3p5q7r2n4v6x").expect("valid dev id"),
            Uuid::parse_str("a1b2c3d4-e5f6-4a5b-8c9d-0e1f2a3b4c5d").expect("valid dev uuid"),
        ),
    );

    map.insert(
        Email::try_new("wedgwood@monkesto.com").expect("valid dev email"),
        (
            UserId::from_str("yj7l2o4p6q8s0u1w").expect("valid dev id"),
            Uuid::parse_str("b2c3d4e5-f6a7-5b6c-9d0e-1f2a3b4c5d6e").expect("valid dev uuid"),
        ),
    );

    map
});
