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
