use crate::ident::Ident;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::store::universal::{GetPayloadUsage, PayloadUsage, SequenceId};
use crate::{entity, payload, state};
use serde::{Deserialize, Serialize};
use std::io::Write;

entity!(
    ExampleEntity,
    EntityType::Example,
    ExampleId,
    ExamplePayload,
    ExampleState,
    Ident::new16()
);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ExampleModifiedPayload {
    Deleted,
}

payload! {
    AnyPayload::Example,

    pub enum ExamplePayload {
        Created,
        Modified(ExampleModifiedPayload)
    }
}

state! {
    #[diesel(table_name = crate::schema::examples)]
    pub struct ExampleState {
        id: ExampleId,
        deleted: bool,
        as_of: SequenceId
    }
}

impl GetPayloadUsage<ExampleEntity> for ExamplePayload {
    fn usage<T: Into<ExampleId>>(
        self,
        entity_id: T,
        sequence_id: SequenceId,
    ) -> PayloadUsage<ExampleEntity> {
        match self {
            ExamplePayload::Created => PayloadUsage::CreatesState(ExampleState {
                id: entity_id.into(),
                deleted: false,
                as_of: sequence_id,
            }),
            ExamplePayload::Modified(modified_payload) => {
                PayloadUsage::ModifiesState(Box::new(move |state: &mut ExampleState| {
                    match modified_payload {
                        ExampleModifiedPayload::Deleted => state.deleted = true,
                    }
                    state.as_of = sequence_id;
                }))
            }
        }
    }
}
