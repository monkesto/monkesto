use crate::BackendType;
use crate::StateType;
use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::JournalId;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::error::JournalError;
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
pub struct CreateAccountForm {
    account_name: String,
    account_type: i8,
}

pub async fn create_account(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<CreateAccountForm>,
) -> Result<Redirect, JournalError> {
    let journal_id = JournalId::from_str(&id)?;

    let user = get_user(session)?;

    let name = Name::try_new(form.account_name)?;
    let account_type = AccountType::try_from(form.account_type)?;

    let event_id = state
        .journal_service
        .create_account(
            AccountId::new(),
            journal_id,
            name,
            account_type,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        )
        .await?;

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(&format!("/journal/{}/account", id)))
}
