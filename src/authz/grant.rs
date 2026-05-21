use crate::grant::GrantId;
use crate::grant::GrantPayload;
use crate::store::revised;

#[derive(Clone, Copy, Debug)]
pub struct GrantStream;

impl revised::Stream for GrantStream {
    type Id = GrantId;
    type Payload = GrantPayload;
}
