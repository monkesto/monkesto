use super::super::nav::TopBar;
use leptos::prelude::*;

#[component]
pub fn GeneralJournal() -> impl IntoView {
    use crate::api::main_api::{
        get_associated_journals, get_transactions, get_user_id_from_session,
    };
    let user_id_resource =
        Resource::new(|| (), |_| async move { get_user_id_from_session().await });

    let journals_resource = Resource::new(
        || (),
        move |_| async move { get_associated_journals().await },
    );

    let transactions_resource = Resource::new(
        || (),
        move |_| async move { get_transactions(journals_resource.await).await },
    );

    view! {
        <Suspense>
            {move || Suspend::new(async move {
                if user_id_resource.await.is_err() {
                    return view! { <meta http-equiv="refresh" content="0; url=/login" /> }
                        .into_any();
                }
                let journals = match journals_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return view! {
                            <p>"An error occurred while fetching journals: "{e.to_string()}</p>
                        }
                            .into_any();
                    }
                };
                if journals.selected.is_none() {
                    return view! {
                        <TopBar journals=journals />
                        <h1 class="font-bold text-4xl">"please select a journal"</h1>
                    }
                        .into_any();
                }
                let mut transactions = match transactions_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return view! {
                            <p>"An error occurred while fetching transactions: "{e.to_string()}</p>
                        }
                            .into_any();
                    }
                };

                view! {
                    <TopBar journals=journals />
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
                }
                    .into_any()
            })}
        </Suspense>
    }
}
