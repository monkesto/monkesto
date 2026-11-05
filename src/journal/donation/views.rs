use crate::authn::user::UserState;
use crate::authn::{UserId, get_user};
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::email::Email;
use crate::journal::donation::DonationState;
use crate::journal::donation::donor::{DonorId, DonorState};
use crate::journal::error::{JournalError, JournalResult};
use crate::journal::fund::{FundId, FundState};
use crate::journal::{JournalId, layout};
use crate::time::Timestamp;
use crate::{BackendType, StateType};
use axum::extract::{Path, State};
use axum_login::AuthSession;
use maud::{Markup, html};
use std::collections::HashMap;
use std::str::FromStr;

pub async fn donation_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
) -> Result<Markup, JournalError> {
    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));

    let journal_id = JournalId::from_str(&id)?;

    let donations_res: JournalResult<Vec<(DonationState, Authority, Timestamp)>> = state
        .journal_service
        .list_journal_donations(journal_id, user_authority)
        .await;

    let funds_res: JournalResult<HashMap<FundId, FundState>> = match state
        .journal_service
        .list_journal_funds(journal_id, user_authority)
        .await
    {
        Ok(funds) => Ok(funds
            .into_iter()
            .map(|(state, _, _)| (state.id, state))
            .collect::<HashMap<FundId, FundState>>()),
        Err(e) => Err(e),
    };

    let donors_res: JournalResult<HashMap<DonorId, DonorState>> = match state
        .journal_service
        .list_journal_donors(journal_id, user_authority)
        .await
    {
        Ok(donors) => Ok(donors
            .into_iter()
            .map(|(state, _, _)| (state.id, state))
            .collect()),
        Err(e) => Err(e),
    };

    let members_res: JournalResult<HashMap<UserId, UserState>> = match state
        .journal_service
        .list_journal_members(journal_id, user_authority)
        .await
    {
        Ok(ids) => match state.authn_service.fetch_users(ids.as_slice()).await {
            Ok(members) => Ok(members
                .into_iter()
                .map(|m| (m.id, m))
                .collect::<HashMap<UserId, UserState>>()),
            Err(e) => Err(e.into()),
        },
        Err(e) => Err(e),
    };

    let mut nonmember_cache: HashMap<UserId, Email> = HashMap::new();

    let content = html! {
          @match donations_res {
            Ok(ref donations) => {
                @for (donations, authority, timestamp) in donations {
                    a
                    href=(format!("/journal/{}/donation/{}", id, donations.id))
                    class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"{
                        div class="space-y-3" {
                            div class="space-y-2" {
                                @for entry in donations.entries.iter() {
                                    div class="flex justify-between items-center" {
                                        span class="text-base font-medium text-gray-900 dark:text-white" {
                                            @match &funds_res {
                                                Ok(accounts) => (accounts.get(&entry.fund_id).map(|fund| fund.name.as_ref()).unwrap_or("Unknown Fund")),
                                                Err(e) => {"encountered an error while fetching accounts: " (e)}
                                            }
                                        }

                                        span class="text-base text-gray-700 dark:text-gray-300" {
                                            @match &donors_res {
                                                Ok(donors) => (donors.get(&entry.donor_id).map(|d| d.name.as_ref()).unwrap_or("Unknown Donor")),
                                                Err(e) => {"encountered an error while fetching donors: " (e)}
                                            }
                                        }

                                        span class="text-base text-gray-700 dark:text-gray-300" {
                                            (Dollars(entry.amount as i64))
                                        }

                                    }
                                }

                                div class="text-xs text-gray-400 dark:text-gray-500" {
                                    @match authority.actor() {
                                        Actor::User(id) => {
                                            @match &members_res {
                                                Ok(members) => {
                                                    @if let Some(email) = members.get(&id).map(|m| m.email.clone()) {
                                                        (email.to_string())
                                                    } @else if let Some(email) = nonmember_cache.get(&id)  {
                                                        (email.to_string())
                                                    } @else {
                                                        // the user may be the owner or somebody who left the journal after creating the donation
                                                        @match state.authn_service.fetch_user(id).await {
                                                            Ok(user) => {
                                                                // maud assumes that you never want to call functions for
                                                                // side effects and makes you assign a value to the result
                                                                @let _ = nonmember_cache.insert(user.id, user.email.clone());
                                                                (user.email.to_string())
                                                            },
                                                            Err(e) => {"failed to fetch user: " (e)}
                                                        }
                                                    }
                                                },
                                                Err(e) => {"failed to fetch users: " (e)}
                                            }
                                        },
                                        Actor::System => {"system"},
                                        Actor::Anonymous => {"anonymous"}
                                    } " on " ((timestamp.format("%m/%d/%Y")))
                                }
                            }
                        }
                    }
                }
            },
            Err(e) => {
                "Error fetching transactions" (e)
            }
          }
    };

    let wrapped_content = html! {
        div class="flex flex-col gap-6 mx-auto w-full max-w-4xl" {
            (content)
        }
    };

    let journal_name = match state
        .journal_service
        .get_journal(journal_id, user_authority)
        .await
    {
        Ok((journal, _, _)) => journal.name.to_string(),
        Err(e) => format!("failed to fetch the journal: {e}"),
    };

    Ok(layout::layout(
        Some(&journal_name),
        true,
        Some(&id),
        wrapped_content,
    ))
}
