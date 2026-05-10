use crate::ident::{Ident, StateFromPayloadError};
use crate::store::universal::ApplyPayload;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::{entity, payload, state};

entity!(
    ExampleEntity,
    EntityType::Example,
    ExampleId,
    ExamplePayload,
    ExampleState,
    Ident::new16()
);

payload! {
    AnyPayload::Example,

    pub enum ExamplePayload {
        Created,
        Deleted,
    }
}

state! {
    #[diesel(table_name = crate::schema::examples)]
    pub struct ExampleState {
        id: ExampleId,
        deleted: bool,
    }
}

impl TryFrom<(ExampleId, ExamplePayload)> for ExampleState {
    type Error = StateFromPayloadError;

    fn try_from(value: (ExampleId, ExamplePayload)) -> Result<Self, Self::Error> {
        let (id, payload) = value;
        match payload {
            ExamplePayload::Created => Ok(Self { id, deleted: false }),
            _ => Err(StateFromPayloadError::IncorrectVariant(format!(
                "{:?}",
                payload
            ))),
        }
    }
}

impl ApplyPayload<ExampleEntity> for ExampleState {
    fn apply(&mut self, payload: &ExamplePayload) -> &mut ExampleState {
        match payload {
            ExamplePayload::Created => {}
            ExamplePayload::Deleted => self.deleted = true,
        }
        self
    }
}
