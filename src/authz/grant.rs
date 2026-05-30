use crate::id;
use crate::ident::Ident;
use crate::store::revised::Stream;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantPayload {
    Created,
    Revoked,
}

id!(GrantId, Ident::new16());

#[derive(Clone, Copy, Debug)]
pub struct GrantStream;

impl Stream for GrantStream {
    type Id = GrantId;
    type Payload = GrantPayload;
}
