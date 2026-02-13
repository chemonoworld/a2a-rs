use a2a_types::Event;
use async_trait::async_trait;

use crate::error::ServerError;
use crate::event_queue::EventQueueWriter;

/// Agent business logic entry point.
///
/// Developers implement this trait to define how the agent processes
/// incoming events and writes response events to the queue.
#[async_trait]
pub trait AgentExecutor: Send + Sync + 'static {
    async fn execute(
        &self,
        event: Event,
        queue: &dyn EventQueueWriter,
    ) -> Result<(), ServerError>;
}
