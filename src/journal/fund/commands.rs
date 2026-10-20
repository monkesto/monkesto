use crate::BackendType;
use crate::StateType;
use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::JournalId;
use crate::journal::error::JournalError;
use crate::journal::fund::FundId;
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
    fund_name: String,
}

pub async fn create_fund(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<CreateAccountForm>,
) -> Result<Redirect, JournalError> {
    let journal_id = JournalId::from_str(&id)?;

    let user = get_user(session)?;

    let name = Name::try_new(form.fund_name)?;

    let event_id = state
        .journal_service
        .create_fund(
            FundId::new(),
            journal_id,
            name,
            Authority::Direct(Actor::User(user.id)),
            DefaultTimeProvider.get_time(),
        )
        .await?;

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(&format!("/journal/{}/account", id)))
}
