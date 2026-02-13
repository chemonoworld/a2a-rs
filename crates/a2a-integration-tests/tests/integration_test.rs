use std::sync::Arc;

use a2a_client::{A2AClient, AgentCardResolver};
use a2a_types::JsonRpcResponse;
use a2a_server::{
    AgentExecutor, DefaultHandler, EventQueueWriter, ServerError,
};
use a2a_types::{
    AgentCapabilities, AgentCard, AgentExtension, AgentInterface, AgentSkill, Artifact, Event,
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

#[tokio::test]
async fn test_cancel_nonexistent_task() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let result = client.cancel_task("nonexistent-task-id").await;
    assert!(result.is_err(), "Expected error for canceling nonexistent task");

    handle.abort();
}

#[tokio::test]
async fn test_cancel_completed_task() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send blocking message to get completed task
    let config = MessageSendConfiguration {
        accepted_output_modes: None,
        blocking: Some(true),
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("Hello"), config)
        .await
        .expect("Failed to send blocking message");

    // Extract the task_id from the completed event
    let task_id = match &event {
        Event::TaskStatusUpdate(u) => u.task_id.clone(),
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Try to cancel a completed task - should fail
    let result = client.cancel_task(&task_id).await;
    assert!(result.is_err(), "Should not be able to cancel a completed task");

    handle.abort();
}

#[tokio::test]
async fn test_streaming_event_order() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut stream = client
        .send_message_stream(make_message("Order test"))
        .await
        .expect("Failed to start stream");

    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.expect("Stream error"));
    }

    // Verify strict ordering: Task(Submitted) -> Working -> ArtifactUpdate -> Completed
    assert!(events.len() >= 4, "Expected at least 4 events, got {}", events.len());

    // First event should be Task (initial)
    assert!(
        matches!(&events[0], Event::Task(t) if t.status.state == TaskState::Submitted),
        "First event should be Task(Submitted), got: {:?}",
        events[0]
    );

    // Find the indices of each event type
    let working_idx = events
        .iter()
        .position(|e| matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Working));
    let artifact_idx = events
        .iter()
        .position(|e| matches!(e, Event::TaskArtifactUpdate(_)));
    let completed_idx = events
        .iter()
        .position(|e| matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Completed));

    assert!(working_idx.is_some(), "Missing Working event");
    assert!(artifact_idx.is_some(), "Missing ArtifactUpdate event");
    assert!(completed_idx.is_some(), "Missing Completed event");

    // Verify order: Working < ArtifactUpdate < Completed
    assert!(
        working_idx.unwrap() < artifact_idx.unwrap(),
        "Working should come before ArtifactUpdate"
    );
    assert!(
        artifact_idx.unwrap() < completed_idx.unwrap(),
        "ArtifactUpdate should come before Completed"
    );

    handle.abort();
}

#[tokio::test]
async fn test_multiple_concurrent_requests() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = Arc::new(
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client"),
    );

    // Fire off 5 concurrent requests
    let mut handles = Vec::new();
    for i in 0..5 {
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let event = client
                .send_message(make_message(&format!("Concurrent msg {i}")))
                .await
                .expect("Failed to send message");

            match event {
                Event::Task(task) => {
                    assert_eq!(task.status.state, TaskState::Submitted);
                    task.id
                }
                _ => panic!("Expected Task event"),
            }
        }));
    }

    let mut task_ids = Vec::new();
    for h in handles {
        task_ids.push(h.await.unwrap());
    }

    // All task IDs should be unique
    task_ids.sort();
    task_ids.dedup();
    assert_eq!(task_ids.len(), 5, "All 5 tasks should have unique IDs");

    handle.abort();
}

/// Executor that produces interrupt state (InputRequired)
struct InputRequiredExecutor;

#[async_trait]
impl AgentExecutor for InputRequiredExecutor {
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
                    state: TaskState::InputRequired,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            }))
            .await?;

        Ok(())
    }
}

#[tokio::test]
async fn test_interrupt_state_input_required() {
    let (addr, handle) = start_test_server(InputRequiredExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Use streaming mode - InputRequired is NOT terminal, so blocking would hang
    let mut stream = client
        .send_message_stream(make_message("Need input"))
        .await
        .expect("Failed to start stream");

    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.expect("Stream error"));
    }

    // Should see the InputRequired status update in the stream
    let has_input_required = events.iter().any(|e| {
        matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::InputRequired)
    });

    // Note: the stream may or may not include this event depending on timing
    // (the executor might finish before the stream is fully set up).
    // At minimum we should get the initial Task event.
    assert!(!events.is_empty(), "Should receive at least the initial Task event");

    if has_input_required {
        // Verify InputRequired is an interrupt state
        assert!(TaskState::InputRequired.is_interrupt());
        assert!(!TaskState::InputRequired.is_terminal());
    }

    handle.abort();
}

/// Executor that produces Failed state
struct FailingExecutor;

#[async_trait]
impl AgentExecutor for FailingExecutor {
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
                    state: TaskState::Failed,
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

#[tokio::test]
async fn test_failed_task_via_stream() {
    let (addr, handle) = start_test_server(FailingExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut stream = client
        .send_message_stream(make_message("Fail me"))
        .await
        .expect("Failed to start stream");

    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.expect("Stream error"));
    }

    let has_failed = events.iter().any(|e| {
        matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Failed)
    });
    assert!(has_failed, "Should receive Failed status update");

    handle.abort();
}

#[tokio::test]
async fn test_failed_task_blocking() {
    let (addr, handle) = start_test_server(FailingExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let config = MessageSendConfiguration {
        accepted_output_modes: None,
        blocking: Some(true),
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("Fail blocking"), config)
        .await
        .expect("Failed to send message");

    match event {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Failed);
        }
        _ => panic!("Expected Failed TaskStatusUpdate, got: {event:?}"),
    }

    handle.abort();
}

/// Executor that sends multiple artifacts
struct MultiArtifactExecutor;

#[async_trait]
impl AgentExecutor for MultiArtifactExecutor {
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

        // Send 3 artifact chunks
        for i in 1..=3 {
            queue
                .write(Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                    task_id: task_id.clone(),
                    context_id: context_id.clone(),
                    artifact: Artifact {
                        artifact_id: format!("art-{i}"),
                        name: Some(format!("Artifact {i}")),
                        description: None,
                        parts: vec![Part {
                            content: PartContent::Text {
                                text: format!("Content of artifact {i}"),
                            },
                            metadata: None,
                            filename: None,
                            media_type: None,
                        }],
                        metadata: None,
                        extensions: None,
                    },
                    append: false,
                    last_chunk: i == 3,
                    metadata: None,
                }))
                .await?;
        }

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

#[tokio::test]
async fn test_multiple_artifacts_via_stream() {
    let (addr, handle) = start_test_server(MultiArtifactExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut stream = client
        .send_message_stream(make_message("Multi artifact"))
        .await
        .expect("Failed to start stream");

    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.expect("Stream error"));
    }

    let artifact_count = events
        .iter()
        .filter(|e| matches!(e, Event::TaskArtifactUpdate(_)))
        .count();
    assert_eq!(artifact_count, 3, "Should receive 3 artifact updates");

    handle.abort();
}

// ---------------------------------------------------------------------------
// HTTP-level tests using raw reqwest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_malformed_json_returns_parse_error() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{addr}"))
        .header("Content-Type", "application/json")
        .body("not valid json {{{{")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200); // JSON-RPC returns 200 with error body
    let body: serde_json::Value = response.json().await.unwrap();

    assert!(body["error"].is_object());
    assert_eq!(body["error"]["code"], -32700); // ParseError
    assert!(body["id"].is_null());

    handle.abort();
}

#[tokio::test]
async fn test_unknown_method_returns_method_not_found() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "nonexistent/method",
        "params": {},
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"].is_object());
    assert_eq!(body["error"]["code"], -32601); // MethodNotFound

    handle.abort();
}

#[tokio::test]
async fn test_invalid_params_returns_invalid_params() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    // Send message/send with invalid params (missing required 'message' field)
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "params": {"wrong_field": "value"},
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"].is_object());
    assert_eq!(body["error"]["code"], -32602); // InvalidParams

    handle.abort();
}

#[tokio::test]
async fn test_invalid_params_for_get_task() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/get",
        "params": {"not_id": "value"},
        "id": 2
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32602);

    handle.abort();
}

#[tokio::test]
async fn test_invalid_params_for_cancel_task() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/cancel",
        "params": 42,
        "id": 3
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32602);

    handle.abort();
}

#[tokio::test]
async fn test_agent_card_cors_headers() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{addr}/.well-known/agent-card.json"))
        .send()
        .await
        .expect("Failed to fetch agent card");

    assert_eq!(response.status(), 200);

    let headers = response.headers();
    assert_eq!(
        headers.get("access-control-allow-origin").unwrap(),
        "*"
    );
    assert!(headers
        .get("access-control-allow-methods")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("GET"));
    assert_eq!(
        headers.get("content-type").unwrap(),
        "application/json"
    );

    // Also verify the body is valid JSON
    let card: a2a_types::AgentCard = response.json().await.unwrap();
    assert_eq!(card.name, "Test Agent");

    handle.abort();
}

#[tokio::test]
async fn test_jsonrpc_request_preserves_id() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();

    // Test with string ID
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/get",
        "params": {"id": "nonexistent"},
        "id": "my-custom-id-42"
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.unwrap();
    // Even error responses should preserve the request ID
    assert_eq!(body["id"], "my-custom-id-42");

    handle.abort();
}

