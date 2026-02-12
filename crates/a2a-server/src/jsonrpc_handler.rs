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
