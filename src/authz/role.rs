use crate::role::RoleId;
use crate::role::RolePayload;
use crate::store::revised::Stream;

#[derive(Clone, Copy, Debug)]
pub struct RoleStream;

impl Stream for RoleStream {
    type Id = RoleId;
    type Payload = RolePayload;
}
