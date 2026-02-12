use a2a_types::JsonRpcError;

/// Client-side errors for the A2A SDK
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("JSON-RPC error: code={}, message={}", .0.code, .0.message)]
    JsonRpc(JsonRpcError),

    #[error("Empty result in JSON-RPC response")]
    EmptyResult,

    #[error("No supported interface found in AgentCard")]
    NoSupportedInterface,

    #[error("SSE parse error: {0}")]
    SseParse(String),

    #[error("Stream closed unexpectedly")]
    StreamClosed,
}
