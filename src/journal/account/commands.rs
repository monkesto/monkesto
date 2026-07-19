use crate::BackendType;
use crate::StateType;
use crate::auth::get_user;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::event_id::GetEventId;
use crate::journal::JournalId;
use crate::journal::account::{AccountId, CreateAccount};
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
pub struct CreateAccountForm {
    account_name: String,
}

pub async fn create_account(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<CreateAccountForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/account", id);

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

    let user = get_user(session)?;

    let name = Name::try_new(form.account_name).or_redirect(callback_url)?;

    let event_id = state
        .journal_service
        .decision_maker
        .make(CreateAccount::new(
            AccountId::new(),
            journal_id,
            name,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        ))
        .await
        .or_redirect(callback_url)?
        .event_id();

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(callback_url))
}
