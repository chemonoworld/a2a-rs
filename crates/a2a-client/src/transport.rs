use std::pin::Pin;

use a2a_types::{
    CancelTaskRequest, Event, GetTaskRequest, SendMessageRequest, Task,
    TaskResubscriptionRequest,
};
use futures_core::Stream;

use crate::error::ClientError;

/// A stream of SSE events from a streaming A2A response.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event, ClientError>> + Send>>;

/// Transport layer abstraction for A2A protocol communication.
///
/// Default implementation: [`crate::JsonRpcTransport`] (reqwest-based HTTP + JSON-RPC).
#[async_trait::async_trait]
pub trait Transport: Send + Sync + 'static {
    async fn send_message(&self, request: SendMessageRequest) -> Result<Event, ClientError>;

    async fn send_message_stream(
        &self,
        request: SendMessageRequest,
    ) -> Result<EventStream, ClientError>;

    async fn get_task(&self, request: GetTaskRequest) -> Result<Task, ClientError>;

    async fn cancel_task(&self, request: CancelTaskRequest) -> Result<Task, ClientError>;

    async fn resubscribe(
        &self,
        request: TaskResubscriptionRequest,
    ) -> Result<EventStream, ClientError>;
}
