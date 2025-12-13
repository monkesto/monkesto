use crate::api::return_types::*;
use leptos::prelude::*;

#[component]
pub fn ClientLogin() -> impl IntoView {
    use crate::api::main_api::{Login, get_user_id_from_session};
    use leptos::either::{Either, EitherOf3};

    let login = ServerAction::<Login>::new();
    let logged_in = Resource::new(|| (), |_| async { get_user_id_from_session().await }); // this throws an error if the database can't find an account associated with the session

    view! {
        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <Suspense>
                // redirect to the homepage if the user's session id is already associated with an account
                {move || match logged_in.get() {
                    Some(Ok(_)) => {
                        EitherOf3::A(
                            view! {
                                <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
                                    <meta http-equiv="refresh" content="0; url=/" />
                                </div>
                            },
                        )
                    }
                    Some(Err(_)) => {
                        EitherOf3::B(
                            view! {
                                <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
                                    <br />

                                    <ActionForm action=login>

                                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">

                                            <input
                                                class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                                type="text"
                                                name="username"
                                                placeholder="username"
                                                value=move || match login.value().get() {
                                                    Some(Err(e)) => {
                                                        match KnownErrors::parse_error(e) {
                                                            Some(KnownErrors::LoginFailed { username }) => username,
                                                            _ => "".to_string(),
                                                        }
                                                    }
                                                    _ => "".to_string(),
                                                }
                                                required
                                            />
                                            <br />
                                            <input
                                                class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                                type="password"
                                                name="password"
                                                placeholder="password"
                                                required
                                            />
                                            <br />
                                            <button
                                                class="mt-3 rounded bg-purple-900 px-10 py-2 font-bold text-white hover:bg-blue-400"
                                                type="submit"
                                            >
                                                "Login"
                                            </button>
                                        </div>
                                    </ActionForm>
                                    <a
                                        href="/signup"
                                        class="mt-3 rounded bg-purple-900 px-10 py-2 font-bold text-white hover:bg-blue-400"
                                        type="submit"
                                    >
                                        "Don't have an account? Sign up"
                                    </a>
                                </div>
                            },
                        )
                    }
                    None => EitherOf3::C(view! { "" }),
                }}
            </Suspense>
            {move || match login.value().get() {
                Some(Err(e)) => {
                    Either::Left(
                        view! {
                            <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
                                <p>{e.to_string()}</p>
                            </div>
                        },
                    )
                }
                _ => Either::Right(view! { "" }),
            }}
        </div>
    }
}

#[component]
pub fn ClientSignUp() -> impl IntoView {
    use crate::api::main_api::CreateUser;
    use leptos::either::Either;
    let signup = ServerAction::<CreateUser>::new();

    view! {
        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <Suspense>
                <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">

                    <br />

                    <ActionForm action=signup>

                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">

                            <input
                                class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                type="text"
                                name="username"
                                placeholder="username"
                                required
                            />
                            <br />
                            <input
                                class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                type="password"
                                name="password"
                                placeholder="password"
                                required
                            />
                            <br />
                            <input
                                class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                type="password"
                                name="confirm_password"
                                placeholder="confirm password"
                                required
                            />
                            <br />
                            <button
                                class="mt-3 rounded bg-purple-900 px-10 py-2 font-bold text-white hover:bg-blue-400"
                                type="submit"
                            >
                                "Sign up"
                            </button>
                        </div>
                    </ActionForm>
                    <a
                        href="/login"
                        class="mt-3 rounded bg-purple-900 px-10 py-2 font-bold text-white hover:bg-blue-400"
                        type="submit"
                    >
                        "Have an account? Sign in"
                    </a>
                </div>

            </Suspense>

            {move || match signup.value().get() {
                Some(Err(e)) => {
                    Either::Left(
                        view! {
                            <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
                                <p>{e.to_string()}</p>
                            </div>
                        },
                    )
                }
                _ => Either::Right(view! { "" }),
            }}
        </div>
    }
}
