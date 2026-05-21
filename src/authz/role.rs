use crate::role::RoleId;
use crate::role::RolePayload;
use crate::store::revised;

#[derive(Clone, Copy, Debug)]
pub struct RoleStream;

impl revised::Stream for RoleStream {
    type Id = RoleId;
    type Payload = RolePayload;
}
