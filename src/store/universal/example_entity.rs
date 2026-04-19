use crate::entity;
use crate::ident::{Ident, ProjectionFromPayloadError};
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::store::universal::{ApplyPayload, PayloadWithId, Projection};
use serde::{Deserialize, Serialize};

entity!(
    ExampleId,
    ExamplePayload,
    ExampleProjection,
    EntityType::Example,
    Ident::new16()
);

#[derive(Payload, Clone, Serialize, Deserialize, Debug)]
pub enum ExamplePayload {
    Created,
    Deleted,
}

impl From<ExamplePayload> for AnyPayload {
    fn from(val: ExamplePayload) -> Self {
        AnyPayload::Example(val)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ExampleProjection {
    id: ExampleId,
    deleted: bool,
}

impl TryFrom<PayloadWithId<'_, ExampleId>> for ExampleProjection {
    type Error = ProjectionFromPayloadError;

    fn try_from(value: PayloadWithId<ExampleId>) -> Result<Self, Self::Error> {
        match value.payload {
            ExamplePayload::Created => Ok(Self {
                id: value.id,
                deleted: false,
            }),
            _ => Err(ProjectionFromPayloadError::IncorrectVariant(format!(
                "{:?}",
                value.payload
            ))),
        }
    }
}

impl ApplyPayload<'_, ExampleId> for ExampleProjection {
    fn apply(&mut self, payload: &ExamplePayload) -> &mut ExampleProjection {
        match payload {
            ExamplePayload::Created => {}
            ExamplePayload::Deleted => self.deleted = true,
        }
        self
    }
}
