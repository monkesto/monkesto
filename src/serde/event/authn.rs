use crate::authn::AuthnEvent;
use crate::email::Email;
use crate::proto::event::authn::ProtoAuthnEvent;
use crate::proto::event::authn::proto_authn_event::{
    AuthnEventType, ProtoPasskeyCreated, ProtoPasskeyDeleted, ProtoUserCreated, ProtoUserDeleted,
};
use crate::serde::error::ProtoError;
use crate::serde::error::ProtoError::FieldRequired;
use webauthn_rs::prelude::Uuid;

impl From<AuthnEvent> for ProtoAuthnEvent {
    fn from(event: AuthnEvent) -> Self {
        let event = match event {
            AuthnEvent::UserCreated {
                user_id,
                email,
                webauthn_uuid,
                authority,
                timestamp,
            } => AuthnEventType::UserCreated(ProtoUserCreated {
                user_id: Some(user_id.into()),
                email: email.to_string(),
                webauthn_uuid: webauthn_uuid.as_bytes().into(),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),

            AuthnEvent::UserDeleted {
                user_id,
                authority,
                timestamp,
            } => AuthnEventType::UserDeleted(ProtoUserDeleted {
                user_id: Some(user_id.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),

            AuthnEvent::PasskeyCreated {
                passkey_id,
                user_id,
                passkey,
                authority,
                timestamp,
            } => AuthnEventType::PasskeyCreated(ProtoPasskeyCreated {
                passkey_id: Some(passkey_id.into()),
                user_id: Some(user_id.into()),
                passkey: rmp_serde::to_vec(&passkey).expect("failed to serialize passkey"),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),

            AuthnEvent::PasskeyDeleted {
                passkey_id,
                authority,
                timestamp,
            } => AuthnEventType::PasskeyDeleted(ProtoPasskeyDeleted {
                passkey_id: Some(passkey_id.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
        };

        ProtoAuthnEvent {
            authn_event_type: Some(event),
        }
    }
}

impl TryFrom<ProtoAuthnEvent> for AuthnEvent {
    type Error = ProtoError;

    fn try_from(event: ProtoAuthnEvent) -> Result<Self, Self::Error> {
        let event = match event.authn_event_type.ok_or(FieldRequired)? {
            AuthnEventType::UserCreated(ev) => AuthnEvent::UserCreated {
                user_id: ev.user_id.try_into()?,
                email: Email::try_new(ev.email)?,
                webauthn_uuid: Uuid::from_slice(ev.webauthn_uuid.as_slice())
                    .map_err(|e| ProtoError::ParseUuid(e.to_string()))?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            AuthnEventType::UserDeleted(ev) => AuthnEvent::UserDeleted {
                user_id: ev.user_id.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            AuthnEventType::PasskeyCreated(ev) => AuthnEvent::PasskeyCreated {
                passkey_id: ev.passkey_id.try_into()?,
                user_id: ev.user_id.try_into()?,
                passkey: Box::new(
                    rmp_serde::from_slice(ev.passkey.as_slice())
                        .map_err(|_| ProtoError::Deserialize)?,
                ),
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            AuthnEventType::PasskeyDeleted(ev) => AuthnEvent::PasskeyDeleted {
                passkey_id: ev.passkey_id.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
        };

        Ok(event)
    }
}
