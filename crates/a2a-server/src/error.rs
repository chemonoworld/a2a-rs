use a2a_types::{A2AError, A2AErrorCode, JsonRpcError};

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("A2A error: {0}")]
    A2A(A2AError),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Task not cancelable: {0}")]
    TaskNotCancelable(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Queue closed")]
    QueueClosed,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<A2AError> for ServerError {
    fn from(err: A2AError) -> Self {
        ServerError::A2A(err)
    }
}

impl From<&ServerError> for A2AError {
    fn from(err: &ServerError) -> Self {
        match err {
            ServerError::A2A(e) => e.clone(),
            ServerError::TaskNotFound(id) => {
                A2AError::new(A2AErrorCode::TaskNotFound, format!("Task not found: {id}"))
            }
            ServerError::TaskNotCancelable(id) => A2AError::new(
                A2AErrorCode::TaskNotCancelable,
                format!("Task not cancelable: {id}"),
            ),
            ServerError::Internal(msg) => {
                A2AError::new(A2AErrorCode::InternalError, msg.clone())
            }
            ServerError::Serialization(e) => {
                A2AError::new(A2AErrorCode::InternalError, e.to_string())
            }
            ServerError::QueueClosed => {
                A2AError::new(A2AErrorCode::InternalError, "Queue closed")
            }
            ServerError::Io(e) => {
                A2AError::new(A2AErrorCode::InternalError, e.to_string())
            }
        }
    }
}

impl From<&ServerError> for JsonRpcError {
    fn from(err: &ServerError) -> Self {
        let a2a: A2AError = err.into();
        JsonRpcError {
            code: a2a.code,
            message: a2a.message,
            data: a2a.data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_error_to_a2a_error() {
        let err = ServerError::TaskNotFound("task-123".into());
        let a2a: A2AError = (&err).into();
        assert_eq!(a2a.code, A2AErrorCode::TaskNotFound.code());
        assert!(a2a.message.contains("task-123"));
    }

    #[test]
    fn test_server_error_to_jsonrpc_error() {
        let err = ServerError::Internal("something broke".into());
        let rpc: JsonRpcError = (&err).into();
        assert_eq!(rpc.code, A2AErrorCode::InternalError.code());
        assert!(rpc.message.contains("something broke"));
    }

    #[test]
    fn test_server_error_task_not_cancelable() {
        let err = ServerError::TaskNotCancelable("task-42".into());
        let a2a: A2AError = (&err).into();
        assert_eq!(a2a.code, A2AErrorCode::TaskNotCancelable.code());
        assert!(a2a.message.contains("task-42"));
    }

    #[test]
    fn test_server_error_queue_closed() {
        let err = ServerError::QueueClosed;
        let a2a: A2AError = (&err).into();
        assert_eq!(a2a.code, A2AErrorCode::InternalError.code());
        assert!(a2a.message.contains("Queue closed"));
    }

    #[test]
    fn test_server_error_serialization() {
        let serde_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = ServerError::Serialization(serde_err);
        let a2a: A2AError = (&err).into();
        assert_eq!(a2a.code, A2AErrorCode::InternalError.code());
    }

    #[test]
    fn test_server_error_from_a2a_error() {
        let a2a = A2AError::new(A2AErrorCode::UnsupportedOperation, "nope");
        let server_err = ServerError::from(a2a.clone());
        match &server_err {
            ServerError::A2A(inner) => {
                assert_eq!(inner.code, a2a.code);
                assert_eq!(inner.message, a2a.message);
            }
            _ => panic!("Expected A2A variant"),
        }
        // Round-trip back
        let roundtripped: A2AError = (&server_err).into();
        assert_eq!(roundtripped.code, A2AErrorCode::UnsupportedOperation.code());
    }

    #[test]
    fn test_server_error_display() {
        let err = ServerError::TaskNotFound("t-99".into());
        let display = format!("{err}");
        assert!(display.contains("t-99"));
        assert!(display.contains("not found"));
    }

    #[test]
    fn test_server_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let err = ServerError::Io(io_err);
        let a2a: A2AError = (&err).into();
        assert_eq!(a2a.code, A2AErrorCode::InternalError.code());
        assert!(a2a.message.contains("refused"));

        let display = format!("{err}");
        assert!(display.contains("IO error"));
    }

    #[test]
    fn test_all_server_error_variants_to_jsonrpc() {
        let errors: Vec<ServerError> = vec![
            ServerError::A2A(A2AError::from(A2AErrorCode::TaskNotFound)),
            ServerError::TaskNotFound("t-1".into()),
            ServerError::TaskNotCancelable("t-2".into()),
            ServerError::Internal("boom".into()),
            ServerError::QueueClosed,
        ];

        for err in &errors {
            let rpc: JsonRpcError = err.into();
            assert!(rpc.code < 0, "All error codes should be negative");
            assert!(!rpc.message.is_empty(), "Error message should not be empty");
        }
    }

    #[test]
    fn test_server_error_a2a_with_data_roundtrip_to_jsonrpc() {
        let a2a = A2AError::new(A2AErrorCode::TaskNotFound, "task missing")
            .with_data(serde_json::json!({"task_id": "t-42", "hint": "check spelling"}));
        let server_err = ServerError::A2A(a2a);

        // ServerError::A2A → JsonRpcError should preserve data
        let rpc: JsonRpcError = (&server_err).into();
        assert_eq!(rpc.code, A2AErrorCode::TaskNotFound.code());
        assert!(rpc.message.contains("task missing"));
        let data = rpc.data.expect("data should be preserved in roundtrip");
        assert_eq!(data["task_id"], "t-42");
        assert_eq!(data["hint"], "check spelling");
    }

    #[test]
    fn test_server_error_is_std_error() {
        let err = ServerError::Internal("test".into());
        // Ensure it implements std::error::Error
        let _: &dyn std::error::Error = &err;

        // Test that Display works through the trait object
        let dyn_err: Box<dyn std::error::Error> = Box::new(err);
        assert!(dyn_err.to_string().contains("test"));
    }
}
