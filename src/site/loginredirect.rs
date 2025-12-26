use leptos::{
    IntoView, component,
    prelude::{CustomAttribute, IntoAny, ServerFnError},
    view,
};

use crate::api::return_types::KnownErrors;

#[component]
pub fn LoginRedirect(err: ServerFnError) -> impl IntoView {
    if let Some(KnownErrors::NotLoggedIn) = KnownErrors::parse_error(err) {
        view! { <meta http-equiv="refresh" content="0; url=/login" /> }.into_any()
    } else {
        view! { "" }.into_any()
    }
}