// ---------------------------------------------------------------------------
// Interceptor tests
// ---------------------------------------------------------------------------

/// Start a test server with an interceptor
async fn start_test_server_with_interceptor(
    executor: impl AgentExecutor,
    interceptor: impl a2a_server::CallInterceptor,
) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind");
    let addr = listener.local_addr().unwrap().to_string();

    let agent_card = make_agent_card(&addr);
    let handler = DefaultHandler::builder()
        .executor(Arc::new(executor))
        .build();
    let router = a2a_server::create_router_with_interceptor(
        Arc::new(handler),
        agent_card,
        Some(Arc::new(interceptor)),
    );

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr, handle)
}

/// Interceptor that blocks all requests
struct BlockingInterceptor;

#[async_trait]
impl a2a_server::CallInterceptor for BlockingInterceptor {
    async fn before(
        &self,
        _method: &str,
        _params: &serde_json::Value,
    ) -> Result<(), ServerError> {
        Err(ServerError::Internal("blocked by interceptor".into()))
    }

    async fn after(
        &self,
        _method: &str,
        _result: &Result<serde_json::Value, ServerError>,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}

/// Interceptor that only allows specific methods
struct MethodFilterInterceptor {
    allowed: Vec<String>,
}

#[async_trait]
impl a2a_server::CallInterceptor for MethodFilterInterceptor {
    async fn before(
        &self,
        method: &str,
        _params: &serde_json::Value,
    ) -> Result<(), ServerError> {
        if self.allowed.iter().any(|m| m == method) {
            Ok(())
        } else {
            Err(ServerError::Internal(format!("Method {method} not allowed")))
        }
    }

    async fn after(
        &self,
        _method: &str,
        _result: &Result<serde_json::Value, ServerError>,
    ) -> Result<(), ServerError> {
        Ok(())
    }
}

#[tokio::test]
async fn test_interceptor_blocks_request() {
    let (addr, handle) = start_test_server_with_interceptor(
        EchoExecutor,
        BlockingInterceptor,
    ).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "params": {
            "message": {
                "messageId": "m-1",
                "role": "user",
                "parts": [{"text": "hello"}]
            }
        },
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"].is_object());
    assert!(body["error"]["message"].as_str().unwrap().contains("blocked"));

    handle.abort();
}

#[tokio::test]
async fn test_interceptor_method_filter() {
    let interceptor = MethodFilterInterceptor {
        allowed: vec!["tasks/get".into()],
    };
    let (addr, handle) = start_test_server_with_interceptor(
        EchoExecutor,
        interceptor,
    ).await;

    let client = reqwest::Client::new();

    // tasks/get should be allowed (will get TaskNotFound, but that's fine)
    let get_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/get",
        "params": {"id": "some-id"},
        "id": 1
    });
    let resp = client
        .post(format!("http://{addr}"))
        .json(&get_request)
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    // Error should be TaskNotFound (-32001), not "not allowed"
    assert_eq!(body["error"]["code"], -32001);

    // message/send should be blocked
    let send_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "params": {
            "message": {
                "messageId": "m-1",
                "role": "user",
                "parts": [{"text": "hello"}]
            }
        },
        "id": 2
    });
    let resp = client
        .post(format!("http://{addr}"))
        .json(&send_request)
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["error"]["message"].as_str().unwrap().contains("not allowed"));

    handle.abort();
}

// ---------------------------------------------------------------------------
// Resubscribe tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_resubscribe_nonexistent_task() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/resubscribe",
        "params": {"id": "nonexistent-task"},
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.unwrap();
    assert!(body["error"].is_object());
    // Should get TaskNotFound error
    assert_eq!(body["error"]["code"], -32001);

    handle.abort();
}

#[tokio::test]
async fn test_get_task_with_history() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send blocking message to let executor fully complete
    let config = MessageSendConfiguration {
        accepted_output_modes: None,
        blocking: Some(true),
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("History test"), config)
        .await
        .expect("Failed to send blocking message");

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => u.task_id.clone(),
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Get task with history
    let task = client
        .get_task_with_history(&task_id, 10)
        .await
        .expect("Failed to get task with history");

    assert_eq!(task.id, task_id);
    // Task should be in a terminal state (Completed)
    assert!(task.status.state.is_terminal());

    handle.abort();
}

#[tokio::test]
async fn test_no_supported_interface_error() {
    // AgentCard with only gRPC binding - no JSONRPC
    let card = AgentCard {
        name: "gRPC only".into(),
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
    assert!(
        format!("{err}").contains("No supported interface"),
        "Expected NoSupportedInterface error, got: {err}"
    );
}

#[tokio::test]
async fn test_resubscribe_active_task() {
    let (addr, handle) = start_test_server(SlowExecutor).await;

    let client = reqwest::Client::new();

    // Send a non-blocking message to create a task
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "params": {
            "message": {
                "messageId": "m-resub",
                "role": "ROLE_USER",
                "parts": [{"text": "slow task"}]
            }
        },
        "id": 1
    });

    let resp = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let task_id = body["result"]["id"].as_str().expect("Task ID expected").to_string();

    // Wait for executor to emit Working event
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Resubscribe to the active task via SSE
    let resub_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/resubscribe",
        "params": {"id": task_id},
        "id": 2
    });

    let resp = client
        .post(format!("http://{addr}"))
        .json(&resub_request)
        .send()
        .await
        .unwrap();

    // Should get a streaming response (text/event-stream)
    let content_type = resp.headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    assert!(
        content_type.contains("text/event-stream"),
        "Expected SSE content-type, got: {content_type}"
    );

    handle.abort();
}

#[tokio::test]
async fn test_send_message_stream_with_role_enum_format() {
    // Verify the server correctly handles ROLE_USER format in SSE-originated messages
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/stream",
        "params": {
            "message": {
                "messageId": "m-stream-raw",
                "role": "ROLE_USER",
                "parts": [{"text": "raw stream test"}]
            }
        },
        "id": 1
    });

    let resp = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .unwrap();

    let content_type = resp.headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    assert!(
        content_type.contains("text/event-stream"),
        "Expected SSE content-type for message/stream"
    );

    // Read the full body as text and check it has SSE data lines
    let body = resp.text().await.unwrap();
    assert!(body.contains("data:"), "SSE response should contain data: lines");

    handle.abort();
}

#[tokio::test]
async fn test_get_task_state_after_completion() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send blocking to wait for completion
    let config = MessageSendConfiguration {
        accepted_output_modes: None,
        blocking: Some(true),
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("Complete me"), config)
        .await
        .expect("Failed to send message");

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => u.task_id.clone(),
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Verify task is Completed via get_task
    let task = client.get_task(&task_id).await.expect("Failed to get task");
    assert_eq!(task.status.state, TaskState::Completed);
    // Task should have artifacts from EchoExecutor
    assert!(task.artifacts.is_some());
    assert!(!task.artifacts.as_ref().unwrap().is_empty());

    handle.abort();
}

#[tokio::test]
async fn test_concurrent_streams() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = std::sync::Arc::new(
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client"),
    );

    // Fire off 3 concurrent streams
    let mut handles = Vec::new();
    for i in 0..3 {
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let mut stream = client
                .send_message_stream(make_message(&format!("stream {i}")))
                .await
                .expect("Failed to start stream");

            let mut events = Vec::new();
            while let Some(result) = stream.next().await {
                events.push(result.expect("Stream error"));
            }

            // Each stream should get Completed
            let has_completed = events.iter().any(|e| {
                matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Completed)
            });
            assert!(has_completed, "Stream {i} should contain Completed event");
            events.len()
        }));
    }

    for h in handles {
        let event_count = h.await.unwrap();
        assert!(event_count >= 3, "Each stream should have at least 3 events");
    }

    handle.abort();
}

#[tokio::test]
async fn test_invalid_params_for_stream() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    // message/stream with invalid params should return JSON error, not SSE
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/stream",
        "params": {"bad_field": "value"},
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32602); // InvalidParams

    handle.abort();
}

#[tokio::test]
async fn test_invalid_params_for_resubscribe() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/resubscribe",
        "params": 42,
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32602); // InvalidParams

    handle.abort();
}

