use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use a2a_types::{
    CancelTaskRequest, GetTaskRequest, JsonRpcError, JsonRpcId, JsonRpcRequest, JsonRpcResponse,
    SendMessageRequest, TaskResubscriptionRequest, A2AErrorCode,
};

use crate::error::ServerError;
use crate::router::AppState;
use crate::sse_writer::event_stream_to_sse;

/// Main JSON-RPC handler for A2A protocol requests.
pub async fn jsonrpc_handler(
    State(state): State<AppState>,
    body: String,
) -> Response {
    // Parse JSON-RPC request
    let request: JsonRpcRequest = match serde_json::from_str(&body) {
        Ok(req) => req,
        Err(_) => {
            let resp = JsonRpcResponse::error(
                JsonRpcId::Null,
                JsonRpcError {
                    code: A2AErrorCode::ParseError.code(),
                    message: A2AErrorCode::ParseError.default_message().into(),
                    data: None,
                },
            );
            return Json(resp).into_response();
        }
    };

    let request_id = request.id.clone();
    let params = request.params.clone().unwrap_or(serde_json::Value::Null);

    // Run interceptor before hook
    if let Some(ref interceptor) = state.interceptor {
        if let Err(e) = interceptor.before(&request.method, &params).await {
            let resp = JsonRpcResponse::error(request_id, (&e).into());
            return Json(resp).into_response();
        }
    }

    let response = match request.method.as_str() {
        "message/send" => handle_send_message(&state, request_id.clone(), &params).await,
        "message/stream" => {
            return handle_stream(&state, request_id, &params).await;
        }
        "tasks/get" => handle_get_task(&state, request_id.clone(), &params).await,
        "tasks/cancel" => handle_cancel_task(&state, request_id.clone(), &params).await,
        "tasks/resubscribe" => {
            return handle_resubscribe(&state, request_id, &params).await;
        }
        _ => {
            let resp = JsonRpcResponse::error(
                request_id.clone(),
                JsonRpcError {
                    code: A2AErrorCode::MethodNotFound.code(),
                    message: A2AErrorCode::MethodNotFound.default_message().into(),
                    data: None,
                },
            );
            Err(resp)
        }
    };

    let json_resp = match response {
        Ok(resp) => resp,
        Err(resp) => resp,
    };

    // Run interceptor after hook
    if let Some(ref interceptor) = state.interceptor {
        let result_value = json_resp
            .result
            .clone()
            .map(Ok)
            .unwrap_or_else(|| Err(ServerError::Internal("error response".into())));
        let _ = interceptor
            .after(&request.method, &result_value)
            .await;
    }

    Json(json_resp).into_response()
}

async fn handle_send_message(
    state: &AppState,
    request_id: JsonRpcId,
    params: &serde_json::Value,
) -> Result<JsonRpcResponse, JsonRpcResponse> {
    let send_req: SendMessageRequest = serde_json::from_value(params.clone()).map_err(|_| {
        JsonRpcResponse::error(
            request_id.clone(),
            JsonRpcError {
                code: A2AErrorCode::InvalidParams.code(),
                message: A2AErrorCode::InvalidParams.default_message().into(),
                data: None,
            },
        )
    })?;

    match state.handler.on_send_message(send_req).await {
        Ok(event) => {
            let result = serde_json::to_value(&event).unwrap_or_default();
            Ok(JsonRpcResponse::success(request_id, result))
        }
        Err(e) => Err(JsonRpcResponse::error(request_id, (&e).into())),
    }
}

async fn handle_stream(
    state: &AppState,
    request_id: JsonRpcId,
    params: &serde_json::Value,
) -> Response {
    let send_req: SendMessageRequest = match serde_json::from_value(params.clone()) {
        Ok(req) => req,
        Err(_) => {
            let resp = JsonRpcResponse::error(
                request_id,
                JsonRpcError {
                    code: A2AErrorCode::InvalidParams.code(),
                    message: A2AErrorCode::InvalidParams.default_message().into(),
                    data: None,
                },
            );
            return Json(resp).into_response();
        }
    };

    match state.handler.on_send_message_stream(send_req).await {
        Ok(stream) => event_stream_to_sse(stream, request_id).into_response(),
        Err(e) => {
            let resp = JsonRpcResponse::error(request_id, (&e).into());
            Json(resp).into_response()
        }
    }
}

async fn handle_get_task(
    state: &AppState,
    request_id: JsonRpcId,
    params: &serde_json::Value,
) -> Result<JsonRpcResponse, JsonRpcResponse> {
    let get_req: GetTaskRequest = serde_json::from_value(params.clone()).map_err(|_| {
        JsonRpcResponse::error(
            request_id.clone(),
            JsonRpcError {
                code: A2AErrorCode::InvalidParams.code(),
                message: A2AErrorCode::InvalidParams.default_message().into(),
                data: None,
            },
        )
    })?;

    match state.handler.on_get_task(get_req).await {
        Ok(task) => {
            let result = serde_json::to_value(&task).unwrap_or_default();
            Ok(JsonRpcResponse::success(request_id, result))
        }
        Err(e) => Err(JsonRpcResponse::error(request_id, (&e).into())),
    }
}

