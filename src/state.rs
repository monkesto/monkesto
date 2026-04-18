use crate::store::EventId;
use crate::store::When;

#[expect(dead_code)]
pub trait State<E> {
    type Error;

    fn initial() -> Self
    where
        Self: Sized;

    fn apply(&mut self, event: E) -> Result<(), Self::Error>;

    fn when(&self) -> When<EventId>;
}