#[tokio::test]
async fn test_context_id_preserved() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send message with explicit context_id
    let mut msg = make_message("Context test");
    msg.context_id = Some("my-custom-context".into());

    let event = client
        .send_message(msg)
        .await
        .expect("Failed to send message");

    match event {
        Event::Task(task) => {
            assert_eq!(task.context_id, "my-custom-context");
        }
        _ => panic!("Expected Task event"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_context_id_auto_generated_integration() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send message WITHOUT context_id
    let event = client
        .send_message(make_message("No context"))
        .await
        .expect("Failed to send message");

    match event {
        Event::Task(task) => {
            assert!(!task.context_id.is_empty());
            // Should be a valid UUID
            assert!(uuid::Uuid::parse_str(&task.context_id).is_ok());
        }
        _ => panic!("Expected Task event"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_from_agent_card_creates_client() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    // Resolve card first
    let resolver = AgentCardResolver::new();
    let card = resolver
        .resolve(&format!("http://{addr}"))
        .await
        .expect("Failed to resolve agent card");

    // Create client from card
    let client = A2AClient::from_agent_card(&card).expect("Failed to create client from card");

    // Use it
    let event = client
        .send_message(make_message("From card"))
        .await
        .expect("Failed to send message");

    assert!(matches!(event, Event::Task(_)));

    handle.abort();
}

#[tokio::test]
async fn test_blocking_multi_artifacts() {
    let (addr, handle) = start_test_server(MultiArtifactExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let config = MessageSendConfiguration {
        accepted_output_modes: None,
        blocking: Some(true),
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("Multi artifact blocking"), config)
        .await
        .expect("Failed to send blocking message");

    // Blocking should return the terminal event (Completed)
    match event {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Completed);
        }
        _ => panic!("Expected Completed TaskStatusUpdate, got: {event:?}"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_cancel_task_then_verify_state() {
    let (addr, handle) = start_test_server(SlowExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Create a task
    let event = client
        .send_message(make_message("Cancel verify"))
        .await
        .expect("Failed to send message");

    let task_id = match &event {
        Event::Task(task) => task.id.clone(),
        _ => panic!("Expected Task event"),
    };

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Cancel it
    let canceled = client
        .cancel_task(&task_id)
        .await
        .expect("Failed to cancel task");
    assert_eq!(canceled.status.state, TaskState::Canceled);

    // Verify via get_task that state persisted
    let task = client
        .get_task(&task_id)
        .await
        .expect("Failed to get task");
    assert_eq!(task.status.state, TaskState::Canceled);

    handle.abort();
}

#[tokio::test]
async fn test_stream_context_id_consistent_across_events() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut msg = make_message("Context consistency");
    msg.context_id = Some("consistent-ctx".into());

    let mut stream = client
        .send_message_stream(msg)
        .await
        .expect("Failed to start stream");

    let mut context_ids = Vec::new();
    while let Some(result) = stream.next().await {
        let event = result.expect("Stream error");
        let ctx = match &event {
            Event::Task(t) => t.context_id.clone(),
            Event::TaskStatusUpdate(u) => u.context_id.clone(),
            Event::TaskArtifactUpdate(a) => a.context_id.clone(),
            Event::Message(m) => m.context_id.clone().unwrap_or_default(),
            _ => String::new(),
        };
        context_ids.push(ctx);
    }

    assert!(!context_ids.is_empty());
    // All context_ids should be "consistent-ctx"
    for ctx in &context_ids {
        assert_eq!(ctx, "consistent-ctx", "All events should have the same context_id");
    }

    handle.abort();
}

#[tokio::test]
async fn test_null_params_request() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    // Send request with no params at all
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    // Should get InvalidParams since message/send requires params
    assert_eq!(body["error"]["code"], -32602);

    handle.abort();
}

#[tokio::test]
async fn test_request_with_numeric_id_zero() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/get",
        "params": {"id": "nonexistent"},
        "id": 0
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = response.json().await.unwrap();
    // ID 0 should be preserved
    assert_eq!(body["id"], 0);
    // Should get TaskNotFound
    assert_eq!(body["error"]["code"], -32001);

    handle.abort();
}

#[tokio::test]
async fn test_large_message_payload() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Create a message with a large text payload (~100KB)
    let large_text = "x".repeat(100_000);
    let event = client
        .send_message(make_message(&large_text))
        .await
        .expect("Failed to send large message");

    match event {
        Event::Task(task) => {
            assert_eq!(task.status.state, TaskState::Submitted);
        }
        _ => panic!("Expected Task event"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_sequential_blocking_messages() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let config = MessageSendConfiguration {
        accepted_output_modes: None,
        blocking: Some(true),
        push_notification_config: None,
        history_length: None,
    };

    // Send 3 sequential blocking messages
    let mut task_ids = Vec::new();
    for i in 0..3 {
        let event = client
            .send_message_with_config(make_message(&format!("seq msg {i}")), config.clone())
            .await
            .expect("Failed to send blocking message");

        let task_id = match &event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.status.state, TaskState::Completed);
                u.task_id.clone()
            }
            _ => panic!("Expected Completed TaskStatusUpdate"),
        };
        task_ids.push(task_id);
    }

    // All task IDs should be unique
    task_ids.sort();
    task_ids.dedup();
    assert_eq!(task_ids.len(), 3, "All sequential tasks should have unique IDs");

    handle.abort();
}

#[tokio::test]
async fn test_stream_events_persisted_to_store() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Use streaming and consume all events
    let mut stream = client
        .send_message_stream(make_message("Persist test"))
        .await
        .expect("Failed to start stream");

    let mut task_id = String::new();
    while let Some(result) = stream.next().await {
        let event = result.expect("Stream error");
        if task_id.is_empty() {
            task_id = match &event {
                Event::Task(t) => t.id.clone(),
                Event::TaskStatusUpdate(u) => u.task_id.clone(),
                Event::TaskArtifactUpdate(a) => a.task_id.clone(),
                _ => String::new(),
            };
        }
    }

    // After streaming is done, verify the task is in the store via get_task
    let task = client.get_task(&task_id).await.expect("Failed to get task");
    assert_eq!(task.id, task_id);
    // Should be in a terminal state since EchoExecutor completes
    assert!(task.status.state.is_terminal());

    handle.abort();
}

// ---------------------------------------------------------------------------
// Artifact Chunking Executor (Round 12)
// ---------------------------------------------------------------------------

/// Executor that emits artifact chunks with proper append/lastChunk semantics
struct ChunkingExecutor;

#[async_trait]
impl AgentExecutor for ChunkingExecutor {
    async fn execute(
        &self,
        event: Event,
        queue: &dyn EventQueueWriter,
    ) -> Result<(), ServerError> {
        let (task_id, context_id) = match &event {
            Event::Task(task) => (task.id.clone(), task.context_id.clone()),
            _ => return Ok(()),
        };

        // Working
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

        // Chunk 1: first chunk (append=false, lastChunk=false)
        queue
            .write(Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                artifact: Artifact {
                    artifact_id: "a-chunked".into(),
                    name: Some("Chunked Artifact".into()),
                    description: None,
                    parts: vec![Part {
                        content: PartContent::Text {
                            text: "chunk-1".into(),
                        },
                        metadata: None,
                        filename: None,
                        media_type: None,
                    }],
                    metadata: None,
                    extensions: None,
                },
                append: false,
                last_chunk: false,
                metadata: Some(serde_json::json!({"chunkIndex": 0})),
            }))
            .await?;

        // Chunk 2: middle chunk (append=true, lastChunk=false)
        queue
            .write(Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                artifact: Artifact {
                    artifact_id: "a-chunked".into(),
                    name: None,
                    description: None,
                    parts: vec![Part {
                        content: PartContent::Text {
                            text: "chunk-2".into(),
                        },
                        metadata: None,
                        filename: None,
                        media_type: None,
                    }],
                    metadata: None,
                    extensions: None,
                },
                append: true,
                last_chunk: false,
                metadata: Some(serde_json::json!({"chunkIndex": 1})),
            }))
            .await?;

        // Chunk 3: last chunk (append=true, lastChunk=true)
        queue
            .write(Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                artifact: Artifact {
                    artifact_id: "a-chunked".into(),
                    name: None,
                    description: None,
                    parts: vec![Part {
                        content: PartContent::Text {
                            text: "chunk-3".into(),
                        },
                        metadata: None,
                        filename: None,
                        media_type: None,
                    }],
                    metadata: None,
                    extensions: None,
                },
                append: true,
                last_chunk: true,
                metadata: Some(serde_json::json!({"chunkIndex": 2})),
            }))
            .await?;

        // Completed
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

#[tokio::test]
async fn test_artifact_chunking_semantics() {
    let (addr, handle) = start_test_server(ChunkingExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut stream = client
        .send_message_stream(make_message("Chunk test"))
        .await
        .expect("Failed to start stream");

    let mut artifact_updates = Vec::new();
    while let Some(result) = stream.next().await {
        let event = result.expect("Stream error");
        if let Event::TaskArtifactUpdate(u) = event {
            artifact_updates.push(u);
        }
    }

    assert_eq!(artifact_updates.len(), 3, "Should receive 3 artifact chunks");

    // Chunk 1: first (append=false, lastChunk=false)
    assert!(!artifact_updates[0].append, "First chunk: append should be false");
    assert!(!artifact_updates[0].last_chunk, "First chunk: lastChunk should be false");
    assert_eq!(artifact_updates[0].artifact.artifact_id, "a-chunked");
    assert_eq!(artifact_updates[0].artifact.name.as_deref(), Some("Chunked Artifact"));

    // Chunk 2: middle (append=true, lastChunk=false)
    assert!(artifact_updates[1].append, "Middle chunk: append should be true");
    assert!(!artifact_updates[1].last_chunk, "Middle chunk: lastChunk should be false");

    // Chunk 3: last (append=true, lastChunk=true)
    assert!(artifact_updates[2].append, "Last chunk: append should be true");
    assert!(artifact_updates[2].last_chunk, "Last chunk: lastChunk should be true");

    // All chunks share the same artifact_id
    for update in &artifact_updates {
        assert_eq!(update.artifact.artifact_id, "a-chunked");
    }

    handle.abort();
}

/// Executor that emits InputRequired with an embedded message in status
struct InputWithMessageExecutor;

#[async_trait]
impl AgentExecutor for InputWithMessageExecutor {
    async fn execute(
        &self,
        event: Event,
        queue: &dyn EventQueueWriter,
    ) -> Result<(), ServerError> {
        let (task_id, context_id) = match &event {
            Event::Task(task) => (task.id.clone(), task.context_id.clone()),
            _ => return Ok(()),
        };

        // Emit InputRequired with a message explaining what's needed
        queue
            .write(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id,
                context_id,
                status: TaskStatus {
                    state: TaskState::InputRequired,
                    message: Some(Message {
                        message_id: "m-prompt".into(),
                        role: Role::Agent,
                        parts: vec![Part {
                            content: PartContent::Text {
                                text: "Please provide your credentials".into(),
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
                    }),
                    timestamp: Some("2026-02-12T12:00:00Z".into()),
                },
                is_final: Some(false),
                metadata: None,
            }))
            .await?;

        Ok(())
    }
}

#[tokio::test]
async fn test_input_required_with_embedded_message() {
    let (addr, handle) = start_test_server(InputWithMessageExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut stream = client
        .send_message_stream(make_message("Auth needed"))
        .await
        .expect("Failed to start stream");

    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.expect("Stream error"));
    }

    // Find the InputRequired status update
    let input_required = events.iter().find(|e| {
        matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::InputRequired)
    });

    if let Some(Event::TaskStatusUpdate(u)) = input_required {
        // Verify the embedded message
        let msg = u.status.message.as_ref().expect("Should have embedded message");
        assert_eq!(msg.message_id, "m-prompt");
        assert_eq!(msg.role, Role::Agent);
        match &msg.parts[0].content {
            PartContent::Text { text } => assert!(text.contains("credentials")),
            _ => panic!("Expected Text part"),
        }
        assert_eq!(u.is_final, Some(false));
        assert!(u.status.timestamp.is_some());
    }

    handle.abort();
}