async fn handle_cancel_task(
    state: &AppState,
    request_id: JsonRpcId,
    params: &serde_json::Value,
) -> Result<JsonRpcResponse, JsonRpcResponse> {
    let cancel_req: CancelTaskRequest =
        serde_json::from_value(params.clone()).map_err(|_| {
            JsonRpcResponse::error(
                request_id.clone(),
                JsonRpcError {
                    code: A2AErrorCode::InvalidParams.code(),
                    message: A2AErrorCode::InvalidParams.default_message().into(),
                    data: None,
                },
            )
        })?;

    match state.handler.on_cancel_task(cancel_req).await {
        Ok(task) => {
            let result = serde_json::to_value(&task).unwrap_or_default();
            Ok(JsonRpcResponse::success(request_id, result))
        }
        Err(e) => Err(JsonRpcResponse::error(request_id, (&e).into())),
    }
}

async fn handle_resubscribe(
    state: &AppState,
    request_id: JsonRpcId,
    params: &serde_json::Value,
) -> Response {
    let resub_req: TaskResubscriptionRequest = match serde_json::from_value(params.clone()) {
        Ok(req) => req,
        Err(_) => {
            let resp = JsonRpcResponse::error(
                request_id,
                JsonRpcError {
                    code: A2AErrorCode::InvalidParams.code(),
                    message: A2AErrorCode::InvalidParams.default_message().into(),
                    data: None,
                },
            );
            return (StatusCode::BAD_REQUEST, Json(resp)).into_response();
        }
    };

    match state.handler.on_resubscribe(resub_req).await {
        Ok(stream) => event_stream_to_sse(stream, request_id).into_response(),
        Err(e) => {
            let resp = JsonRpcResponse::error(request_id, (&e).into());
            Json(resp).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_executor::AgentExecutor;
    use crate::event_queue::EventQueueWriter;
    use crate::handler::DefaultHandler;
    use crate::middleware::CallInterceptor;
    use crate::router::create_router_with_interceptor;
    use a2a_types::{
        AgentCard, Event, Message, Part, PartContent, Role, SendMessageRequest, TaskState,
        TaskStatus, TaskStatusUpdateEvent,
    };
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tower::ServiceExt;

    struct EchoExecutor;

    #[async_trait::async_trait]
    impl AgentExecutor for EchoExecutor {
        async fn execute(
            &self,
            event: Event,
            queue: &dyn EventQueueWriter,
        ) -> Result<(), ServerError> {
            let (task_id, context_id) = match &event {
                Event::Task(t) => (t.id.clone(), t.context_id.clone()),
                _ => return Ok(()),
            };

            let completed = Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id,
                context_id,
                status: TaskStatus {
                    state: TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            });
            queue.write(completed).await?;
            Ok(())
        }
    }

    fn make_agent_card() -> AgentCard {
        AgentCard {
            name: "Test".into(),
            description: None,
            supported_interfaces: vec![],
            capabilities: None,
            security_schemes: None,
            skills: None,
            default_input_modes: None,
            default_output_modes: None,
        }
    }

    fn make_router_app(
        interceptor: Option<Arc<dyn CallInterceptor>>,
    ) -> axum::Router {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();
        create_router_with_interceptor(Arc::new(handler), make_agent_card(), interceptor)
    }

    fn make_jsonrpc_body(method: &str, params: serde_json::Value) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        })
        .to_string()
    }

    fn make_send_params() -> serde_json::Value {
        let req = SendMessageRequest {
            message: Message {
                message_id: "m-1".into(),
                role: Role::User,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "Hello".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                }],
                context_id: Some("ctx-1".into()),
                task_id: None,
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            },
            configuration: None,
        };
        serde_json::to_value(req).unwrap()
    }

    async fn post_jsonrpc(
        app: axum::Router,
        body: &str,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let req = axum::http::Request::builder()
            .uri("/")
            .method("POST")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        (status, json)
    }

    #[tokio::test]
    async fn test_parse_error_returns_json_rpc_error() {
        let app = make_router_app(None);
        let (status, json) = post_jsonrpc(app, "not valid json{{{").await;

        assert_eq!(status, axum::http::StatusCode::OK);
        assert!(json["error"].is_object());
        assert_eq!(json["error"]["code"], A2AErrorCode::ParseError.code());
        assert_eq!(json["id"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn test_method_not_found() {
        let app = make_router_app(None);
        let body = make_jsonrpc_body("nonexistent/method", serde_json::json!({}));
        let (_, json) = post_jsonrpc(app, &body).await;

        assert!(json["error"].is_object());
        assert_eq!(json["error"]["code"], A2AErrorCode::MethodNotFound.code());
        assert_eq!(json["id"], 1);
    }

    #[tokio::test]
    async fn test_send_message_invalid_params() {
        let app = make_router_app(None);
        // Missing required "message" field
        let body = make_jsonrpc_body("message/send", serde_json::json!({"wrong": "field"}));
        let (_, json) = post_jsonrpc(app, &body).await;

        assert!(json["error"].is_object());
        assert_eq!(json["error"]["code"], A2AErrorCode::InvalidParams.code());
    }

    #[tokio::test]
    async fn test_send_message_success() {
        let app = make_router_app(None);
        let body = make_jsonrpc_body("message/send", make_send_params());
        let (status, json) = post_jsonrpc(app, &body).await;

        assert_eq!(status, axum::http::StatusCode::OK);
        assert!(
            json["result"].is_object(),
            "Expected result object, got: {json}"
        );
        assert_eq!(json["id"], 1);
    }

    #[tokio::test]
    async fn test_tasks_get_not_found() {
        let app = make_router_app(None);
        let body = make_jsonrpc_body("tasks/get", serde_json::json!({"id": "nonexistent"}));
        let (_, json) = post_jsonrpc(app, &body).await;

        assert!(json["error"].is_object());
        assert_eq!(json["error"]["code"], A2AErrorCode::TaskNotFound.code());
    }

    #[tokio::test]
    async fn test_tasks_cancel_not_found() {
        let app = make_router_app(None);
        let body = make_jsonrpc_body("tasks/cancel", serde_json::json!({"id": "nonexistent"}));
        let (_, json) = post_jsonrpc(app, &body).await;

        assert!(json["error"].is_object());
        assert_eq!(json["error"]["code"], A2AErrorCode::TaskNotFound.code());
    }

    #[tokio::test]
    async fn test_tasks_get_invalid_params() {
        let app = make_router_app(None);
        let body = make_jsonrpc_body("tasks/get", serde_json::json!({"wrong": "field"}));
        let (_, json) = post_jsonrpc(app, &body).await;

        assert!(json["error"].is_object());
        assert_eq!(json["error"]["code"], A2AErrorCode::InvalidParams.code());
    }

    #[tokio::test]
    async fn test_tasks_cancel_invalid_params() {
        let app = make_router_app(None);
        let body = make_jsonrpc_body("tasks/cancel", serde_json::json!(42));
        let (_, json) = post_jsonrpc(app, &body).await;

        assert!(json["error"].is_object());
        assert_eq!(json["error"]["code"], A2AErrorCode::InvalidParams.code());
    }

    #[tokio::test]
    async fn test_interceptor_before_rejects_request() {
        struct RejectingInterceptor;

        #[async_trait::async_trait]
        impl CallInterceptor for RejectingInterceptor {
            async fn before(
                &self,
                _method: &str,
                _params: &serde_json::Value,
            ) -> Result<(), ServerError> {
                Err(ServerError::Internal("Access denied".into()))
            }

            async fn after(
                &self,
                _method: &str,
                _result: &Result<serde_json::Value, ServerError>,
            ) -> Result<(), ServerError> {
                Ok(())
            }
        }

        let app = make_router_app(Some(Arc::new(RejectingInterceptor)));
        let body = make_jsonrpc_body("message/send", make_send_params());
        let (_, json) = post_jsonrpc(app, &body).await;

        assert!(json["error"].is_object());
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Access denied"));
    }

    #[tokio::test]
    async fn test_null_params_treated_as_null() {
        let app = make_router_app(None);
        // Explicit null params should be deserialized into Null
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "message/send"
        })
        .to_string();

        let (_, json) = post_jsonrpc(app, &body).await;

        // Missing params → null → will fail to parse as SendMessageRequest → InvalidParams
        assert!(json["error"].is_object());
        assert_eq!(json["error"]["code"], A2AErrorCode::InvalidParams.code());
    }

    #[tokio::test]
    async fn test_string_id_preserved_in_response() {
        let app = make_router_app(None);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "my-string-id",
            "method": "message/send",
            "params": make_send_params()
        })
        .to_string();

        let (_, json) = post_jsonrpc(app, &body).await;
        assert_eq!(json["id"], "my-string-id");
    }

    #[tokio::test]
    async fn test_message_stream_returns_sse_response() {
        let app = make_router_app(None);
        let body = make_jsonrpc_body("message/stream", make_send_params());

        let req = axum::http::Request::builder()
            .uri("/")
            .method("POST")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        assert_eq!(status, axum::http::StatusCode::OK);

        // SSE response should have text/event-stream content type
        let content_type = resp
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        assert!(
            content_type.contains("text/event-stream"),
            "Expected text/event-stream, got: {content_type}. Headers: {:?}",
            resp.headers()
        );
    }

    #[tokio::test]
    async fn test_message_stream_invalid_params() {
        let app = make_router_app(None);
        let body = make_jsonrpc_body("message/stream", serde_json::json!({"bad": true}));
        let (_, json) = post_jsonrpc(app, &body).await;

        assert!(json["error"].is_object());
        assert_eq!(json["error"]["code"], A2AErrorCode::InvalidParams.code());
    }
}
