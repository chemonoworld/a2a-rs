use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use a2a_types::{
    CancelTaskRequest, Event, GetTaskRequest, JsonRpcId, JsonRpcRequest, JsonRpcResponse,
    SendMessageRequest, Task, TaskResubscriptionRequest,
};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::ClientError;
use crate::sse::parse_sse_stream;
use crate::transport::{EventStream, Transport};

/// JSON-RPC over HTTP transport for the A2A protocol.
///
/// Uses two `reqwest::Client` instances:
/// - `client`: 180-second timeout for synchronous request/response calls.
/// - `streaming_client`: no global timeout for SSE streaming responses.
pub struct JsonRpcTransport {
    client: reqwest::Client,
    streaming_client: reqwest::Client,
    url: String,
    request_id: AtomicI64,
}

impl JsonRpcTransport {
    /// Create a new transport targeting the given JSON-RPC endpoint URL.
    pub fn new(url: &str) -> Result<Self, ClientError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()?;

        let streaming_client = reqwest::Client::builder().build()?;

        Ok(Self {
            client,
            streaming_client,
            url: url.to_string(),
            request_id: AtomicI64::new(1),
        })
    }

    fn next_id(&self) -> i64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Send a synchronous JSON-RPC request and deserialize the result.
    async fn send_jsonrpc<T: DeserializeOwned>(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> Result<T, ClientError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: Some(serde_json::to_value(params)?),
            id: JsonRpcId::Number(self.next_id()),
        };

        let response = self.client.post(&self.url).json(&request).send().await?;
        let jsonrpc_resp: JsonRpcResponse = response.json().await?;

        if let Some(error) = jsonrpc_resp.error {
            return Err(ClientError::JsonRpc(error));
        }

        let result = jsonrpc_resp.result.ok_or(ClientError::EmptyResult)?;
        serde_json::from_value(result).map_err(Into::into)
    }

    /// Send a JSON-RPC request and return an SSE event stream.
    async fn send_jsonrpc_stream(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> Result<EventStream, ClientError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: Some(serde_json::to_value(params)?),
            id: JsonRpcId::Number(self.next_id()),
        };

        let response = self
            .streaming_client
            .post(&self.url)
            .json(&request)
            .send()
            .await?;

        Ok(parse_sse_stream(response.bytes_stream()))
    }
}

#[async_trait::async_trait]
impl Transport for JsonRpcTransport {
    async fn send_message(&self, request: SendMessageRequest) -> Result<Event, ClientError> {
        self.send_jsonrpc("message/send", request).await
    }

    async fn send_message_stream(
        &self,
        request: SendMessageRequest,
    ) -> Result<EventStream, ClientError> {
        self.send_jsonrpc_stream("message/stream", request).await
    }

    async fn get_task(&self, request: GetTaskRequest) -> Result<Task, ClientError> {
        self.send_jsonrpc("tasks/get", request).await
    }

    async fn cancel_task(&self, request: CancelTaskRequest) -> Result<Task, ClientError> {
        self.send_jsonrpc("tasks/cancel", request).await
    }

    async fn resubscribe(
        &self,
        request: TaskResubscriptionRequest,
    ) -> Result<EventStream, ClientError> {
        self.send_jsonrpc_stream("tasks/resubscribe", request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_new_creates_successfully() {
        let transport = JsonRpcTransport::new("http://localhost:3000");
        assert!(transport.is_ok());
    }

    #[test]
    fn test_transport_id_starts_at_one() {
        let transport = JsonRpcTransport::new("http://localhost:3000").unwrap();
        assert_eq!(transport.next_id(), 1);
    }

    #[test]
    fn test_transport_id_increments() {
        let transport = JsonRpcTransport::new("http://localhost:3000").unwrap();
        let id1 = transport.next_id();
        let id2 = transport.next_id();
        let id3 = transport.next_id();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[test]
    fn test_transport_stores_url() {
        let transport = JsonRpcTransport::new("http://example.com:8080/rpc").unwrap();
        assert_eq!(transport.url, "http://example.com:8080/rpc");
    }

    #[test]
    fn test_transport_next_id_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let transport = Arc::new(JsonRpcTransport::new("http://localhost:3000").unwrap());
        let mut handles = vec![];

        for _ in 0..10 {
            let t = transport.clone();
            handles.push(thread::spawn(move || t.next_id()));
        }

        let mut ids: Vec<i64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        ids.sort();
        ids.dedup();
        // All 10 IDs should be unique
        assert_eq!(ids.len(), 10, "All concurrent IDs should be unique");
        // IDs should be in range 1..=10
        assert_eq!(*ids.first().unwrap(), 1);
        assert_eq!(*ids.last().unwrap(), 10);
    }

    #[test]
    fn test_transport_url_with_query_params() {
        let url = "http://example.com:8080/rpc?key=value&token=abc";
        let transport = JsonRpcTransport::new(url).unwrap();
        assert_eq!(transport.url, url);
    }

    #[test]
    fn test_transport_url_various_formats() {
        // Trailing slash
        let t1 = JsonRpcTransport::new("http://localhost:3000/").unwrap();
        assert_eq!(t1.url, "http://localhost:3000/");

        // HTTPS
        let t2 = JsonRpcTransport::new("https://agent.example.com/v1/rpc").unwrap();
        assert_eq!(t2.url, "https://agent.example.com/v1/rpc");

        // With fragment
        let t3 = JsonRpcTransport::new("http://localhost:3000/rpc#section").unwrap();
        assert_eq!(t3.url, "http://localhost:3000/rpc#section");
    }
}
