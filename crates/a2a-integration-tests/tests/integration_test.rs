use std::sync::Arc;

use a2a_client::{A2AClient, AgentCardResolver};
use a2a_server::{
    AgentExecutor, DefaultHandler, EventQueueWriter, ServerError,
};
use a2a_types::{
    AgentCapabilities, AgentCard, AgentInterface, AgentSkill, Artifact, Event,
    Message, MessageSendConfiguration, Part, PartContent, ProtocolBinding, Role,
    TaskArtifactUpdateEvent, TaskState, TaskStatus, TaskStatusUpdateEvent,
};
use async_trait::async_trait;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

// ---------------------------------------------------------------------------
// Test Executors
// ---------------------------------------------------------------------------

/// Echo executor: Working -> ArtifactUpdate -> Completed
struct EchoExecutor;

#[async_trait]
impl AgentExecutor for EchoExecutor {
    async fn execute(
        &self,
        event: Event,
        queue: &dyn EventQueueWriter,
    ) -> Result<(), ServerError> {
        let (task_id, context_id) = match &event {
            Event::Task(task) => (task.id.clone(), task.context_id.clone()),
            _ => return Ok(()),
        };

        queue
            .write(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            }))
            .await?;

        queue
            .write(Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                artifact: Artifact {
                    artifact_id: "art-1".into(),
                    name: None,
                    description: None,
                    parts: vec![Part {
                        content: PartContent::Text {
                            text: "Echo response".into(),
                        },
                        metadata: None,
                        filename: None,
                        media_type: None,
                    }],
                    metadata: None,
                    extensions: None,
                },
                append: false,
                last_chunk: true,
                metadata: None,
            }))
            .await?;

        queue
            .write(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id,
                context_id,
                status: TaskStatus {
                    state: TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            }))
            .await?;

        Ok(())
    }
}

/// Slow executor: waits for a long time (used for cancel tests)
struct SlowExecutor;

#[async_trait]
impl AgentExecutor for SlowExecutor {
    async fn execute(
        &self,
        event: Event,
        queue: &dyn EventQueueWriter,
    ) -> Result<(), ServerError> {
        let (task_id, context_id) = match &event {
            Event::Task(task) => (task.id.clone(), task.context_id.clone()),
            _ => return Ok(()),
        };

        queue
            .write(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id,
                context_id,
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            }))
            .await?;

        // Hang for a long time (will be interrupted by cancel)
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test Helpers
// ---------------------------------------------------------------------------

fn make_agent_card(addr: &str) -> AgentCard {
    AgentCard {
        name: "Test Agent".into(),
        description: Some("Integration test agent".into()),
        supported_interfaces: vec![AgentInterface {
            url: format!("http://{addr}"),
            protocol_binding: ProtocolBinding::JsonRpc,
            protocol_version: "1.0".into(),
            tenant: None,
        }],
        capabilities: Some(AgentCapabilities {
            streaming: true,
            push_notifications: false,
            extended_agent_card: false,
            extensions: None,
        }),
        security_schemes: None,
        skills: Some(vec![AgentSkill {
            id: "echo".into(),
            name: "Echo".into(),
            description: Some("Echoes messages".into()),
            tags: None,
            examples: None,
            input_modes: None,
            output_modes: None,
        }]),
        default_input_modes: None,
        default_output_modes: None,
    }
}

fn make_message(text: &str) -> Message {
    Message {
        message_id: uuid::Uuid::new_v4().to_string(),
        role: Role::User,
        parts: vec![Part {
            content: PartContent::Text {
                text: text.into(),
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
    }
}

/// Start a test server on a random port and return (addr, handle).
async fn start_test_server(
    executor: impl AgentExecutor,
) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind");
    let addr = listener.local_addr().unwrap().to_string();

    let agent_card = make_agent_card(&addr);
    let handler = DefaultHandler::builder()
        .executor(Arc::new(executor))
        .build();
    let router = a2a_server::create_router(Arc::new(handler), agent_card);

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (addr, handle)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_card_resolve() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let resolver = AgentCardResolver::new();
    let card = resolver
        .resolve(&format!("http://{addr}"))
        .await
        .expect("Failed to resolve agent card");

    assert_eq!(card.name, "Test Agent");
    assert_eq!(card.supported_interfaces.len(), 1);
    assert_eq!(
        card.supported_interfaces[0].protocol_binding,
        ProtocolBinding::JsonRpc
    );
    assert!(card.capabilities.as_ref().unwrap().streaming);
    assert_eq!(card.skills.as_ref().unwrap()[0].id, "echo");

    handle.abort();
}

#[tokio::test]
async fn test_send_message_sync() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let event = client
        .send_message(make_message("Hello"))
        .await
        .expect("Failed to send message");

    // Non-blocking returns Submitted task
    match event {
        Event::Task(task) => {
            assert_eq!(task.status.state, TaskState::Submitted);
            assert!(!task.id.is_empty());
        }
        _ => panic!("Expected Task event, got: {event:?}"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_send_message_blocking() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let config = MessageSendConfiguration {
        accepted_output_modes: None,
        blocking: Some(true),
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("Hello blocking"), config)
        .await
        .expect("Failed to send blocking message");

    // Blocking mode waits for Completed
    match event {
        Event::TaskStatusUpdate(update) => {
            assert_eq!(update.status.state, TaskState::Completed);
        }
        _ => panic!("Expected Completed TaskStatusUpdate, got: {event:?}"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_send_message_stream() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut stream = client
        .send_message_stream(make_message("Hello stream"))
        .await
        .expect("Failed to start stream");

    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.expect("Stream error"));
    }

    // Expect: Task(Submitted) -> StatusUpdate(Working) -> ArtifactUpdate -> StatusUpdate(Completed)
    assert!(
        events.len() >= 3,
        "Expected at least 3 events, got {}",
        events.len()
    );

    // Check that we see Working and Completed status updates
    let has_working = events.iter().any(|e| matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Working));
    let has_completed = events.iter().any(|e| matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Completed));
    let has_artifact = events
        .iter()
        .any(|e| matches!(e, Event::TaskArtifactUpdate(_)));

    assert!(has_working, "Missing Working status update");
    assert!(has_artifact, "Missing artifact update");
    assert!(has_completed, "Missing Completed status update");

    handle.abort();
}

#[tokio::test]
async fn test_get_task() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send a message to create a task
    let event = client
        .send_message(make_message("Hello"))
        .await
        .expect("Failed to send message");

    let task_id = match &event {
        Event::Task(task) => task.id.clone(),
        _ => panic!("Expected Task event"),
    };

    // Small delay to let executor process
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Retrieve the task
    let task = client
        .get_task(&task_id)
        .await
        .expect("Failed to get task");

    assert_eq!(task.id, task_id);

    handle.abort();
}

#[tokio::test]
async fn test_cancel_task() {
    let (addr, handle) = start_test_server(SlowExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send a message to create a task
    let event = client
        .send_message(make_message("Cancel me"))
        .await
        .expect("Failed to send message");

    let task_id = match &event {
        Event::Task(task) => task.id.clone(),
        _ => panic!("Expected Task event"),
    };

    // Small delay so executor starts Working
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Cancel the task
    let canceled = client
        .cancel_task(&task_id)
        .await
        .expect("Failed to cancel task");

    assert_eq!(canceled.id, task_id);
    assert_eq!(canceled.status.state, TaskState::Canceled);

    handle.abort();
}

#[tokio::test]
async fn test_get_nonexistent_task() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let result = client.get_task("nonexistent-task-id").await;
    assert!(result.is_err(), "Expected error for nonexistent task");

    handle.abort();
}
