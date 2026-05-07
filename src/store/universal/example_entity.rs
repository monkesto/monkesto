use crate::ident::{Ident, ProjectionFromPayloadError};
use crate::store::universal::ApplyPayload;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::{entity, payload, projection};

entity!(
    ExampleEntity,
    EntityType::Example,
    ExampleId,
    ExamplePayload,
    ExampleProjection,
    Ident::new16()
);

payload! {
    AnyPayload::Example,

    pub enum ExamplePayload {
        Created,
        Deleted,
    }
}

projection! {
    pub struct ExampleProjection {
        id: ExampleId,
        deleted: bool,
    }
}

impl TryFrom<(ExampleId, ExamplePayload)> for ExampleProjection {
    type Error = ProjectionFromPayloadError;

    fn try_from(value: (ExampleId, ExamplePayload)) -> Result<Self, Self::Error> {
        let (id, payload) = value;
        match payload {
            ExamplePayload::Created => Ok(Self { id, deleted: false }),
            _ => Err(ProjectionFromPayloadError::IncorrectVariant(format!(
                "{:?}",
                payload
            ))),
        }
    }
}

impl ApplyPayload<ExampleEntity> for ExampleProjection {
    fn apply(&mut self, payload: &ExamplePayload) -> &mut ExampleProjection {
        match payload {
            ExamplePayload::Created => {}
            ExamplePayload::Deleted => self.deleted = true,
        }
        self
    }
}