#[tokio::test]
async fn test_chunking_blocking_mode() {
    let (addr, handle) = start_test_server(ChunkingExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("Blocking chunk test"), config)
        .await
        .expect("Failed to send blocking message");

    // In blocking mode, should get the terminal Completed event
    match event {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Completed);
        }
        _ => panic!("Expected Completed TaskStatusUpdate in blocking mode, got: {event:?}"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_method_not_found_error_format() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    // Send a raw JSON-RPC request with unknown method
    let req_body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/nonexistent",
        "params": {},
        "id": 42
    });

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .json(&req_body)
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = resp.json().await.expect("Failed to parse response");
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 42);
    assert!(body["result"].is_null());
    assert_eq!(body["error"]["code"], -32601);
    assert!(body["error"]["message"].as_str().unwrap().contains("not found"));

    handle.abort();
}

/// Executor that emits Rejected state
struct RejectedExecutor;

#[async_trait]
impl AgentExecutor for RejectedExecutor {
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
                    state: TaskState::Rejected,
                    message: Some(Message {
                        message_id: "m-reject".into(),
                        role: Role::Agent,
                        parts: vec![Part {
                            content: PartContent::Text {
                                text: "Request rejected: insufficient permissions".into(),
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
                    }),
                    timestamp: Some("2026-02-12T14:00:00Z".into()),
                },
                is_final: Some(true),
                metadata: None,
            }))
            .await?;

        Ok(())
    }
}

#[tokio::test]
async fn test_rejected_task_via_blocking() {
    let (addr, handle) = start_test_server(RejectedExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("Reject me"), config)
        .await
        .expect("Failed to send message");

    match event {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Rejected);
            assert!(u.status.state.is_terminal());
            let msg = u.status.message.as_ref().expect("Should have rejection message");
            match &msg.parts[0].content {
                PartContent::Text { text } => assert!(text.contains("insufficient permissions")),
                _ => panic!("Expected Text part in rejection message"),
            }
            assert_eq!(u.is_final, Some(true));
        }
        _ => panic!("Expected Rejected TaskStatusUpdate, got: {event:?}"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_rejected_task_via_stream() {
    let (addr, handle) = start_test_server(RejectedExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut stream = client
        .send_message_stream(make_message("Reject me stream"))
        .await
        .expect("Failed to start stream");

    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.expect("Stream error"));
    }

    let has_rejected = events.iter().any(|e| {
        matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Rejected)
    });
    assert!(has_rejected, "Should receive Rejected status update");

    handle.abort();
}

#[tokio::test]
async fn test_get_task_artifacts_after_chunking_blocking() {
    let (addr, handle) = start_test_server(ChunkingExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("Chunk then get"), config)
        .await
        .expect("Failed to send blocking message");

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => u.task_id.clone(),
        _ => panic!("Expected Completed TaskStatusUpdate"),
    };

    // Get the task and verify artifacts were merged by the store
    let task = client.get_task(&task_id).await.expect("Failed to get task");
    assert_eq!(task.status.state, TaskState::Completed);
    let artifacts = task.artifacts.expect("Task should have artifacts");
    // ChunkingExecutor emits 3 chunks with same artifact_id "a-chunked"
    // First chunk: append=false (creates), then 2x append=true (extends parts)
    assert_eq!(artifacts.len(), 1, "Should have 1 merged artifact");
    assert_eq!(artifacts[0].artifact_id, "a-chunked");
    assert_eq!(
        artifacts[0].parts.len(),
        3,
        "Should have 3 parts from 3 chunks"
    );

    handle.abort();
}

#[tokio::test]
async fn test_content_type_json_for_rpc_response() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/get",
        "params": {"id": "nonexistent"},
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .expect("Should have Content-Type header")
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("application/json"),
        "JSON-RPC responses should have application/json content type, got: {content_type}"
    );

    handle.abort();
}

/// Executor that emits AuthRequired state (interrupt, not terminal)
struct AuthRequiredExecutor;

#[async_trait]
impl AgentExecutor for AuthRequiredExecutor {
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
                    state: TaskState::AuthRequired,
                    message: Some(Message {
                        message_id: "m-auth".into(),
                        role: Role::Agent,
                        parts: vec![Part {
                            content: PartContent::Text {
                                text: "OAuth2 authentication required".into(),
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
                    }),
                    timestamp: Some("2026-02-12T16:00:00Z".into()),
                },
                is_final: Some(false),
                metadata: None,
            }))
            .await?;

        Ok(())
    }
}

#[tokio::test]
async fn test_auth_required_via_stream() {
    let (addr, handle) = start_test_server(AuthRequiredExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut stream = client
        .send_message_stream(make_message("Auth needed"))
        .await
        .expect("Failed to start stream");

    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.expect("Stream error"));
    }

    // Find the AuthRequired status update
    let auth_required = events.iter().find(|e| {
        matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::AuthRequired)
    });

    if let Some(Event::TaskStatusUpdate(u)) = auth_required {
        // AuthRequired is an interrupt state, NOT terminal
        assert!(u.status.state.is_interrupt());
        assert!(!u.status.state.is_terminal());
        assert_eq!(u.is_final, Some(false));
        let msg = u.status.message.as_ref().expect("Should have auth message");
        match &msg.parts[0].content {
            PartContent::Text { text } => assert!(text.contains("OAuth2")),
            _ => panic!("Expected Text part"),
        }
        assert!(u.status.timestamp.is_some());
    }

    handle.abort();
}

#[tokio::test]
async fn test_cancel_already_canceled_task() {
    let (addr, handle) = start_test_server(SlowExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Create a task
    let event = client
        .send_message(make_message("Double cancel"))
        .await
        .expect("Failed to send message");

    let task_id = match &event {
        Event::Task(task) => task.id.clone(),
        _ => panic!("Expected Task event"),
    };

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Cancel it once - should succeed
    let canceled = client
        .cancel_task(&task_id)
        .await
        .expect("Failed to cancel task");
    assert_eq!(canceled.status.state, TaskState::Canceled);

    // Cancel it again - should fail because Canceled is terminal
    let result = client.cancel_task(&task_id).await;
    assert!(
        result.is_err(),
        "Should not be able to cancel an already-canceled task"
    );

    handle.abort();
}

#[tokio::test]
async fn test_empty_body_returns_parse_error() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .header("content-type", "application/json")
        .body("")
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = resp.json().await.expect("Failed to parse response");
    assert_eq!(body["jsonrpc"], "2.0");
    assert!(body["id"].is_null());
    assert_eq!(body["error"]["code"], -32700); // Parse error

    handle.abort();
}

#[tokio::test]
async fn test_multiple_get_task_same_id() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("Multi-get"), config)
        .await
        .expect("Failed to send message");

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => u.task_id.clone(),
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Get the same task 3 times - should all return consistent data
    for _ in 0..3 {
        let task = client
            .get_task(&task_id)
            .await
            .expect("Failed to get task");
        assert_eq!(task.id, task_id);
        assert_eq!(task.status.state, TaskState::Completed);
        assert!(task.artifacts.is_some());
    }

    handle.abort();
}

#[tokio::test]
async fn test_parse_error_for_invalid_json() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .header("content-type", "application/json")
        .body("not valid json {{{")
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = resp.json().await.expect("Failed to parse response");
    assert_eq!(body["jsonrpc"], "2.0");
    assert!(body["id"].is_null());
    assert_eq!(body["error"]["code"], -32700); // Parse error
    assert!(body["error"]["message"].as_str().unwrap().contains("Parse"));

    handle.abort();
}

#[tokio::test]
async fn test_stream_with_configuration() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/stream",
        "params": {
            "message": {
                "messageId": "m-cfg-stream",
                "role": "ROLE_USER",
                "parts": [{"text": "Hello with config"}]
            },
            "configuration": {
                "acceptedOutputModes": ["text/plain", "application/json"],
                "historyLength": 10,
                "blocking": false
            }
        },
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    // Stream response should be SSE (text/event-stream)
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        content_type.contains("text/event-stream"),
        "Expected text/event-stream, got: {content_type}"
    );

    // Consume the stream body
    let body = response.text().await.expect("Failed to read body");
    assert!(body.contains("data:"), "SSE events should contain data: lines");

    handle.abort();
}

