use super::RoleState;
use crate::BackendType;
use crate::StateType;
use crate::auth::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::layout::layout;
use crate::monkesto_error::OrRedirect;
use crate::name::Name;
use axum::Router;
use axum::extract::State;
use axum::response::Redirect;
use axum::routing::get;
use axum_extra::extract::Form;
use axum_login::AuthSession;
use maud::{Markup, html};
use serde::Deserialize;

pub fn router() -> Router<StateType> {
    Router::new().route("/authz/roles", get(roles_page).post(create_role))
}

async fn roles_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
) -> Result<Markup, Redirect> {
    let _user = get_user(session)?;

    let roles = state.authz_service.all_roles().await.unwrap_or_default();

    let content = html! {
        div class="mx-auto flex w-full max-w-4xl flex-col gap-6" {
            div class="flex items-center justify-between" {
                h1 class="text-xl font-semibold text-gray-900 dark:text-gray-100" {
                    "Authorization Roles"
                }
            }

            div class="grid grid-cols-1 gap-3" {
                @if roles.is_empty() {
                    p class="text-sm text-gray-500 dark:text-gray-400" {
                        "No roles have been created."
                    }
                } @else {
                    @for (role_id, role) in roles {
                        @match role {
                            RoleState::Present { name, actors, .. } => {
                                div class="rounded-md border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-800" {
                                    div class="flex items-start justify-between gap-4" {
                                        div {
                                            h2 class="text-base font-semibold text-gray-900 dark:text-gray-100" {
                                                (name)
                                            }
                                            p class="mt-1 text-xs text-gray-500 dark:text-gray-400" {
                                                (role_id)
                                            }
                                        }
                                        span class="text-sm text-gray-600 dark:text-gray-300" {
                                            (actors.len()) " actor" @if actors.len() != 1 { "s" }
                                        }
                                    }
                                }
                            }
                            RoleState::Absent => {}
                        }
                    }
                }
            }

            form action="/authz/roles" method="post" class="space-y-4 border-t border-gray-200 pt-6 dark:border-gray-700" {
                h2 class="text-base font-semibold text-gray-900 dark:text-gray-100" {
                    "Create Role"
                }
                div {
                    label
                    for="role_name"
                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                        "Name"
                    }
                    div class="mt-2" {
                        input
                        id="role_name"
                        type="text"
                        name="role_name"
                        required
                        class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                        ;
                    }
                }
                button
                type="submit"
                class="rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                    "Create"
                }
            }
        }
    };

    Ok(layout(Some("Authorization"), true, None, content))
}

#[derive(Deserialize)]
struct CreateRoleForm {
    role_name: String,
}

async fn create_role(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Form(form): Form<CreateRoleForm>,
) -> Result<Redirect, Redirect> {
    const CALLBACK_URL: &str = "/authz/roles";

    let user = get_user(session)?;
    let name = Name::try_new(form.role_name).or_redirect(CALLBACK_URL)?;

    state
        .authz_service
        .create_role(Authority::Direct(Actor::User(user.id)), name)
        .await
        .map_err(|_| Redirect::to(CALLBACK_URL))?;

    Ok(Redirect::to(CALLBACK_URL))
}
