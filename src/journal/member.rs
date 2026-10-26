use crate::authn::UserId;
use crate::authority::{Actor, Authority};
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::{JournalError, JournalResult};
use crate::journal::event::JournalDomainEvent;
use crate::journal::service::stream_lock::{append_modification_event_in_tx, get_stream_handle};
use crate::journal::service::{
    JournalService, MemberPayload, StreamType, get_latest_member_payload,
};
use crate::journal::{JournalId, PermissionDecodeError, Permissions};
use crate::time::Timestamp;

impl JournalService {
    pub async fn add_member(
        &self,
        journal_id: JournalId,
        user_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<i64> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::INVITE)
            .await?;

        let stream_handle =
            get_stream_handle(&mut tx, journal_id, *journal_id, StreamType::Journal).await?;

        let member_payload = get_latest_member_payload(&mut tx, journal_id, user_id).await?;

        if let Some(payload) = member_payload {
            match payload {
                MemberPayload::AddedToJournal(..)
                | MemberPayload::PermissionsUpdated(..)
                | MemberPayload::CreatedJournal => {
                    return Err(JournalError::UserAlreadyHasAccess(user_id));
                }

                MemberPayload::RemovedFromJournal => {
                    // allow re-invitation of removed users
                }
            }
        }

        let event_id = append_modification_event_in_tx(
            &mut tx,
            stream_handle,
            JournalDomainEvent::MemberAdded {
                journal_id,
                user_id,
                permissions,
            },
            Some(user_id),
            authority,
            timestamp,
            None,
        )
        .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    pub async fn update_member_permissions(
        &self,
        journal_id: JournalId,
        user_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<i64> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::OWNER)
            .await?;

        let stream_handle =
            get_stream_handle(&mut tx, journal_id, *journal_id, StreamType::Journal).await?;

        let latest_member_payload = get_latest_member_payload(&mut tx, journal_id, user_id)
            .await?
            .ok_or(JournalError::UserDoesntHaveAccess(user_id))?;

        match latest_member_payload {
            MemberPayload::AddedToJournal(..) | MemberPayload::PermissionsUpdated(..) => {}

            MemberPayload::CreatedJournal => {
                return Err(JournalError::OwnerModification(user_id));
            }

            MemberPayload::RemovedFromJournal => {
                return Err(JournalError::UserDoesntHaveAccess(user_id));
            }
        }

        let event_id = append_modification_event_in_tx(
            &mut tx,
            stream_handle,
            JournalDomainEvent::MemberPermissionsUpdated {
                journal_id,
                user_id,
                permissions,
            },
            Some(user_id),
            authority,
            timestamp,
            None,
        )
        .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    pub async fn remove_member(
        &self,
        journal_id: JournalId,
        user_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<i64> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::OWNER)
            .await?;

        let stream_handle =
            get_stream_handle(&mut tx, journal_id, *journal_id, StreamType::Journal).await?;

        let latest_member_payload = get_latest_member_payload(&mut tx, journal_id, user_id)
            .await?
            .ok_or(JournalError::UserDoesntHaveAccess(user_id))?;

        match latest_member_payload {
            MemberPayload::CreatedJournal => {
                return Err(JournalError::OwnerModification(user_id));
            }
            MemberPayload::AddedToJournal(..) | MemberPayload::PermissionsUpdated(..) => {}

            MemberPayload::RemovedFromJournal => {
                return Err(JournalError::UserDoesntHaveAccess(user_id));
            }
        }

        let event_id = append_modification_event_in_tx(
            &mut tx,
            stream_handle,
            JournalDomainEvent::MemberRemoved {
                journal_id,
                user_id,
            },
            Some(user_id),
            authority,
            timestamp,
            None,
        )
        .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    pub async fn list_journal_members(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<UserId>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        Ok(sqlx::query_scalar!(
            r#"
            SELECT user_id as "user_id: UserId" FROM journal_members WHERE journal_id = $1
            "#,
            journal_id as JournalId
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn get_effective_permissions(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Permissions> {
        match authority.actor() {
            Actor::System => Ok(Permissions::OWNER),
            Actor::Anonymous => Ok(Permissions::empty()),
            Actor::User(user_id) => {
                let permission_bits = sqlx::query_scalar!(
                    r#"
                    SELECT
                        CASE
                            WHEN j.owner_id = $1 THEN $2::INTEGER
                            ELSE COALESCE(
                                 (SELECT jm.permissions
                                 FROM journal_members jm
                                 WHERE jm.journal_id = j.id AND jm.user_id = $1),
                                 0
                            )
                        END as "i32!"
                    FROM journals j
                    WHERE j.id = $3
                "#,
                    user_id as UserId,
                    Permissions::all().bits(),
                    journal_id as JournalId
                )
                .fetch_optional(self.pool())
                .await?;

                if let Some(bits) = permission_bits {
                    Ok(Permissions::from_bits(bits)
                        .ok_or(JournalError::PermissionDecode(PermissionDecodeError(bits)))?)
                } else {
                    Ok(Permissions::empty())
                }
            }
        }
    }
}
