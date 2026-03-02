use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::broadcast::error::RecvError;

#[expect(dead_code)]
pub enum QueueError {
    QueueClosed,
}

#[expect(dead_code)]
pub trait Queue<Eid: Clone, Event: Clone> {
    /// Waits for the next event
    ///
    /// If a prior event was missed or the event was too big to send in the queue,
    /// `None` is sent in place of the event and the caller must manually get the events they missed
    async fn await_next(&self) -> Result<(Eid, Option<Event>), QueueError>;

    /// record an event into the queue
    async fn record(&self, event_id: Eid, event: Event) -> Result<(), QueueError>;
}

/// A queue based on tokio's broadcast channel
///
/// This doesn't work cross-process
#[expect(dead_code)]
pub struct ProcessBroadcastQueue<Eid: Clone, Event: Clone> {
    sender: tokio::sync::broadcast::Sender<(Eid, Event)>,
    receiver: Arc<Mutex<tokio::sync::broadcast::Receiver<(Eid, Event)>>>,
}

impl<Eid: Clone, Event: Clone> ProcessBroadcastQueue<Eid, Event> {
    #[expect(dead_code)]
    pub fn new() -> Self {
        let (sender, receiver) = tokio::sync::broadcast::channel(64);
        Self {
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }
}

impl<Eid: Clone, Event: Clone> Queue<Eid, Event> for ProcessBroadcastQueue<Eid, Event> {
    async fn await_next(&self) -> Result<(Eid, Option<Event>), QueueError> {
        // this will hold the lock for extended periods of time, but it shouldn't matter
        // because this method should not be called multiple times in parallel on the same process
        let mut lock = self.receiver.lock().await;

        match lock.recv().await {
            Ok((id, event)) => Ok((id, Some(event))),
            Err(RecvError::Lagged(_skipped)) => {
                loop {
                    match lock.recv().await {
                        // return `None` for the event so that the caller must manually request the missed event(s)
                        Ok((id, _event)) => return Ok((id, None)),

                        Err(RecvError::Lagged(_skipped)) => {}

                        Err(RecvError::Closed) => return Err(QueueError::QueueClosed),
                    }
                }
            }
            Err(RecvError::Closed) => Err(QueueError::QueueClosed),
        }
    }
    async fn record(&self, event_id: Eid, event: Event) -> Result<(), QueueError> {
        match self.sender.send((event_id, event)) {
            Ok(_receivers) => Ok(()),
            Err(tokio::sync::broadcast::error::SendError(_payload)) => Err(QueueError::QueueClosed),
        }
    }
}
