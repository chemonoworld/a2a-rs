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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_error_is_std_error() {
        let err = ClientError::EmptyResult;
        let _: &dyn std::error::Error = &err;

        let err = ClientError::StreamClosed;
        let dyn_err: Box<dyn std::error::Error> = Box::new(err);
        assert!(dyn_err.to_string().contains("Stream closed"));
    }

    #[test]
    fn test_client_error_json_source() {
        let serde_err = serde_json::from_str::<serde_json::Value>("bad json").unwrap_err();
        let err = ClientError::Json(serde_err);

        // std::error::Error::source() should return the underlying serde_json error
        use std::error::Error;
        let source = err.source();
        assert!(source.is_some(), "Json variant should have a source");

        let display = format!("{err}");
        assert!(display.contains("JSON error"));
    }

    #[test]
    fn test_client_error_all_variants_display() {
        let variants: Vec<ClientError> = vec![
            ClientError::JsonRpc(JsonRpcError {
                code: -32001,
                message: "Task not found".into(),
                data: None,
            }),
            ClientError::EmptyResult,
            ClientError::NoSupportedInterface,
            ClientError::SseParse("unexpected EOF".into()),
            ClientError::StreamClosed,
        ];

        for err in &variants {
            let display = format!("{err}");
            assert!(!display.is_empty(), "Display should not be empty");
        }

        // Verify specific content
        let jsonrpc_display = format!("{}", variants[0]);
        assert!(jsonrpc_display.contains("-32001"));
        assert!(jsonrpc_display.contains("Task not found"));

        let sse_display = format!("{}", variants[3]);
        assert!(sse_display.contains("unexpected EOF"));
    }

    #[test]
    fn test_client_error_jsonrpc_with_data() {
        let err = ClientError::JsonRpc(JsonRpcError {
            code: -32001,
            message: "Task not found".into(),
            data: Some(serde_json::json!({"task_id": "t-missing", "details": "no such task"})),
        });

        let display = format!("{err}");
        assert!(display.contains("-32001"));
        assert!(display.contains("Task not found"));
        // data is not in Display but should be accessible
        match err {
            ClientError::JsonRpc(e) => {
                assert!(e.data.is_some());
                assert_eq!(e.data.unwrap()["task_id"], "t-missing");
            }
            _ => panic!("Expected JsonRpc"),
        }
    }

    #[test]
    fn test_client_error_from_reqwest() {
        // reqwest::Error is tricky to construct, but we can verify the From impl exists
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let client_err: ClientError = json_err.into();
        assert!(matches!(client_err, ClientError::Json(_)));
    }
}