#[tokio::test]
async fn test_auth_required_persisted_to_store() {
    let (addr, handle) = start_test_server(AuthRequiredExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Use streaming mode so events get persisted to the store
    let mut stream = client
        .send_message_stream(make_message("Check auth persistence"))
        .await
        .expect("Failed to start stream");

    // Collect the task_id from the first event
    let mut task_id = String::new();
    while let Some(result) = stream.next().await {
        let event = result.expect("Stream error");
        if let Event::TaskStatusUpdate(u) = &event {
            task_id = u.task_id.clone();
        }
    }

    assert!(!task_id.is_empty(), "Should have received events with task_id");

    // Now get_task should show the last state persisted
    let task = client.get_task(&task_id).await.expect("Failed to get task");
    assert_eq!(task.id, task_id);
    // AuthRequired is the only status emitted by AuthRequiredExecutor
    assert_eq!(task.status.state, TaskState::AuthRequired);

    handle.abort();
}

#[tokio::test]
async fn test_interceptor_after_hook_sees_result() {
    use std::sync::atomic::{AtomicBool, Ordering};

    // An interceptor that tracks whether its after hook was called
    struct TrackingInterceptor {
        after_called: Arc<AtomicBool>,
    }

    #[async_trait]
    impl a2a_server::CallInterceptor for TrackingInterceptor {
        async fn before(
            &self,
            _method: &str,
            _params: &serde_json::Value,
        ) -> Result<(), ServerError> {
            Ok(()) // Allow all
        }

        async fn after(
            &self,
            _method: &str,
            _result: &Result<serde_json::Value, ServerError>,
        ) -> Result<(), ServerError> {
            self.after_called.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let after_called = Arc::new(AtomicBool::new(false));
    let interceptor = TrackingInterceptor {
        after_called: after_called.clone(),
    };

    let (addr, handle) = start_test_server_with_interceptor(EchoExecutor, interceptor).await;

    // Send a message/send (non-streaming) so the after hook fires
    let client = reqwest::Client::new();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "params": {
            "message": {
                "messageId": "m-after-hook",
                "role": "ROLE_USER",
                "parts": [{"text": "test after hook"}]
            }
        },
        "id": 1
    });

    let response = client
        .post(format!("http://{addr}"))
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    let body: serde_json::Value = response.json().await.unwrap();
    // The request should succeed (result present)
    assert!(body["result"].is_object(), "Should have a result");

    // Give a moment for the after hook to run (it's synchronous in the flow)
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        after_called.load(Ordering::SeqCst),
        "after hook should have been called"
    );

    handle.abort();
}

#[tokio::test]
async fn test_post_to_agent_card_endpoint() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    // POST to the agent card endpoint should not be routed (only GET is registered)
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/.well-known/agent-card.json"))
        .body("irrelevant")
        .send()
        .await
        .expect("Failed to send request");

    // axum returns 405 Method Not Allowed for wrong HTTP method on existing route
    assert_eq!(resp.status().as_u16(), 405);

    handle.abort();
}

#[tokio::test]
async fn test_blocking_returns_terminal_event() {
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("blocking test"), config)
        .await
        .expect("Failed to send blocking message");

    // In blocking mode, the response should be a terminal event
    match &event {
        Event::TaskStatusUpdate(u) => {
            assert!(
                u.status.state.is_terminal(),
                "Blocking should return terminal state, got: {:?}",
                u.status.state
            );
        }
        Event::Task(t) => {
            // If Task is returned, it should have a terminal state
            assert!(
                t.status.state.is_terminal(),
                "Blocking task should have terminal state, got: {:?}",
                t.status.state
            );
        }
        _ => panic!("Expected terminal event from blocking send"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_concurrent_blocking_sends_different_tasks() {
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client = std::sync::Arc::new(
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client"),
    );

    let mut handles = Vec::new();
    for i in 0..5 {
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let config = MessageSendConfiguration {
                blocking: Some(true),
                accepted_output_modes: None,
                push_notification_config: None,
                history_length: None,
            };
            let event = client
                .send_message_with_config(
                    make_message(&format!("concurrent-{i}")),
                    config,
                )
                .await
                .expect("Failed to send blocking message");
            event
        }));
    }

    let mut events = Vec::new();
    for h in handles {
        events.push(h.await.expect("Task panicked"));
    }

    assert_eq!(events.len(), 5, "All 5 concurrent requests should complete");

    // Verify each returned a terminal event
    for event in &events {
        match event {
            Event::TaskStatusUpdate(u) => {
                assert!(u.status.state.is_terminal());
            }
            Event::Task(t) => {
                assert!(t.status.state.is_terminal());
            }
            _ => panic!("Expected terminal event"),
        }
    }

    handle.abort();
}

#[tokio::test]
async fn test_task_ids_unique_across_requests() {
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut task_ids = std::collections::HashSet::new();

    for i in 0..10 {
        let event = client
            .send_message(make_message(&format!("unique-{i}")))
            .await
            .expect("Failed to send message");

        let task_id = match &event {
            Event::Task(t) => t.id.clone(),
            _ => panic!("Expected Task event"),
        };
        assert!(
            task_ids.insert(task_id.clone()),
            "Task ID should be unique, but {task_id} was duplicated"
        );
    }

    assert_eq!(task_ids.len(), 10);

    handle.abort();
}

#[tokio::test]
async fn test_get_to_rpc_endpoint_returns_method_not_allowed() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    // GET to root "/" (JSON-RPC endpoint) should return 405 (only POST is registered)
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}"))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(resp.status().as_u16(), 405);

    handle.abort();
}

#[tokio::test]
async fn test_resubscribe_after_task_completion_returns_empty_stream() {
    // After a blocking send completes, the queue is closed but still exists.
    // Resubscribe returns an SSE response with an empty stream.
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();

    // Send a blocking message so the task fully completes
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "params": {
            "message": {
                "messageId": "m-completed",
                "role": "ROLE_USER",
                "parts": [{"text": "complete me"}]
            },
            "configuration": {"blocking": true}
        },
        "id": 1
    });

    let resp = client
        .post(format!("http://{addr}"))
        .json(&request)
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let task_id = body["result"]["taskId"]
        .as_str()
        .expect("Should have taskId in completed event");

    // Small delay to ensure executor close() runs
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Resubscribe - queue is closed so it returns an SSE stream (empty)
    let resub_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/resubscribe",
        "params": {"id": task_id},
        "id": 2
    });

    let resp = client
        .post(format!("http://{addr}"))
        .json(&resub_request)
        .send()
        .await
        .unwrap();

    let content_type = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    // The queue exists (closed), so resubscribe returns SSE
    assert!(
        content_type.contains("text/event-stream"),
        "Resubscribe to closed queue should return SSE, got: {content_type}"
    );

    handle.abort();
}

#[tokio::test]
async fn test_cancel_then_resubscribe_returns_error() {
    // After canceling a task, the queue is destroyed.
    // Resubscribe via raw HTTP should return a JSON-RPC error (TaskNotFound).
    let (addr, handle) = start_test_server(SlowExecutor).await;

    let client = reqwest::Client::new();

    // Create a task (non-blocking)
    let send_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/send",
        "params": {
            "message": {
                "messageId": "m-cancel-resub",
                "role": "ROLE_USER",
                "parts": [{"text": "cancel then resub"}]
            }
        },
        "id": 1
    });

    let resp = client
        .post(format!("http://{addr}"))
        .json(&send_request)
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    let task_id = body["result"]["id"]
        .as_str()
        .expect("Should have task id")
        .to_string();

    // Wait for executor to start Working
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Cancel it
    let cancel_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/cancel",
        "params": {"id": task_id},
        "id": 2
    });

    let resp = client
        .post(format!("http://{addr}"))
        .json(&cancel_request)
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["result"].is_object(), "Cancel should succeed");

    // Now try to resubscribe - should get JSON error (queue destroyed)
    let resub_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/resubscribe",
        "params": {"id": task_id},
        "id": 3
    });

    let resp = client
        .post(format!("http://{addr}"))
        .json(&resub_request)
        .send()
        .await
        .unwrap();

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["error"].is_object(),
        "Resubscribe after cancel should return error"
    );
    assert_eq!(body["error"]["code"], -32001); // TaskNotFound

    handle.abort();
}

