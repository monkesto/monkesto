use leptos::{
    IntoView,
    prelude::{CustomAttribute, ElementChild, IntoAny, ServerFnError},
    view,
};

use crate::api::return_types::KnownErrors;

#[expect(non_snake_case)]
pub fn HandleError(err: ServerFnError, context: &str) -> impl IntoView {
    use KnownErrors::*;
    if let Some(e) = KnownErrors::parse_error(&err) {
        match e {
            NotLoggedIn => {
                view! { <meta http-equiv="refresh" content="0; url=/login" /> }.into_any()
            }

            _ => view! {
                <p>
                    "An error occurred while " {context} ":"
                    {e.to_string().unwrap_or("failed to encode error".to_string())}
                </p>
            }
            .into_any(),
        }
    } else {
        view! { <p>"An unknown error occurred: " {err.to_string()}</p> }.into_any()
    }
}
