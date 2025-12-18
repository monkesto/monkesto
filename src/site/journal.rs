use super::nav::TopBar;
use crate::event_sourcing::journal::Permissions;
use leptos::prelude::*;

#[component]
pub fn JournalList() -> impl IntoView {
    view! {
        <div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8">
            <div class="sm:mx-auto sm:w-full sm:max-w-sm">
                <img src="logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto" />
                <h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white">
                    Journals
                </h2>
            </div>
        </div>
    }
}

#[component]
pub fn GeneralJournal() -> impl IntoView {
    use crate::api::main_api::{
        get_associated_journals, get_transactions, get_user_id_from_session,
    };
    use leptos::either::EitherOf5;

    let user_id_resource =
        Resource::new(|| (), |_| async move { get_user_id_from_session().await });

    let journals_resource = Resource::new(
        || (),
        move |_| async move { get_associated_journals(user_id_resource.await).await },
    );

    let transactions_resource = Resource::new(
        || (),
        move |_| async move { get_transactions(user_id_resource.await, journals_resource.await).await },
    );

    view! {
        <Suspense>
            {move || Suspend::new(async move {
                let user_id = match user_id_resource.await {
                    Ok(s) => s,
                    Err(_) => {
                        return EitherOf5::A(
                            view! { <meta http-equiv="refresh" content="0; url=/login" /> },
                        );
                    }
                };
                let journals = match journals_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return EitherOf5::B(

                            view! {
                                <p>"An error occured while fetching journals: "{e.to_string()}</p>
                            },
                        );
                    }
                };
                if journals.selected.is_none() {
                    return EitherOf5::C(

                        view! {
                            <TopBar journals=journals user_id=user_id />
                            <h1 class="font-bold text-4xl">"please select a journal"</h1>
                        },
                    );
                }
                let mut transactions = match transactions_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return EitherOf5::D(

                            view! {
                                <p>
                                    "An error occured while fetching transactions: "{e.to_string()}
                                </p>
                            },
                        );
                    }
                };
                EitherOf5::E(

                    view! {
                        <TopBar journals=journals user_id=user_id />
                        <div class="flex flex-col items-center text-center px-10 py-10">
                            <h1 class="font-bold text-4xl">"General Journal"</h1>
                            <ul>
                                {
                                    transactions
                                        .sort_unstable_by_key(|transaction| std::cmp::Reverse(
                                            transaction.timestamp,
                                        ));
                                    transactions
                                        .into_iter()
                                        .map(|transaction| {
                                            view! {
                                                <div class="flex flex-col items-center text-center px-10 py-10">
                                                    <li>
                                                        <h2 class="font-bold text-xl">
                                                            {transaction
                                                                .timestamp
                                                                .with_timezone(&chrono_tz::America::Chicago)
                                                                .format("%Y-%m-%d %H:%M:%S %Z")
                                                                .to_string()}" by "{transaction.transaction.author}":"
                                                        </h2>
                                                        <br />
                                                        <ul>
                                                            {transaction
                                                                .transaction
                                                                .updates
                                                                .into_iter()
                                                                .map(|update| {
                                                                    view! {
                                                                        <li>
                                                                            {update.account_name} " : $"
                                                                            {format!(
                                                                                "{}.{:02} {}",
                                                                                update.changed_by.abs() / 100,
                                                                                update.changed_by.abs() % 100,
                                                                                if update.changed_by < 0 { "Dr" } else { "Cr" },
                                                                            )}
                                                                        </li>
                                                                    }
                                                                })
                                                                .collect_view()}
                                                        </ul>
                                                    </li>
                                                </div>
                                            }
                                        })
                                        .collect_view()
                                }
                            </ul>
                        </div>
                    },
                )
            })}
        </Suspense>
    }
}

