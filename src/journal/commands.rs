use crate::ident::JournalId;
use crate::journal::Permissions;
use axum_login::AuthSession;

use crate::BackendType;
use crate::StateType;
use crate::auth::user::Email;
use crate::auth::user::{self};
use crate::authority::UserId;
use crate::known_errors::KnownErrors;
use crate::known_errors::RedirectOnError;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Redirect;
use axum_extra::extract::Form;
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

    let user = user::get_user(session)?;

    if form.journal_name.trim().is_empty() {
        return Err(KnownErrors::InvalidInput.redirect("/journal"));
    }

    state
        .journal_service
        .journal_create(JournalId::new(), form.journal_name, user.id)
        .await
        .or_redirect(CALLBACK_URL)?;

    Ok(Redirect::to(CALLBACK_URL))
}

#[derive(Deserialize)]
pub struct InviteUserForm {
    email: String,
    pub read: Option<String>,
    pub addaccount: Option<String>,
    pub appendtransaction: Option<String>,
    pub invite: Option<String>,
    pub delete: Option<String>,
}

pub async fn invite_user(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<InviteUserForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person", id);

    let email = Email::try_new(form.email)
        .map_err(|_| KnownErrors::UserDoesntExist)
        .or_redirect(callback_url)?;

    let user = user::get_user(session)?;

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

    let mut invitee_permissions = Permissions::empty();
    if form.read.is_some() {
        invitee_permissions.insert(Permissions::READ);
    }
    if form.addaccount.is_some() {
        invitee_permissions.insert(Permissions::ADDACCOUNT);
    }
    if form.appendtransaction.is_some() {
        invitee_permissions.insert(Permissions::APPENDTRANSACTION);
    }
    if form.invite.is_some() {
        invitee_permissions.insert(Permissions::INVITE);
    }
    if form.delete.is_some() {
        invitee_permissions.insert(Permissions::DELETE);
    }

    state
        .journal_service
        .journal_invite_tenant(journal_id, user.id, email, invitee_permissions)
        .await
        .or_redirect(callback_url)?;

    Ok(Redirect::to(callback_url))
}

#[derive(Deserialize)]
pub struct UpdatePermissionsForm {
    pub read: Option<String>,
    pub addaccount: Option<String>,
    pub appendtransaction: Option<String>,
    pub invite: Option<String>,
    pub delete: Option<String>,
}

pub async fn update_permissions(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((id, person_id)): Path<(String, String)>,
    Form(form): Form<UpdatePermissionsForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person/{}", id, person_id);

    let user = user::get_user(session)?;
    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;
    let target_user_id = UserId::from_str(&person_id).or_redirect(callback_url)?;

    let mut new_permissions = Permissions::empty();
    if form.read.is_some() {
        new_permissions.insert(Permissions::READ);
    }
    if form.addaccount.is_some() {
        new_permissions.insert(Permissions::ADDACCOUNT);
    }
    if form.appendtransaction.is_some() {
        new_permissions.insert(Permissions::APPENDTRANSACTION);
    }
    if form.invite.is_some() {
        new_permissions.insert(Permissions::INVITE);
    }
    if form.delete.is_some() {
        new_permissions.insert(Permissions::DELETE);
    }

    state
        .journal_service
        .journal_update_tenant_permissions(journal_id, target_user_id, new_permissions, user.id)
        .await
        .or_redirect(callback_url)?;

    Ok(Redirect::to(callback_url))
}

#[derive(Deserialize)]
pub struct CreateSubJournalForm {
    subjournal_name: String,
}

pub async fn create_sub_journal(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<CreateSubJournalForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = format!("/journal/{}/subjournals", id);

    let user = user::get_user(session)?;

    if form.subjournal_name.trim().is_empty() {
        return Err(KnownErrors::InvalidInput.redirect(&callback_url));
    }

    let journal_id = JournalId::from_str(&id).or_redirect(&callback_url)?;

    state
        .journal_service
        .journal_create_subjournal(journal_id, form.subjournal_name, user.id)
        .await
        .or_redirect(&callback_url)?;

    Ok(Redirect::to(&callback_url))
}

pub async fn remove_tenant(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((id, person_id)): Path<(String, String)>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person", id);
    let person_detail_url = &format!("/journal/{}/person/{}", id, person_id);

    let user = user::get_user(session)?;
    let journal_id = JournalId::from_str(&id).or_redirect(person_detail_url)?;
    let target_user_id = UserId::from_str(&person_id).or_redirect(person_detail_url)?;

    state
        .journal_service
        .journal_remove_tenant(journal_id, target_user_id, user.id)
        .await
        .or_redirect(callback_url)?;

    Ok(Redirect::to(callback_url))
}
