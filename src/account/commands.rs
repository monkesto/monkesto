use crate::BackendType;
use crate::StateType;
use crate::auth::user::{self};
use crate::authority::Actor;
use crate::authority::Authority;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::monkesto_error::OrRedirect;
use crate::name::Name;
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
    parent_account_id: Option<String>,
}

pub async fn create_account(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<CreateAccountForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/account", id);

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

    let user = user::get_user(session)?;

    let name = Name::try_new(form.account_name).or_redirect(callback_url)?;

    let parent_account_id = form
        .parent_account_id
        .filter(|s| !s.is_empty())
        .map(|s| AccountId::from_str(&s).or_redirect(callback_url))
        .transpose()?;

    state
        .account_service
        .create_account(
            AccountId::new(),
            journal_id,
            &Authority::Direct(Actor::User(user.id)),
            name,
            parent_account_id,
        )
        .await
        .or_redirect(callback_url)?;

    Ok(Redirect::to(callback_url))
}