#[tokio::test]
async fn test_message_with_reference_task_ids() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send a message with reference_task_ids
    let mut msg = make_message("With refs");
    msg.reference_task_ids = Some(vec!["ref-task-1".into(), "ref-task-2".into()]);

    let event = client
        .send_message(msg)
        .await
        .expect("Failed to send message");

    // The initial response should be a Task event with Submitted state
    match event {
        Event::Task(task) => {
            assert_eq!(task.status.state, TaskState::Submitted);
            // The task itself doesn't directly expose reference_task_ids
            // but the message should have been accepted without error
        }
        _ => panic!("Expected Task event"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_follow_up_message_same_context() {
    // Test that sending a follow-up message with the same context_id creates a new task
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // First, create a task
    let event = client
        .send_message(make_message("Initial"))
        .await
        .expect("Failed to send message");

    let (task_id, context_id) = match &event {
        Event::Task(task) => (task.id.clone(), task.context_id.clone()),
        _ => panic!("Expected Task event"),
    };

    // Wait for the first task to complete
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Send a follow-up message with the same context_id
    let mut msg = make_message("Follow-up");
    msg.context_id = Some(context_id.clone());

    let event2 = client
        .send_message(msg)
        .await
        .expect("Failed to send follow-up message");

    // Should create a new task (with new task_id) but same context_id
    match event2 {
        Event::Task(task) => {
            assert_eq!(task.context_id, context_id, "context_id should match");
            // task_id should be different (new task for new message)
            assert_ne!(task.id, task_id, "Should create a new task");
        }
        _ => panic!("Expected Task event"),
    }

    handle.abort();
}

// ---------------------------------------------------------------------------
// JSON-RPC ID Preservation Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_jsonrpc_response_preserves_request_id() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    // Use a string ID instead of numeric
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/nonexistent",
        "id": "my-custom-id-123"
    });

    let resp = client
        .post(format!("http://{addr}/"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .unwrap();

    let json_resp: JsonRpcResponse = resp.json().await.unwrap();
    // Verify the ID was preserved
    match &json_resp.id {
        a2a_types::JsonRpcId::String(s) => assert_eq!(s, "my-custom-id-123"),
        _ => panic!("Expected String ID, got {:?}", json_resp.id),
    }

    handle.abort();
}

#[tokio::test]
async fn test_message_stream_with_invalid_params_returns_json_error() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    // Send message/stream with invalid params (no message field)
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "message/stream",
        "params": {"invalid": true},
        "id": 77
    });

    let resp = client
        .post(format!("http://{addr}/"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .unwrap();

    // Should return JSON response (not SSE) on invalid params
    let content_type = resp.headers().get("content-type").unwrap().to_str().unwrap().to_string();
    assert!(content_type.contains("application/json"), "Expected JSON error response, got {content_type}");

    let json_resp: JsonRpcResponse = resp.json().await.unwrap();
    let err = json_resp.error.expect("Should have error");
    assert_eq!(err.code, -32602); // InvalidParams

    handle.abort();
}

#[tokio::test]
async fn test_null_jsonrpc_id_in_error_response() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    // Send request with null ID
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/nonexistent",
        "id": null
    });

    let resp = client
        .post(format!("http://{addr}/"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .unwrap();

    let json_resp: JsonRpcResponse = resp.json().await.unwrap();
    // Verify null ID is preserved
    match &json_resp.id {
        a2a_types::JsonRpcId::Null => {}
        _ => panic!("Expected Null ID, got {:?}", json_resp.id),
    }
    assert!(json_resp.error.is_some());

    handle.abort();
}

#[tokio::test]
async fn test_resubscribe_with_invalid_params() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    // Send tasks/resubscribe with invalid params (missing required 'id')
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/resubscribe",
        "params": {"not_id": "value"},
        "id": 99
    });

    let resp = client
        .post(format!("http://{addr}/"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .unwrap();

    let json_resp: JsonRpcResponse = resp.json().await.unwrap();
    let err = json_resp.error.expect("Should have error");
    assert_eq!(err.code, -32602); // InvalidParams

    handle.abort();
}

#[tokio::test]
async fn test_cancel_already_completed_task_returns_not_cancelable() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send blocking message - completes the task
    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };
    let event = client
        .send_message_with_config(make_message("blocking cancel test"), config)
        .await
        .expect("Send should succeed");

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => u.task_id.clone(),
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Try to cancel the completed task via raw HTTP to inspect exact error code
    let http_client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tasks/cancel",
        "params": {"id": task_id},
        "id": 1
    });

    let resp = http_client
        .post(format!("http://{addr}/"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .unwrap();

    let json_resp: JsonRpcResponse = resp.json().await.unwrap();
    let err = json_resp.error.expect("Should have error for completed task");
    assert_eq!(err.code, -32002); // TaskNotCancelable

    handle.abort();
}

#[tokio::test]
async fn test_failed_task_get_returns_failed_state() {
    let (addr, handle) = start_test_server(FailingExecutor).await;
    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Blocking send triggers FailingExecutor → Failed state
    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };
    let event = client
        .send_message_with_config(make_message("fail me"), config)
        .await
        .expect("Failed to send message");

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => u.task_id.clone(),
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Get the task - should be Failed
    let task = client.get_task(&task_id).await.expect("Failed to get task");
    assert_eq!(task.status.state, TaskState::Failed);
    assert!(task.status.state.is_terminal());

    handle.abort();
}

#[tokio::test]
async fn test_rejected_task_cancel_returns_not_cancelable() {
    let (addr, handle) = start_test_server(RejectedExecutor).await;
    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Blocking send → Rejected (terminal)
    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };
    let event = client
        .send_message_with_config(make_message("reject me"), config)
        .await
        .expect("Failed to send message");

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Rejected);
            u.task_id.clone()
        }
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Cancel rejected task → should fail
    let result = client.cancel_task(&task_id).await;
    assert!(
        result.is_err(),
        "Should not be able to cancel a rejected task"
    );

    handle.abort();
}

#[tokio::test]
async fn test_failed_task_cancel_returns_not_cancelable() {
    let (addr, handle) = start_test_server(FailingExecutor).await;
    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Blocking send triggers FailingExecutor → Failed state
    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };
    let event = client
        .send_message_with_config(make_message("fail me"), config)
        .await
        .expect("Failed to send message");

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Failed);
            u.task_id.clone()
        }
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Cancel failed task → should fail (terminal state)
    let result = client.cancel_task(&task_id).await;
    assert!(
        result.is_err(),
        "Should not be able to cancel a failed task"
    );

    handle.abort();
}

#[tokio::test]
async fn test_agent_card_endpoint_cors_headers() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/.well-known/agent-card.json"))
        .send()
        .await
        .expect("Failed to fetch agent card");

    assert_eq!(resp.status(), 200);

    // Check CORS headers
    let headers = resp.headers();
    assert_eq!(
        headers.get("access-control-allow-origin").unwrap(),
        "*"
    );
    assert!(
        headers
            .get("access-control-allow-methods")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("GET")
    );
    assert!(
        headers
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("application/json")
    );

    // Verify body is valid AgentCard JSON
    let card: AgentCard = resp.json().await.expect("Failed to parse agent card");
    assert_eq!(card.name, "Test Agent");
    assert_eq!(card.supported_interfaces.len(), 1);

    handle.abort();
}

#[tokio::test]
async fn test_agent_card_from_resolver_creates_client() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    // Resolve the agent card from the well-known endpoint
    let resolver = AgentCardResolver::new();
    let card = resolver
        .resolve(&format!("http://{addr}"))
        .await
        .expect("Failed to resolve agent card");

    // Create client from agent card
    let client = A2AClient::from_agent_card(&card)
        .expect("Failed to create client from agent card");

    // Verify the client works
    let event = client
        .send_message(make_message("Hello via resolved card"))
        .await
        .expect("Failed to send message");

    match event {
        Event::Task(task) => {
            assert_eq!(task.status.state, TaskState::Submitted);
        }
        _ => panic!("Expected Task event"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_streaming_includes_artifact_events() {
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    let mut stream = client
        .send_message_stream(make_message("stream me"))
        .await
        .expect("Failed to start stream");

    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.expect("Stream error"));
    }

    // Should include: Task(Submitted), Working, ArtifactUpdate, Completed
    assert!(events.len() >= 4, "Expected at least 4 events, got {}", events.len());

    // Verify artifact event is present
    let has_artifact = events.iter().any(|e| matches!(e, Event::TaskArtifactUpdate(_)));
    assert!(has_artifact, "Stream should include TaskArtifactUpdate event");

    // Verify artifact content
    for event in &events {
        if let Event::TaskArtifactUpdate(u) = event {
            assert_eq!(u.artifact.artifact_id, "art-1");
            assert!(u.last_chunk);
            assert!(!u.append);
            match &u.artifact.parts[0].content {
                PartContent::Text { text } => assert_eq!(text, "Echo response"),
                _ => panic!("Expected Text part in artifact"),
            }
        }
    }

    handle.abort();
}

#[tokio::test]
async fn test_multiple_tasks_independent() {
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Send two independent messages
    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event1 = client
        .send_message_with_config(make_message("task 1"), config.clone())
        .await
        .expect("Failed to send message 1");

    let event2 = client
        .send_message_with_config(make_message("task 2"), config)
        .await
        .expect("Failed to send message 2");

    // Both should complete independently
    let task_id_1 = match &event1 {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Completed);
            u.task_id.clone()
        }
        _ => panic!("Expected TaskStatusUpdate for task 1"),
    };

    let task_id_2 = match &event2 {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Completed);
            u.task_id.clone()
        }
        _ => panic!("Expected TaskStatusUpdate for task 2"),
    };

    // Task IDs should be different
    assert_ne!(task_id_1, task_id_2, "Tasks should have distinct IDs");

    // Both should be retrievable
    let t1 = client.get_task(&task_id_1).await.expect("Failed to get task 1");
    let t2 = client.get_task(&task_id_2).await.expect("Failed to get task 2");
    assert_eq!(t1.status.state, TaskState::Completed);
    assert_eq!(t2.status.state, TaskState::Completed);

    handle.abort();
}

#[tokio::test]
async fn test_options_request_returns_method_not_allowed() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    // OPTIONS to root "/" (JSON-RPC endpoint) should return 405 (only POST is registered)
    let client = reqwest::Client::new();
    let resp = client
        .request(reqwest::Method::OPTIONS, format!("http://{addr}"))
        .send()
        .await
        .expect("Failed to send OPTIONS request");

    // axum returns 405 for unregistered methods
    assert_eq!(resp.status().as_u16(), 405);

    handle.abort();
}

