use a2a_types::{
    AgentCard, CancelTaskRequest, Event, GetTaskRequest, Message, MessageSendConfiguration,
    ProtocolBinding, SendMessageRequest, Task, TaskResubscriptionRequest,
};

use crate::error::ClientError;
use crate::jsonrpc_transport::JsonRpcTransport;
use crate::transport::{EventStream, Transport};

/// High-level A2A client.
///
/// Wraps a [`Transport`] and provides ergonomic methods for the A2A protocol operations.
pub struct A2AClient {
    transport: Box<dyn Transport>,
}

impl A2AClient {
    /// Create a client from an [`AgentCard`].
    ///
    /// Scans `supported_interfaces` for a `JSONRPC` binding and uses its URL.
    pub fn from_agent_card(card: &AgentCard) -> Result<Self, ClientError> {
        let interface = card
            .supported_interfaces
            .iter()
            .find(|i| i.protocol_binding == ProtocolBinding::JsonRpc)
            .ok_or(ClientError::NoSupportedInterface)?;

        let transport = JsonRpcTransport::new(&interface.url)?;
        Ok(Self {
            transport: Box::new(transport),
        })
    }

    /// Create a client targeting a JSON-RPC endpoint URL directly.
    pub fn new(url: &str) -> Result<Self, ClientError> {
        let transport = JsonRpcTransport::new(url)?;
        Ok(Self {
            transport: Box::new(transport),
        })
    }

    /// Create a client with a custom [`Transport`] implementation.
    pub fn with_transport(transport: impl Transport) -> Self {
        Self {
            transport: Box::new(transport),
        }
    }

    /// Send a message (synchronous response).
    pub async fn send_message(&self, message: Message) -> Result<Event, ClientError> {
        let request = SendMessageRequest {
            message,
            configuration: None,
        };
        self.transport.send_message(request).await
    }

    /// Send a message with explicit configuration (synchronous response).
    pub async fn send_message_with_config(
        &self,
        message: Message,
        config: MessageSendConfiguration,
    ) -> Result<Event, ClientError> {
        let request = SendMessageRequest {
            message,
            configuration: Some(config),
        };
        self.transport.send_message(request).await
    }

    /// Send a message and receive a streaming SSE response.
    pub async fn send_message_stream(
        &self,
        message: Message,
    ) -> Result<EventStream, ClientError> {
        let request = SendMessageRequest {
            message,
            configuration: None,
        };
        self.transport.send_message_stream(request).await
    }

    /// Get a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Result<Task, ClientError> {
        self.transport
            .get_task(GetTaskRequest {
                id: task_id.to_string(),
                history_length: None,
            })
            .await
    }

    /// Get a task by ID with history.
    pub async fn get_task_with_history(
        &self,
        task_id: &str,
        history_length: i32,
    ) -> Result<Task, ClientError> {
        self.transport
            .get_task(GetTaskRequest {
                id: task_id.to_string(),
                history_length: Some(history_length),
            })
            .await
    }

    /// Cancel a task by ID.
    pub async fn cancel_task(&self, task_id: &str) -> Result<Task, ClientError> {
        self.transport
            .cancel_task(CancelTaskRequest {
                id: task_id.to_string(),
            })
            .await
    }

    /// Resubscribe to a task's event stream.
    pub async fn resubscribe(&self, task_id: &str) -> Result<EventStream, ClientError> {
        self.transport
            .resubscribe(TaskResubscriptionRequest {
                id: task_id.to_string(),
            })
            .await
    }
}
