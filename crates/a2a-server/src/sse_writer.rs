use std::convert::Infallible;

use a2a_types::{JsonRpcId, JsonRpcResponse};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use futures_core::Stream;
use tokio_stream::StreamExt;

use crate::event_queue::EventStream;

/// Convert an `EventStream` into an axum SSE response.
///
/// Each A2A event is wrapped in a `JsonRpcResponse` and serialized as SSE data.
pub fn event_stream_to_sse(
    stream: EventStream,
    request_id: JsonRpcId,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let sse_stream = stream.map(move |result| {
        let sse_event = match result {
            Ok(event) => {
                let result_value = serde_json::to_value(&event).unwrap_or_default();
                let response = JsonRpcResponse::success(request_id.clone(), result_value);
                let data = serde_json::to_string(&response).unwrap_or_default();
                let id = uuid::Uuid::new_v4().to_string();
                SseEvent::default().id(id).data(data)
            }
            Err(err) => {
                let rpc_error: a2a_types::JsonRpcError = (&err).into();
                let response = JsonRpcResponse::error(request_id.clone(), rpc_error);
                let data = serde_json::to_string(&response).unwrap_or_default();
                SseEvent::default().data(data)
            }
        };
        Ok(sse_event)
    });

    Sse::new(sse_stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2a_types::{
        Event, Task, TaskState, TaskStatus, TaskStatusUpdateEvent,
    };
    use crate::error::ServerError;
    use axum::response::IntoResponse;

    fn make_task_event(id: &str) -> Event {
        Event::Task(Task {
            id: id.into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Submitted,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        })
    }

    #[tokio::test]
    async fn test_sse_response_content_type() {
        let stream: EventStream = Box::pin(tokio_stream::once(Ok(make_task_event("t-1"))));

        let sse = event_stream_to_sse(stream, JsonRpcId::Number(1));
        let response = sse.into_response();

        let content_type = response.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(
            content_type.contains("text/event-stream"),
            "Content-Type should be text/event-stream, got: {content_type}"
        );
    }

    #[tokio::test]
    async fn test_sse_stream_mapping_success() {
        // Test the mapping logic directly by reproducing it
        let event = make_task_event("t-1");
        let request_id = JsonRpcId::Number(42);

        let result_value = serde_json::to_value(&event).unwrap();
        let response = JsonRpcResponse::success(request_id, result_value);
        let data = serde_json::to_string(&response).unwrap();

        // Verify the response contains expected fields
        assert!(data.contains("\"jsonrpc\":\"2.0\""));
        assert!(data.contains("\"result\":"));
        assert!(data.contains("t-1"));
        assert!(!data.contains("\"error\":"));
    }

    #[tokio::test]
    async fn test_sse_stream_mapping_error() {
        let err = ServerError::TaskNotFound("t-missing".into());
        let request_id = JsonRpcId::Number(1);

        let rpc_error: a2a_types::JsonRpcError = (&err).into();
        let response = JsonRpcResponse::error(request_id, rpc_error);
        let data = serde_json::to_string(&response).unwrap();

        assert!(data.contains("\"error\":"));
        assert!(data.contains("-32001")); // TaskNotFound code
        assert!(data.contains("t-missing"));
        assert!(!data.contains("\"result\":"));
    }

    #[tokio::test]
    async fn test_sse_stream_mapping_preserves_request_id() {
        let event = make_task_event("t-1");

        // Test with number ID
        let result_value = serde_json::to_value(&event).unwrap();
        let response = JsonRpcResponse::success(JsonRpcId::Number(99), result_value);
        let data = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed["id"], 99);

        // Test with string ID
        let result_value = serde_json::to_value(&event).unwrap();
        let response = JsonRpcResponse::success(JsonRpcId::String("req-abc".into()), result_value);
        let data = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed["id"], "req-abc");
    }

    #[tokio::test]
    async fn test_sse_event_stream_produces_correct_items() {
        use tokio_stream::StreamExt;

        let events: Vec<Result<Event, ServerError>> = vec![
            Ok(make_task_event("t-1")),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t-1".into(),
                context_id: "ctx-1".into(),
                status: TaskStatus {
                    state: TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            })),
        ];

        let stream: EventStream = Box::pin(tokio_stream::iter(events));
        let request_id = JsonRpcId::Number(1);

        // Map events through the same logic as event_stream_to_sse
        let mapped = stream.map(move |result| {
            match result {
                Ok(event) => {
                    let result_value = serde_json::to_value(&event).unwrap_or_default();
                    let response = JsonRpcResponse::success(request_id.clone(), result_value);
                    serde_json::to_string(&response).unwrap()
                }
                Err(err) => {
                    let rpc_error: a2a_types::JsonRpcError = (&err).into();
                    let response = JsonRpcResponse::error(request_id.clone(), rpc_error);
                    serde_json::to_string(&response).unwrap()
                }
            }
        });

        let items: Vec<String> = mapped.collect().await;
        assert_eq!(items.len(), 2);

        // First item: Task event
        let first: serde_json::Value = serde_json::from_str(&items[0]).unwrap();
        assert!(first["result"]["id"].is_string());

        // Second item: Status update
        let second: serde_json::Value = serde_json::from_str(&items[1]).unwrap();
        assert!(second["result"]["status"]["state"].is_string());
    }

    #[tokio::test]
    async fn test_sse_stream_with_mixed_ok_and_error() {
        use tokio_stream::StreamExt;

        let events: Vec<Result<Event, ServerError>> = vec![
            Ok(make_task_event("t-ok")),
            Err(ServerError::TaskNotFound("t-missing".into())),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t-ok".into(),
                context_id: "ctx-1".into(),
                status: TaskStatus {
                    state: TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            })),
        ];

        let stream: EventStream = Box::pin(tokio_stream::iter(events));
        let request_id = JsonRpcId::Number(5);

        let mapped = stream.map(move |result| match result {
            Ok(event) => {
                let result_value = serde_json::to_value(&event).unwrap_or_default();
                let response = JsonRpcResponse::success(request_id.clone(), result_value);
                serde_json::to_string(&response).unwrap()
            }
            Err(err) => {
                let rpc_error: a2a_types::JsonRpcError = (&err).into();
                let response = JsonRpcResponse::error(request_id.clone(), rpc_error);
                serde_json::to_string(&response).unwrap()
            }
        });

        let items: Vec<String> = mapped.collect().await;
        assert_eq!(items.len(), 3);

        // First: success
        let first: serde_json::Value = serde_json::from_str(&items[0]).unwrap();
        assert!(first["result"].is_object());
        assert!(first["error"].is_null());

        // Second: error
        let second: serde_json::Value = serde_json::from_str(&items[1]).unwrap();
        assert!(second["error"].is_object());
        assert_eq!(second["error"]["code"], -32001);
        assert!(second["result"].is_null());

        // Third: success again
        let third: serde_json::Value = serde_json::from_str(&items[2]).unwrap();
        assert!(third["result"].is_object());
    }

    #[tokio::test]
    async fn test_sse_stream_with_artifact_update_event() {
        let event = Event::TaskArtifactUpdate(a2a_types::TaskArtifactUpdateEvent {
            task_id: "t-art".into(),
            context_id: "ctx-art".into(),
            artifact: a2a_types::Artifact {
                artifact_id: "a-1".into(),
                name: Some("Result".into()),
                description: None,
                parts: vec![a2a_types::Part {
                    content: a2a_types::PartContent::Text {
                        text: "artifact data".into(),
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
        });

        let request_id = JsonRpcId::Number(7);
        let result_value = serde_json::to_value(&event).unwrap();
        let response = JsonRpcResponse::success(request_id, result_value);
        let data = serde_json::to_string(&response).unwrap();

        assert!(data.contains("\"jsonrpc\":\"2.0\""));
        assert!(data.contains("t-art"));
        assert!(data.contains("a-1"));
        assert!(data.contains("artifact data"));
        assert!(!data.contains("\"error\":"));
    }

    #[tokio::test]
    async fn test_sse_stream_with_null_jsonrpc_id() {
        let event = make_task_event("t-null-id");
        let request_id = JsonRpcId::Null;

        let result_value = serde_json::to_value(&event).unwrap();
        let response = JsonRpcResponse::success(request_id, result_value);
        let data = serde_json::to_string(&response).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert!(parsed["id"].is_null(), "Null ID should serialize as null");
        assert!(parsed["result"].is_object());
        assert!(data.contains("t-null-id"));
    }

    #[tokio::test]
    async fn test_sse_stream_error_task_not_cancelable() {
        let err = ServerError::TaskNotCancelable("t-done".into());
        let request_id = JsonRpcId::String("req-cancel".into());

        let rpc_error: a2a_types::JsonRpcError = (&err).into();
        let response = JsonRpcResponse::error(request_id, rpc_error);
        let data = serde_json::to_string(&response).unwrap();

        assert!(data.contains("\"error\":"));
        assert!(data.contains("-32002")); // TaskNotCancelable code
        assert!(data.contains("t-done"));
        assert!(data.contains("req-cancel"));
        assert!(!data.contains("\"result\":"));
    }

    #[tokio::test]
    async fn test_sse_empty_stream() {
        use tokio_stream::StreamExt;

        let events: Vec<Result<Event, ServerError>> = vec![];
        let stream: EventStream = Box::pin(tokio_stream::iter(events));
        let request_id = JsonRpcId::Number(1);

        let mapped = stream.map(move |result| match result {
            Ok(event) => {
                let result_value = serde_json::to_value(&event).unwrap_or_default();
                serde_json::to_string(&JsonRpcResponse::success(request_id.clone(), result_value))
                    .unwrap()
            }
            Err(err) => {
                let rpc_error: a2a_types::JsonRpcError = (&err).into();
                serde_json::to_string(&JsonRpcResponse::error(request_id.clone(), rpc_error))
                    .unwrap()
            }
        });

        let items: Vec<String> = mapped.collect().await;
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_sse_stream_error_internal() {
        let err = ServerError::Internal("database timeout".into());
        let request_id = JsonRpcId::Number(10);

        let rpc_error: a2a_types::JsonRpcError = (&err).into();
        let response = JsonRpcResponse::error(request_id, rpc_error);
        let data = serde_json::to_string(&response).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed["error"]["code"], a2a_types::A2AErrorCode::InternalError.code());
        assert!(parsed["error"]["message"].as_str().unwrap().contains("database timeout"));
    }

    #[tokio::test]
    async fn test_sse_stream_error_queue_closed() {
        let err = ServerError::QueueClosed;
        let request_id = JsonRpcId::Number(11);

        let rpc_error: a2a_types::JsonRpcError = (&err).into();
        let response = JsonRpcResponse::error(request_id, rpc_error);
        let data = serde_json::to_string(&response).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed["error"]["code"], a2a_types::A2AErrorCode::InternalError.code());
        assert!(parsed["error"]["message"].as_str().unwrap().contains("Queue closed"));
    }

    #[tokio::test]
    async fn test_sse_stream_error_a2a_with_data() {
        let a2a_err = a2a_types::A2AError::new(a2a_types::A2AErrorCode::TaskNotFound, "task missing")
            .with_data(serde_json::json!({"hint": "check task ID"}));
        let err = ServerError::A2A(a2a_err);
        let request_id = JsonRpcId::String("req-err".into());

        let rpc_error: a2a_types::JsonRpcError = (&err).into();
        let response = JsonRpcResponse::error(request_id, rpc_error);
        let data = serde_json::to_string(&response).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed["error"]["code"], a2a_types::A2AErrorCode::TaskNotFound.code());
        assert_eq!(parsed["error"]["data"]["hint"], "check task ID");
        assert_eq!(parsed["id"], "req-err");
    }

    #[tokio::test]
    async fn test_sse_stream_with_large_numeric_id() {
        let event = make_task_event("t-bigid");
        let request_id = JsonRpcId::Number(i64::MAX);

        let result_value = serde_json::to_value(&event).unwrap();
        let response = JsonRpcResponse::success(request_id, result_value);
        let data = serde_json::to_string(&response).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(parsed["id"], i64::MAX);
        assert!(parsed["result"].is_object());
    }
}
