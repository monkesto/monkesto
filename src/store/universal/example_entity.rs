use crate::ident::Ident;
use crate::schema::examples;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::store::universal::{DieselExecute, EventId, GetPayloadUsage, PayloadUsage};
use crate::{entity, payload, state};
use diesel::{QueryResult, SqliteConnection, delete, insert_into, update};
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
    UpdateCounter { new_val: i64 },
    Deleted,
}

payload! {
    AnyPayload::Example,

    pub enum ExamplePayload {
        Created {counter_value: i64},
        Modified(ExampleModifiedPayload)
    }
}

state! {
    #[diesel(table_name = crate::schema::examples)]
    pub struct ExampleState {
        id: ExampleId,
        counter: i64,
        as_of: EventId
    }
}

impl DieselExecute for ExamplePayload {
    fn execute_sql(
        &self,
        entity_id: Ident,
        event_id: EventId,
        conn: &mut SqliteConnection,
    ) -> QueryResult<()> {
        match self {
            ExamplePayload::Created { counter_value } => insert_into(examples::table)
                .values(ExampleState {
                    id: entity_id.into(),
                    counter: *counter_value,
                    as_of: event_id,
                })
                .execute(conn)
                .map(drop),
            ExamplePayload::Modified(modified_payload) => match modified_payload {
                ExampleModifiedPayload::UpdateCounter { new_val } => {
                    update(examples::table.filter(examples::id.eq(entity_id)))
                        .set(examples::counter.eq(new_val))
                        .execute(conn)
                        .map(drop)
                }
                ExampleModifiedPayload::Deleted => {
                    delete(examples::table.filter(examples::id.eq(entity_id)))
                        .execute(conn)
                        .map(drop)
                }
            },
        }
    }
}

impl GetPayloadUsage<ExampleEntity> for ExamplePayload {
    fn usage<T: Into<ExampleId>>(
        self,
        entity_id: T,
        event_id: EventId,
    ) -> PayloadUsage<ExampleEntity> {
        match self {
            ExamplePayload::Created { counter_value } => PayloadUsage::CreatesState(ExampleState {
                id: entity_id.into(),
                counter: counter_value,
                as_of: event_id,
            }),
            ExamplePayload::Modified(modified_payload) => {
                PayloadUsage::ModifiesState(Box::new(move |state: &mut ExampleState| {
                    match modified_payload {
                        ExampleModifiedPayload::UpdateCounter { new_val } => {
                            state.counter = new_val
                        }
                        ExampleModifiedPayload::Deleted => {}
                    }
                    state.as_of = event_id;
                }))
            }
        }
    }
}