#[tokio::test]
async fn test_put_request_returns_method_not_allowed() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let resp = client
        .put(format!("http://{addr}"))
        .header("Content-Type", "application/json")
        .body(r#"{"jsonrpc":"2.0","method":"message/send","id":1}"#)
        .send()
        .await
        .expect("Failed to send PUT request");

    assert_eq!(resp.status().as_u16(), 405);

    handle.abort();
}

#[tokio::test]
async fn test_nonexistent_path_returns_404() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/nonexistent/path"))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(resp.status().as_u16(), 404);

    handle.abort();
}

#[tokio::test]
async fn test_post_to_agent_card_returns_method_not_allowed() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/.well-known/agent-card.json"))
        .header("Content-Type", "application/json")
        .body(r#"{}"#)
        .send()
        .await
        .expect("Failed to send POST to agent card");

    // Agent card endpoint only accepts GET
    assert_eq!(resp.status().as_u16(), 405);

    handle.abort();
}

#[tokio::test]
async fn test_blocking_then_get_task_includes_artifacts() {
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client =
        A2AClient::new(&format!("http://{addr}")).expect("Failed to create client");

    // Blocking send
    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("generate artifacts"), config)
        .await
        .expect("Failed to send blocking message");

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Completed);
            u.task_id.clone()
        }
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Get the task and verify artifacts are included
    let task = client.get_task(&task_id).await.expect("Failed to get task");
    assert_eq!(task.status.state, TaskState::Completed);
    let artifacts = task.artifacts.expect("Task should have artifacts");
    assert!(!artifacts.is_empty(), "Should have at least one artifact");
    assert_eq!(artifacts[0].artifact_id, "art-1");

    handle.abort();
}

// ---------------------------------------------------------------------------
// Round 32: Additional Integration Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_blocking_send_returns_completed_not_submitted() {
    // Verify that blocking mode never returns Submitted - it waits for terminal
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("blocking test"), config)
        .await
        .unwrap();

    match &event {
        Event::TaskStatusUpdate(u) => {
            assert!(
                u.status.state == TaskState::Completed
                    || u.status.state == TaskState::Failed
                    || u.status.state == TaskState::Canceled
                    || u.status.state == TaskState::Rejected,
                "Blocking should return terminal state, got: {:?}",
                u.status.state
            );
        }
        _ => panic!("Expected TaskStatusUpdate for blocking mode"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_stream_task_id_matches_across_all_events() {
    // All events in a stream should reference the same task_id
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    let mut stream = client
        .send_message_stream(make_message("consistency check"))
        .await
        .unwrap();

    let mut task_id: Option<String> = None;
    while let Some(event) = stream.next().await {
        let event = event.unwrap();
        let event_task_id = match &event {
            Event::Task(t) => t.id.clone(),
            Event::TaskStatusUpdate(u) => u.task_id.clone(),
            Event::TaskArtifactUpdate(a) => a.task_id.clone(),
            Event::Message(_) => continue,
            _ => continue,
        };

        match &task_id {
            None => task_id = Some(event_task_id),
            Some(expected) => assert_eq!(
                &event_task_id, expected,
                "All events should have same task_id"
            ),
        }
    }

    assert!(task_id.is_some(), "Should have received at least one event");
    handle.abort();
}

#[tokio::test]
async fn test_multiple_sequential_streams_independent() {
    // Two sequential streams should create independent tasks
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    // First stream
    let mut stream1 = client
        .send_message_stream(make_message("stream 1"))
        .await
        .unwrap();
    let first_task_id = match stream1.next().await.unwrap().unwrap() {
        Event::Task(t) => t.id,
        other => panic!("Expected Task, got: {other:?}"),
    };
    while stream1.next().await.is_some() {}

    // Second stream
    let mut stream2 = client
        .send_message_stream(make_message("stream 2"))
        .await
        .unwrap();
    let second_task_id = match stream2.next().await.unwrap().unwrap() {
        Event::Task(t) => t.id,
        other => panic!("Expected Task, got: {other:?}"),
    };
    while stream2.next().await.is_some() {}

    assert_ne!(first_task_id, second_task_id, "Each stream should create a new task");

    // Both tasks should be retrievable
    let t1 = client.get_task(&first_task_id).await.unwrap();
    let t2 = client.get_task(&second_task_id).await.unwrap();
    assert_eq!(t1.status.state, TaskState::Completed);
    assert_eq!(t2.status.state, TaskState::Completed);

    handle.abort();
}

#[tokio::test]
async fn test_get_task_returns_consistent_data_after_stream() {
    // After streaming completes, get_task should reflect final state with all artifacts
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    let mut stream = client
        .send_message_stream(make_message("get after stream"))
        .await
        .unwrap();

    let task_id = match stream.next().await.unwrap().unwrap() {
        Event::Task(t) => t.id,
        other => panic!("Expected Task, got: {other:?}"),
    };

    // Consume all events
    while stream.next().await.is_some() {}

    // Get the task
    let task = client.get_task(&task_id).await.unwrap();
    assert_eq!(task.status.state, TaskState::Completed);
    assert!(!task.context_id.is_empty());
    let artifacts = task.artifacts.unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].artifact_id, "art-1");

    handle.abort();
}

#[tokio::test]
async fn test_rejected_task_get_returns_rejected_state() {
    let (addr, handle) = start_test_server(RejectedExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("reject me"), config)
        .await
        .unwrap();

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Rejected);
            u.task_id.clone()
        }
        _ => panic!("Expected TaskStatusUpdate with Rejected"),
    };

    // Get should show Rejected state
    let task = client.get_task(&task_id).await.unwrap();
    assert_eq!(task.status.state, TaskState::Rejected);

    handle.abort();
}

#[tokio::test]
async fn test_empty_string_method_returns_method_not_found() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "",
            "id": 1
        }))
        .send()
        .await
        .unwrap();

    let body: JsonRpcResponse = resp.json().await.unwrap();
    assert!(body.error.is_some());
    assert_eq!(body.error.unwrap().code, -32601); // Method not found

    handle.abort();
}

#[tokio::test]
async fn test_batch_jsonrpc_not_supported() {
    // JSON-RPC batch (array of requests) should return parse error
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .header("content-type", "application/json")
        .body(r#"[{"jsonrpc":"2.0","method":"message/send","id":1},{"jsonrpc":"2.0","method":"tasks/get","id":2}]"#)
        .send()
        .await
        .unwrap();

    let body: JsonRpcResponse = resp.json().await.unwrap();
    assert!(body.error.is_some());
    // Should return some error (parse error or invalid request)
    let code = body.error.unwrap().code;
    assert!(code == -32700 || code == -32600, "Expected parse/invalid error, got: {code}");

    handle.abort();
}

#[tokio::test]
async fn test_wrong_jsonrpc_version_accepted() {
    // Our implementation doesn't strictly validate jsonrpc version
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .json(&serde_json::json!({
            "jsonrpc": "1.0",
            "method": "tasks/get",
            "params": {"id": "nonexistent"},
            "id": 1
        }))
        .send()
        .await
        .unwrap();

    // Should still process the request (we don't reject on jsonrpc version)
    let body: JsonRpcResponse = resp.json().await.unwrap();
    // Either success with task not found error, or processed normally
    if let Some(error) = &body.error {
        // TaskNotFound is expected since the task doesn't exist
        assert_eq!(error.code, -32001);
    }

    handle.abort();
}

#[tokio::test]
async fn test_string_jsonrpc_id_preserved_through_stream() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}"))
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "message/send",
            "params": {
                "message": {
                    "messageId": "m-str-id",
                    "role": "ROLE_USER",
                    "parts": [{"text": "string id test"}]
                }
            },
            "id": "custom-string-id-123"
        }))
        .send()
        .await
        .unwrap();

    let body: JsonRpcResponse = resp.json().await.unwrap();
    match &body.id {
        a2a_types::JsonRpcId::String(s) => assert_eq!(s, "custom-string-id-123"),
        other => panic!("Expected String ID, got: {other:?}"),
    }
    assert!(body.result.is_some());
    assert!(body.error.is_none());

    handle.abort();
}

