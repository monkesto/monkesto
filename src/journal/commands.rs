use crate::BackendType;
use crate::StateType;
use crate::auth::get_user;
use crate::auth::user::UserId;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::email::Email;
use crate::event_id::GetEventId;
use crate::journal::member::{AddJournalMember, RemoveJournalMember, UpdateJournalMember};
use crate::journal::{CreateJournal, JournalId, Permissions};
use crate::monkesto_error::OrRedirect;
use crate::name::Name;
use crate::time_provider::{DefaultTimeProvider, TimeProvider};
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
) -> Result<Redirect, Redirect> {
    const CALLBACK_URL: &str = "/journal";

    let user = get_user(session)?;

    let name = Name::try_new(form.journal_name).or_redirect(CALLBACK_URL)?;

    let event_id = state
        .journal_service
        .decision_maker
        .make(CreateJournal::new(
            JournalId::new(),
            user.id,
            name,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        ))
        .await
        .or_redirect(CALLBACK_URL)?
        .event_id();

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(CALLBACK_URL))
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
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person", id);

    let email = Email::try_new(form.email).or_redirect(callback_url)?;

    let user = get_user(session)?;

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

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

    let invitee_id = state
        .auth_service
        .lookup_user_id(&email)
        .await
        .or_redirect(callback_url)?;

    let event_id = state
        .journal_service
        .decision_maker
        .make(AddJournalMember::new(
            journal_id,
            invitee_id,
            invitee_permissions,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        ))
        .await
        .or_redirect(callback_url)?
        .event_id();

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(callback_url))
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
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person/{}", id, person_id);

    let user = get_user(session)?;
    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;
    let target_user_id = UserId::from_str(&person_id).or_redirect(callback_url)?;

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
        .decision_maker
        .make(UpdateJournalMember::new(
            journal_id,
            target_user_id,
            new_permissions,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        ))
        .await
        .or_redirect(callback_url)?
        .event_id();

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(callback_url))
}

pub async fn remove_member(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((id, person_id)): Path<(String, String)>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person", id);
    let person_detail_url = &format!("/journal/{}/person/{}", id, person_id);

    let user = get_user(session)?;
    let journal_id = JournalId::from_str(&id).or_redirect(person_detail_url)?;
    let target_user_id = UserId::from_str(&person_id).or_redirect(person_detail_url)?;

    let event_id = state
        .journal_service
        .decision_maker
        .make(RemoveJournalMember::new(
            journal_id,
            target_user_id,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        ))
        .await
        .or_redirect(callback_url)?
        .event_id();

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(callback_url))
}
