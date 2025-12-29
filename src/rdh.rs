use axum::extract::Form;
use axum::response::IntoResponse;
use leptos::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct NameForm {
    name: String,
    age: i32,
    awesomeness: f32,
}

#[component]
fn BasicForm() -> impl IntoView {
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

#[component]
fn InterpolatedResponse(name: String, age: i32, awesomeness: f32) -> impl IntoView {
    let name_clone = name.clone();
    view! {
        <h1>"Hello, " {name} "!"</h1>
        <p>"Nice to meet you, " {name_clone} ". You are " {age} " years old."</p>
        <p>"I am " {awesomeness} "% confident that you are awesome!"</p>
        <a href="/rdh">"Start over"</a>
    }
}

pub async fn basic() -> impl IntoResponse {
    let html = BasicForm().to_html();
    axum::response::Html(html)
}

pub async fn interpolated(Form(form): Form<NameForm>) -> impl IntoResponse {
    let html =
        view! { <InterpolatedResponse name=form.name age=form.age awesomeness=form.awesomeness /> }
            .to_html();
    axum::response::Html(html)
}
