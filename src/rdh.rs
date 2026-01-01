use crate::auth;
use crate::cuid::Cuid;
use crate::extensions;
use axum::Extension;
use axum::extract::Form;
use axum::extract::Query;
use axum::response::IntoResponse;
use axum::response::Redirect;
use leptos::prelude::*;
use maud::Markup;
use maud::html;
use serde::Deserialize;
use sqlx::PgPool;
use tower_sessions::Session;

#[derive(Deserialize)]
pub struct NameForm {
    name: String,
    age: i32,
    awesomeness: f32,
}

fn basic_form() -> impl IntoView {
    view! {
        <h1>"Hello, World!"</h1>
        <form action="/rdh" method="post">
            <label>"Name: " <input type="text" name="name" required /></label>
            <br />
            <label>"Age: " <input type="number" name="age" required /></label>
            <br />
            <label>
                "Awesomeness (%): "
                <input type="number" name="awesomeness" step="0.01" min="0" max="100" required />
            </label>
            <br />
            <input type="submit" value="Submit" />
        </form>
    }
}

async fn interpolated_response(
    name: String,
    age: i32,
    awesomeness: f32,
    user_id: Cuid,
) -> impl IntoView {
    let name_clone = name.clone();

    view! {
        <h1>"Hello, " {name} "!"</h1>
        <p>"Nice to meet you, " {name_clone} ". You are " {age} " years old."</p>
        <p>"I am " {awesomeness} "% confident that you are awesome!"</p>
        <p>"Your user_id is: " {user_id.to_string()}</p>
        <a href="/rdh">"Start over"</a>
    }
}

pub async fn basic() -> impl IntoResponse {
    let html = basic_form().to_html();
    axum::response::Html(html)
}

pub async fn interpolated(Form(form): Form<NameForm>) -> impl IntoResponse {
    let encoded_name = urlencoding::encode(&form.name);
    Redirect::to(&format!(
        "/rdh/result?name={}&age={}&awesomeness={}",
        encoded_name, form.age, form.awesomeness
    ))
}

pub async fn show_result(
    Query(query): Query<NameForm>,
    Extension(db_pool): Extension<PgPool>,
    session: Session,
) -> impl IntoResponse {
    let user_id = auth::get_user_id(
        &extensions::intialize_session(&session)
            .await
            .expect("failed to get session")
            .to_string(),
        &db_pool,
    )
    .await;
    let html = interpolated_response(
        query.name,
        query.age,
        query.awesomeness,
        user_id.expect("failed to get user_id"),
    )
    .await
    .to_html();
    axum::response::Html(html)
}

pub async fn maud_test() -> Markup {
    let v = vec![12, 23, 34, 45, 56];

    html! {
        (maud::DOCTYPE)
        link rel="stylesheet" href="/pkg/monkesto.css";

        div class="bg-white dark:bg-gray-800 border-b border-gray-200
            dark:border-gray-700" {
            p {"test"}

            ul{
                @for item in v {
                    li {(item)}
                }
            }
        }
    }
}
