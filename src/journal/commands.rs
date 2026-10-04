use crate::BackendType;
use crate::StateType;
use crate::authn::{UserId, get_user};
use crate::authority::{Actor, Authority};
use crate::email::Email;
use crate::journal::error::JournalError;
use crate::journal::{JournalId, Permissions};
use crate::name::Name;
use crate::time::{DefaultTimeProvider, TimeProvider};
use axum::extract::Path;
use axum::extract::State;
use axum::response::Redirect;
use axum_extra::extract::Form;
use axum_login::AuthSession;
use serde::Deserialize;
use std::str::FromStr;

#[derive(Deserialize)]
pub struct CreateJournalForm {
    journal_name: String,
}
pub async fn create_journal(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Form(form): Form<CreateJournalForm>,
) -> Result<Redirect, JournalError> {
    let user = get_user(session)?;

    let name = Name::try_new(form.journal_name)?;

    let event_id = state
        .journal_service
        .create_journal(
            JournalId::new(),
            user.id,
            name,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        )
        .await?;

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to("/journal"))
}

#[derive(Deserialize)]
pub struct InviteUserForm {
    email: String,
    pub read: Option<String>,
    pub add_account: Option<String>,
    pub append_transaction: Option<String>,
    pub invite: Option<String>,
}

pub async fn invite_member(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<InviteUserForm>,
) -> Result<Redirect, JournalError> {
    let email = Email::try_new(form.email)?;

    let user = get_user(session)?;

    let journal_id = JournalId::from_str(&id)?;

    let mut invitee_permissions = Permissions::empty();
    if form.read.is_some() {
        invitee_permissions.insert(Permissions::READ);
    }
    if form.add_account.is_some() {
        invitee_permissions.insert(Permissions::ADD_ACCOUNT);
    }
    if form.append_transaction.is_some() {
        invitee_permissions.insert(Permissions::APPEND_TRANSACTION);
    }
    if form.invite.is_some() {
        invitee_permissions.insert(Permissions::INVITE);
    }

    let invitee_id = state.authn_service.lookup_user_id(&email).await?;

    let event_id = state
        .journal_service
        .add_member(
            journal_id,
            invitee_id,
            invitee_permissions,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        )
        .await?;

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(&format!("/journal/{}/person", id)))
}

#[derive(Deserialize)]
pub struct UpdatePermissionsForm {
    pub read: Option<String>,
    pub add_account: Option<String>,
    pub append_transaction: Option<String>,
    pub invite: Option<String>,
}

pub async fn update_permissions(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((id, person_id)): Path<(String, String)>,
    Form(form): Form<UpdatePermissionsForm>,
) -> Result<Redirect, JournalError> {
    let user = get_user(session)?;
    let journal_id = JournalId::from_str(&id)?;
    let target_user_id = UserId::from_str(&person_id)?;

    let mut new_permissions = Permissions::empty();
    if form.read.is_some() {
        new_permissions.insert(Permissions::READ);
    }
    if form.add_account.is_some() {
        new_permissions.insert(Permissions::ADD_ACCOUNT);
    }
    if form.append_transaction.is_some() {
        new_permissions.insert(Permissions::APPEND_TRANSACTION);
    }
    if form.invite.is_some() {
        new_permissions.insert(Permissions::INVITE);
    }

    let event_id = state
        .journal_service
        .update_member(
            journal_id,
            target_user_id,
            new_permissions,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        )
        .await?;

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(&format!(
        "/journal/{}/person/{}",
        id, person_id
    )))
}

pub async fn remove_member(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((id, person_id)): Path<(String, String)>,
) -> Result<Redirect, JournalError> {
    let user = get_user(session)?;
    let journal_id = JournalId::from_str(&id)?;
    let target_user_id = UserId::from_str(&person_id)?;

    let event_id = state
        .journal_service
        .remove_member(
            journal_id,
            target_user_id,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        )
        .await?;

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(&format!(
        "/journal/{}/person/{}",
        id, person_id
    )))
}
