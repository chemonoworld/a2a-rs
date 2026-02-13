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

#[cfg(test)]
mod tests {
    use super::*;
    use a2a_types::{AgentInterface, ProtocolBinding};

    #[test]
    fn test_from_agent_card_no_jsonrpc_interface() {
        let card = AgentCard {
            name: "gRPC Only Agent".into(),
            description: None,
            supported_interfaces: vec![AgentInterface {
                url: "grpc://localhost:50051".into(),
                protocol_binding: ProtocolBinding::Grpc,
                protocol_version: "1.0".into(),
                tenant: None,
            }],
            capabilities: None,
            security_schemes: None,
            skills: None,
            default_input_modes: None,
            default_output_modes: None,
        };

        let result = A2AClient::from_agent_card(&card);
        assert!(result.is_err());
        let err = result.err().unwrap();
        match err {
            ClientError::NoSupportedInterface => {}
            other => panic!("Expected NoSupportedInterface, got: {other}"),
        }
    }

    #[test]
    fn test_from_agent_card_selects_jsonrpc() {
        let card = AgentCard {
            name: "Multi Agent".into(),
            description: None,
            supported_interfaces: vec![
                AgentInterface {
                    url: "grpc://localhost:50051".into(),
                    protocol_binding: ProtocolBinding::Grpc,
                    protocol_version: "1.0".into(),
                    tenant: None,
                },
                AgentInterface {
                    url: "http://localhost:3000".into(),
                    protocol_binding: ProtocolBinding::JsonRpc,
                    protocol_version: "1.0".into(),
                    tenant: None,
                },
            ],
            capabilities: None,
            security_schemes: None,
            skills: None,
            default_input_modes: None,
            default_output_modes: None,
        };

        let result = A2AClient::from_agent_card(&card);
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_error_display() {
        let err = ClientError::EmptyResult;
        assert!(format!("{err}").contains("Empty result"));

        let err = ClientError::NoSupportedInterface;
        assert!(format!("{err}").contains("No supported interface"));

        let err = ClientError::SseParse("bad data".into());
        assert!(format!("{err}").contains("bad data"));

        let err = ClientError::StreamClosed;
        assert!(format!("{err}").contains("Stream closed"));

        let err = ClientError::JsonRpc(a2a_types::JsonRpcError {
            code: -32001,
            message: "Task not found".into(),
            data: None,
        });
        let display = format!("{err}");
        assert!(display.contains("-32001"));
        assert!(display.contains("Task not found"));
    }

    #[test]
    fn test_from_agent_card_empty_interfaces() {
        let card = AgentCard {
            name: "Empty".into(),
            description: None,
            supported_interfaces: vec![],
            capabilities: None,
            security_schemes: None,
            skills: None,
            default_input_modes: None,
            default_output_modes: None,
        };

        let result = A2AClient::from_agent_card(&card);
        assert!(result.is_err());
        match result.err().unwrap() {
            ClientError::NoSupportedInterface => {}
            other => panic!("Expected NoSupportedInterface, got: {other}"),
        }
    }

    #[test]
    fn test_client_new_creates_transport() {
        // A2AClient::new should succeed with a valid URL format
        let client = A2AClient::new("http://localhost:9999");
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_with_transport() {
        use async_trait::async_trait;

        struct MockTransport;

        #[async_trait]
        impl Transport for MockTransport {
            async fn send_message(
                &self,
                _request: SendMessageRequest,
            ) -> Result<Event, ClientError> {
                Err(ClientError::EmptyResult)
            }

            async fn send_message_stream(
                &self,
                _request: SendMessageRequest,
            ) -> Result<EventStream, ClientError> {
                Err(ClientError::EmptyResult)
            }

            async fn get_task(
                &self,
                _request: GetTaskRequest,
            ) -> Result<Task, ClientError> {
                Err(ClientError::EmptyResult)
            }

            async fn cancel_task(
                &self,
                _request: CancelTaskRequest,
            ) -> Result<Task, ClientError> {
                Err(ClientError::EmptyResult)
            }

            async fn resubscribe(
                &self,
                _request: TaskResubscriptionRequest,
            ) -> Result<EventStream, ClientError> {
                Err(ClientError::EmptyResult)
            }
        }

        let client = A2AClient::with_transport(MockTransport);
        // Client should be created - we can't call methods because they're async
        // but the constructor should succeed
        let _ = client;
    }

    #[tokio::test]
    async fn test_mock_transport_send_message() {
        use a2a_types::{
            Message, Part, PartContent, Role, Task, TaskState, TaskStatus,
        };
        use async_trait::async_trait;

        struct CompletingTransport;

        #[async_trait]
        impl Transport for CompletingTransport {
            async fn send_message(
                &self,
                _request: SendMessageRequest,
            ) -> Result<Event, ClientError> {
                Ok(Event::Task(Task {
                    id: "mock-task-1".into(),
                    context_id: "mock-ctx".into(),
                    status: TaskStatus {
                        state: TaskState::Submitted,
                        message: None,
                        timestamp: None,
                    },
                    artifacts: None,
                    history: None,
                    metadata: None,
                }))
            }

            async fn send_message_stream(
                &self,
                _request: SendMessageRequest,
            ) -> Result<EventStream, ClientError> {
                Err(ClientError::EmptyResult)
            }

            async fn get_task(
                &self,
                request: GetTaskRequest,
            ) -> Result<Task, ClientError> {
                Ok(Task {
                    id: request.id,
                    context_id: "mock-ctx".into(),
                    status: TaskStatus {
                        state: TaskState::Completed,
                        message: None,
                        timestamp: None,
                    },
                    artifacts: None,
                    history: None,
                    metadata: None,
                })
            }

            async fn cancel_task(
                &self,
                request: CancelTaskRequest,
            ) -> Result<Task, ClientError> {
                Ok(Task {
                    id: request.id,
                    context_id: "mock-ctx".into(),
                    status: TaskStatus {
                        state: TaskState::Canceled,
                        message: None,
                        timestamp: None,
                    },
                    artifacts: None,
                    history: None,
                    metadata: None,
                })
            }

            async fn resubscribe(
                &self,
                _request: TaskResubscriptionRequest,
            ) -> Result<EventStream, ClientError> {
                Err(ClientError::EmptyResult)
            }
        }

        let client = A2AClient::with_transport(CompletingTransport);

        // Test send_message
        let msg = Message {
            message_id: "m-mock".into(),
            role: Role::User,
            parts: vec![Part {
                content: PartContent::Text {
                    text: "Hello mock".into(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            }],
            context_id: None,
            task_id: None,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let event = client.send_message(msg).await.unwrap();
        match event {
            Event::Task(t) => {
                assert_eq!(t.id, "mock-task-1");
                assert_eq!(t.status.state, TaskState::Submitted);
            }
            _ => panic!("Expected Task event from mock"),
        }

        // Test get_task
        let task = client.get_task("mock-task-1").await.unwrap();
        assert_eq!(task.id, "mock-task-1");
        assert_eq!(task.status.state, TaskState::Completed);

        // Test get_task_with_history
        let task = client.get_task_with_history("mock-task-1", 5).await.unwrap();
        assert_eq!(task.id, "mock-task-1");

        // Test cancel_task
        let task = client.cancel_task("mock-task-1").await.unwrap();
        assert_eq!(task.status.state, TaskState::Canceled);

        // Test resubscribe returns error
        let result = client.resubscribe("mock-task-1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_message_with_config_via_mock() {
        use a2a_types::{
            Message, Part, PartContent, Role, Task, TaskState, TaskStatus,
            TaskStatusUpdateEvent,
        };
        use async_trait::async_trait;

        struct BlockingTransport;

        #[async_trait]
        impl Transport for BlockingTransport {
            async fn send_message(
                &self,
                request: SendMessageRequest,
            ) -> Result<Event, ClientError> {
                // Return Completed if blocking is requested
                let blocking = request
                    .configuration
                    .as_ref()
                    .and_then(|c| c.blocking)
                    .unwrap_or(false);
                if blocking {
                    Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                        task_id: "mock-blocking".into(),
                        context_id: "mock-ctx".into(),
                        status: TaskStatus {
                            state: TaskState::Completed,
                            message: None,
                            timestamp: None,
                        },
                        is_final: Some(true),
                        metadata: None,
                    }))
                } else {
                    Ok(Event::Task(Task {
                        id: "mock-nonblocking".into(),
                        context_id: "mock-ctx".into(),
                        status: TaskStatus {
                            state: TaskState::Submitted,
                            message: None,
                            timestamp: None,
                        },
                        artifacts: None,
                        history: None,
                        metadata: None,
                    }))
                }
            }

            async fn send_message_stream(
                &self,
                _request: SendMessageRequest,
            ) -> Result<EventStream, ClientError> {
                Err(ClientError::EmptyResult)
            }

            async fn get_task(
                &self,
                _request: GetTaskRequest,
            ) -> Result<Task, ClientError> {
                Err(ClientError::EmptyResult)
            }

            async fn cancel_task(
                &self,
                _request: CancelTaskRequest,
            ) -> Result<Task, ClientError> {
                Err(ClientError::EmptyResult)
            }

            async fn resubscribe(
                &self,
                _request: TaskResubscriptionRequest,
            ) -> Result<EventStream, ClientError> {
                Err(ClientError::EmptyResult)
            }
        }

        let client = A2AClient::with_transport(BlockingTransport);

        // Test with blocking config
        let msg = Message {
            message_id: "m-cfg".into(),
            role: Role::User,
            parts: vec![Part {
                content: PartContent::Text {
                    text: "Config test".into(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            }],
            context_id: None,
            task_id: None,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let config = a2a_types::MessageSendConfiguration {
            blocking: Some(true),
            accepted_output_modes: None,
            push_notification_config: None,
            history_length: None,
        };

        let event = client.send_message_with_config(msg, config).await.unwrap();
        match event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.status.state, TaskState::Completed);
                assert_eq!(u.task_id, "mock-blocking");
            }
            _ => panic!("Expected TaskStatusUpdate from blocking mock"),
        }
    }
}