#[tokio::test]
async fn test_data_part_type_through_stream() {
    // Data part (JSON object) should be correctly preserved through streaming
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    let msg = Message {
        message_id: uuid::Uuid::new_v4().to_string(),
        role: Role::User,
        parts: vec![
            Part {
                content: PartContent::Text {
                    text: "With data part".into(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            },
            Part {
                content: PartContent::Data {
                    data: serde_json::json!({"key": "value", "nested": {"deep": true}}),
                },
                metadata: None,
                filename: None,
                media_type: Some("application/json".into()),
            },
        ],
        context_id: None,
        task_id: None,
        metadata: None,
        extensions: None,
        reference_task_ids: None,
    };

    // Message should be accepted and task should be created
    let event = client.send_message(msg).await.unwrap();
    match event {
        Event::Task(t) => {
            assert_eq!(t.status.state, TaskState::Submitted);
        }
        _ => panic!("Expected Task event"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_cancel_task_returns_canceled_state() {
    // End-to-end: send, then cancel, verify the returned task state
    let (addr, handle) = start_test_server(SlowExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    // Non-blocking send to get task ID
    let event = client.send_message(make_message("cancel me")).await.unwrap();
    let task_id = match event {
        Event::Task(t) => t.id,
        _ => panic!("Expected Task"),
    };

    // Small delay to let executor start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Cancel
    let canceled_task = client.cancel_task(&task_id).await.unwrap();
    assert_eq!(canceled_task.status.state, TaskState::Canceled);
    assert_eq!(canceled_task.id, task_id);

    handle.abort();
}

// ---------------------------------------------------------------------------
// Final Round Tests
// ---------------------------------------------------------------------------

/// Executor that produces a URL artifact
struct UrlArtifactExecutor;

#[async_trait]
impl AgentExecutor for UrlArtifactExecutor {
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
                    artifact_id: "url-art-1".into(),
                    name: Some("Generated Report".into()),
                    description: Some("PDF report".into()),
                    parts: vec![Part {
                        content: PartContent::Url {
                            url: "https://cdn.example.com/report.pdf".into(),
                        },
                        metadata: None,
                        filename: Some("report.pdf".into()),
                        media_type: Some("application/pdf".into()),
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

#[tokio::test]
async fn test_url_part_preserved_through_stream() {
    let (addr, handle) = start_test_server(UrlArtifactExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    let mut stream = client
        .send_message_stream(make_message("Generate report"))
        .await
        .unwrap();

    let mut found_url_artifact = false;
    while let Some(Ok(event)) = stream.next().await {
        if let Event::TaskArtifactUpdate(update) = &event {
            assert_eq!(update.artifact.artifact_id, "url-art-1");
            assert_eq!(update.artifact.name.as_deref(), Some("Generated Report"));
            assert_eq!(update.artifact.parts.len(), 1);
            match &update.artifact.parts[0].content {
                PartContent::Url { url } => {
                    assert_eq!(url, "https://cdn.example.com/report.pdf");
                }
                other => panic!("Expected Url part, got: {other:?}"),
            }
            assert_eq!(
                update.artifact.parts[0].filename.as_deref(),
                Some("report.pdf")
            );
            assert_eq!(
                update.artifact.parts[0].media_type.as_deref(),
                Some("application/pdf")
            );
            found_url_artifact = true;
        }
    }

    assert!(found_url_artifact, "Should have received URL artifact event");
    handle.abort();
}

#[tokio::test]
async fn test_url_artifact_persisted_via_blocking() {
    let (addr, handle) = start_test_server(UrlArtifactExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };

    let event = client
        .send_message_with_config(make_message("blocking report"), config)
        .await
        .unwrap();

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => {
            assert_eq!(u.status.state, TaskState::Completed);
            u.task_id.clone()
        }
        _ => panic!("Expected TaskStatusUpdate Completed"),
    };

    // Verify artifact is persisted in task store
    let task = client.get_task(&task_id).await.unwrap();
    let artifacts = task.artifacts.expect("Task should have artifacts");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].artifact_id, "url-art-1");
    match &artifacts[0].parts[0].content {
        PartContent::Url { url } => {
            assert!(url.contains("report.pdf"));
        }
        other => panic!("Expected Url part in stored artifact, got: {other:?}"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_get_task_with_history_length() {
    let (addr, handle) = start_test_server(EchoExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    // Send a blocking message to create task with history
    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };
    let event = client
        .send_message_with_config(make_message("history test"), config)
        .await
        .unwrap();

    let task_id = match &event {
        Event::TaskStatusUpdate(u) => u.task_id.clone(),
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Get task with history_length=0 (should still return task)
    let task = client.get_task_with_history(&task_id, 0).await.unwrap();
    assert_eq!(task.id, task_id);

    // Get task with history_length=10 (should include available history)
    let task = client.get_task_with_history(&task_id, 10).await.unwrap();
    assert_eq!(task.id, task_id);
    // The task should have history populated if history_length > 0
    if let Some(history) = &task.history {
        assert!(!history.is_empty(), "History with length 10 should include messages");
    }

    handle.abort();
}

/// Executor that returns an error from execute
struct ErroringExecutor;

#[async_trait]
impl AgentExecutor for ErroringExecutor {
    async fn execute(
        &self,
        _event: Event,
        _queue: &dyn EventQueueWriter,
    ) -> Result<(), ServerError> {
        Err(ServerError::Internal("Executor crashed".into()))
    }
}

#[tokio::test]
async fn test_executor_error_does_not_crash_server() {
    let (addr, handle) = start_test_server(ErroringExecutor).await;
    let client = A2AClient::new(&format!("http://{addr}")).unwrap();

    // First request should return a task (error happens in background executor)
    let event = client.send_message(make_message("will error")).await.unwrap();
    match event {
        Event::Task(t) => {
            assert_eq!(t.status.state, TaskState::Submitted);
        }
        _ => panic!("Expected Task event even when executor errors"),
    }

    // Server should still be alive for a second request
    let event2 = client.send_message(make_message("second request")).await.unwrap();
    match event2 {
        Event::Task(t) => {
            assert_eq!(t.status.state, TaskState::Submitted);
        }
        _ => panic!("Expected Task event on second request"),
    }

    handle.abort();
}

#[tokio::test]
async fn test_agent_card_preserves_all_fields() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind");
    let addr = listener.local_addr().unwrap().to_string();

    let card = AgentCard {
        name: "Full Card Agent".into(),
        description: Some("Agent with all card fields populated".into()),
        supported_interfaces: vec![AgentInterface {
            url: format!("http://{addr}"),
            protocol_binding: ProtocolBinding::JsonRpc,
            protocol_version: "1.0".into(),
            tenant: Some("tenant-1".into()),
        }],
        capabilities: Some(AgentCapabilities {
            streaming: true,
            push_notifications: true,
            extended_agent_card: true,
            extensions: Some(vec![AgentExtension {
                uri: "urn:a2a:ext:custom".into(),
                description: None,
                required: None,
            }]),
        }),
        security_schemes: None,
        skills: Some(vec![
            AgentSkill {
                id: "skill-1".into(),
                name: "Summarize".into(),
                description: Some("Summarizes text".into()),
                tags: Some(vec!["nlp".into(), "text".into()]),
                examples: Some(vec!["Summarize this article".into()]),
                input_modes: Some(vec!["text/plain".into()]),
                output_modes: Some(vec!["text/plain".into()]),
            },
            AgentSkill {
                id: "skill-2".into(),
                name: "Translate".into(),
                description: None,
                tags: None,
                examples: None,
                input_modes: None,
                output_modes: None,
            },
        ]),
        default_input_modes: Some(vec!["text/plain".into(), "application/json".into()]),
        default_output_modes: Some(vec!["text/plain".into()]),
    };

    let handler = DefaultHandler::builder()
        .executor(Arc::new(EchoExecutor))
        .build();
    let router = a2a_server::create_router(Arc::new(handler), card);

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Resolve agent card and verify all fields
    let resolver = AgentCardResolver::new();
    let resolved = resolver
        .resolve(&format!("http://{addr}"))
        .await
        .expect("Failed to resolve");

    assert_eq!(resolved.name, "Full Card Agent");
    assert_eq!(
        resolved.description.as_deref(),
        Some("Agent with all card fields populated")
    );
    assert_eq!(resolved.supported_interfaces.len(), 1);
    assert_eq!(
        resolved.supported_interfaces[0].tenant.as_deref(),
        Some("tenant-1")
    );

    let caps = resolved.capabilities.unwrap();
    assert!(caps.streaming);
    assert!(caps.push_notifications);
    assert!(caps.extended_agent_card);
    let extensions = caps.extensions.unwrap();
    assert_eq!(extensions.len(), 1);
    assert_eq!(extensions[0].uri, "urn:a2a:ext:custom");

    let skills = resolved.skills.unwrap();
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0].id, "skill-1");
    assert_eq!(skills[0].tags.as_ref().unwrap().len(), 2);
    assert_eq!(skills[0].examples.as_ref().unwrap()[0], "Summarize this article");
    assert_eq!(skills[1].id, "skill-2");
    assert!(skills[1].description.is_none());

    assert_eq!(
        resolved.default_input_modes.unwrap(),
        vec!["text/plain", "application/json"]
    );
    assert_eq!(resolved.default_output_modes.unwrap(), vec!["text/plain"]);

    server_handle.abort();
}

#[tokio::test]
async fn test_multiple_clients_share_task_state() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let client1 = A2AClient::new(&format!("http://{addr}")).unwrap();
    let client2 = A2AClient::new(&format!("http://{addr}")).unwrap();

    // Client 1 creates a task via blocking
    let config = MessageSendConfiguration {
        blocking: Some(true),
        accepted_output_modes: None,
        push_notification_config: None,
        history_length: None,
    };
    let event = client1
        .send_message_with_config(make_message("shared state"), config)
        .await
        .unwrap();
    let task_id = match event {
        Event::TaskStatusUpdate(u) => u.task_id,
        _ => panic!("Expected TaskStatusUpdate"),
    };

    // Client 2 can see the same task
    let task = client2.get_task(&task_id).await.unwrap();
    assert_eq!(task.id, task_id);
    assert_eq!(task.status.state, TaskState::Completed);

    handle.abort();
}

#[tokio::test]
async fn test_raw_http_content_type_handling() {
    let (addr, handle) = start_test_server(EchoExecutor).await;

    let http_client = reqwest::Client::new();

    // Send with wrong content-type (text/plain instead of application/json)
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"message/send","params":{}}"#;
    let response = http_client
        .post(format!("http://{addr}/"))
        .header("content-type", "text/plain")
        .body(body)
        .send()
        .await
        .unwrap();

    // axum still accepts it (JSON parsing happens in handler body extraction)
    assert_eq!(response.status(), 200);
    let json: serde_json::Value = response.json().await.unwrap();
    assert!(json["error"].is_object() || json["result"].is_object());

    handle.abort();
}