#[component]
pub fn JournalInvites() -> impl IntoView {
    use crate::api::main_api::{
        InviteToJournal, RespondToJournalInvite, get_associated_journals, get_journal_invites,
        get_user_id_from_session,
    };
    use leptos::either::{Either, EitherOf4};

    let invite_action = ServerAction::<InviteToJournal>::new();
    let response_action = ServerAction::<RespondToJournalInvite>::new();

    let user_id_resource =
        Resource::new(|| (), |_| async move { get_user_id_from_session().await });

    let journals_resource = Resource::new(
        || (),
        move |_| async move { get_associated_journals(user_id_resource.await).await },
    );

    let invites_resource = Resource::new(
        || (),
        move |_| async move { get_journal_invites(user_id_resource.await).await },
    );

    view! {
        <Suspense>
            {move || Suspend::new(async move {
                let user_id = match user_id_resource.await {
                    Ok(s) => s,
                    Err(_) => {
                        return EitherOf4::A(
                            view! { <meta http-equiv="refresh" content="0; url=/login" /> },
                        );
                    }
                };
                let journals = match journals_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return EitherOf4::B(

                            view! {
                                <p>"An error occured while fetching journals: "{e.to_string()}</p>
                            },
                        );
                    }
                };
                let invites = match invites_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return EitherOf4::C(

                            view! {
                                <p>"An error occured while fetching invites: "{e.to_string()}</p>
                            },
                        );
                    }
                };
                EitherOf4::D(

                    view! {
                        <TopBar user_id=user_id journals=journals.clone() />

                        {if let Some(selected) = journals.selected {
                            Either::Left(
                                view! {
                                    <ActionForm action=invite_action>
                                        <input
                                            type="hidden"
                                            name="journal_id"
                                            value=selected.get_id().to_string()
                                        />

                                        <input
                                            type="hidden"
                                            name="own_id"
                                            value=user_id.to_string()
                                        />

                                        <input
                                            type="hidden"
                                            name="permissions"
                                            // TODO: Add a selector for permissions
                                            value=serde_json::to_string(&Permissions::all())
                                                .expect("serialization of permissions failed")
                                        />

                                        <input
                                            class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                            type="text"
                                            name="invitee_username"
                                            placeholder="johndoe"
                                            required
                                        />

                                        <button class="mt-3 rounded bg-purple-900 px-10 py-2 font-bold text-white hover:bg-blue-400">
                                            "Invite to "{selected.get_name()} "!"
                                        </button>

                                    </ActionForm>

                                    {if let Some(Err(e)) = invite_action.value().get() {
                                        Either::Left(
                                            view! {
                                                <p>
                                                    "An error occured while creating the invitation: "
                                                    {e.to_string()}
                                                </p>
                                            },
                                        )
                                    } else {
                                        Either::Right(view! { "" })
                                    }}
                                },
                            )
                        } else {
                            Either::Right(view! { "" })
                        }}

                        {invites
                            .into_iter()
                            .map(|invite| {
                                view! {
                                    <h2 class="block mb-2 font-xl">{invite.name}</h2>
                                    <div class="flex">
                                        <ActionForm action=response_action>

                                            <input
                                                type="hidden"
                                                name="user_id"
                                                value=user_id.to_string()
                                            />

                                            <input
                                                type="hidden"
                                                name="journal_id"
                                                value=invite.id.to_string()
                                            />

                                            <input
                                                type="hidden"
                                                name="accepted"
                                                value=serde_json::to_string(&true)
                                                    .expect("failed to serialize true")
                                            />

                                            <button
                                                type="submit"
                                                class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400"
                                            >
                                                "Accept"
                                            </button>

                                        </ActionForm>
                                        <ActionForm action=response_action>
                                            <input
                                                type="hidden"
                                                name="user_id"
                                                value=user_id.to_string()
                                            />

                                            <input
                                                type="hidden"
                                                name="journal_id"
                                                value=invite.id.to_string()
                                            />

                                            <input
                                                type="hidden"
                                                name="accepted"
                                                value=serde_json::to_string(&false)
                                                    .expect("failed to serialize true")
                                            />

                                            <button
                                                type="submit"
                                                class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400"
                                            >
                                                "Decline"
                                            </button>
                                        </ActionForm>
                                    </div>
                                }
                            })
                            .collect_view()}

                        {if let Some(Err(e)) = response_action.value().get() {
                            Either::Left(view! { <p>"An error occured: "{e.to_string()}</p> })
                        } else {
                            Either::Right(view! { "" })
                        }}
                    },
                )
            })}
        </Suspense>
    }
}
