use crate::grant::GrantId;
use crate::grant::GrantPayload;
use crate::store::revised::Stream;

#[derive(Clone, Copy, Debug)]
pub struct GrantStream;

impl Stream for GrantStream {
    type Id = GrantId;
    type Payload = GrantPayload;
}
