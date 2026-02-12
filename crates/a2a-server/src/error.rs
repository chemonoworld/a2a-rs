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
}
