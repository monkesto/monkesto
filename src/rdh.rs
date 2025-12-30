use crate::auth;
use axum::extract::Form;
use axum::extract::Query;
use axum::response::IntoResponse;
use axum::response::Redirect;
use leptos::prelude::*;
use serde::Deserialize;

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

async fn interpolated_response(name: String, age: i32, awesomeness: f32) -> impl IntoView {
    let user_id = match auth::user::get_user_id_from_session().await {
        Ok(s) => s.to_string(),
        Err(e) => e.to_string(),
    };

    let name_clone = name.clone();

    view! {
        <h1>"Hello, " {name} "!"</h1>
        <p>"Nice to meet you, " {name_clone} ". You are " {age} " years old."</p>
        <p>"I am " {awesomeness} "% confident that you are awesome!"</p>
        <p>"Your user_id is: " {user_id}</p>
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

pub async fn show_result(Query(query): Query<NameForm>) -> impl IntoResponse {
    let html = interpolated_response(query.name, query.age, query.awesomeness)
        .await
        .to_html();
    axum::response::Html(html)
}
