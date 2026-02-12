use serde::{Deserialize, Serialize};

/// A2A JSON-RPC error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum A2AErrorCode {
    // Standard JSON-RPC errors
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,

    // A2A-specific errors
    TaskNotFound,
    TaskNotCancelable,
    PushNotSupported,
    UnsupportedOperation,
    IncompatibleContentTypes,
    ExtensionsNotSupported,
    AgentCardNotFound,

    // Auth errors
    Unauthenticated,
    Unauthorized,
}

impl A2AErrorCode {
    pub fn code(&self) -> i32 {
        match self {
            A2AErrorCode::ParseError => -32700,
            A2AErrorCode::InvalidRequest => -32600,
            A2AErrorCode::MethodNotFound => -32601,
            A2AErrorCode::InvalidParams => -32602,
            A2AErrorCode::InternalError => -32603,
            A2AErrorCode::TaskNotFound => -32001,
            A2AErrorCode::TaskNotCancelable => -32002,
            A2AErrorCode::PushNotSupported => -32003,
            A2AErrorCode::UnsupportedOperation => -32004,
            A2AErrorCode::IncompatibleContentTypes => -32005,
            A2AErrorCode::ExtensionsNotSupported => -32006,
            A2AErrorCode::AgentCardNotFound => -32007,
            A2AErrorCode::Unauthenticated => -31401,
            A2AErrorCode::Unauthorized => -31403,
        }
    }

    pub fn default_message(&self) -> &'static str {
        match self {
            A2AErrorCode::ParseError => "Parse error",
            A2AErrorCode::InvalidRequest => "Invalid request",
            A2AErrorCode::MethodNotFound => "Method not found",
            A2AErrorCode::InvalidParams => "Invalid params",
            A2AErrorCode::InternalError => "Internal error",
            A2AErrorCode::TaskNotFound => "Task not found",
            A2AErrorCode::TaskNotCancelable => "Task not cancelable",
            A2AErrorCode::PushNotSupported => "Push notifications not supported",
            A2AErrorCode::UnsupportedOperation => "Unsupported operation",
            A2AErrorCode::IncompatibleContentTypes => "Incompatible content types",
            A2AErrorCode::ExtensionsNotSupported => "Extensions not supported",
            A2AErrorCode::AgentCardNotFound => "Agent card not found",
            A2AErrorCode::Unauthenticated => "Unauthenticated",
            A2AErrorCode::Unauthorized => "Unauthorized",
        }
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            -32700 => Some(A2AErrorCode::ParseError),
            -32600 => Some(A2AErrorCode::InvalidRequest),
            -32601 => Some(A2AErrorCode::MethodNotFound),
            -32602 => Some(A2AErrorCode::InvalidParams),
            -32603 => Some(A2AErrorCode::InternalError),
            -32001 => Some(A2AErrorCode::TaskNotFound),
            -32002 => Some(A2AErrorCode::TaskNotCancelable),
            -32003 => Some(A2AErrorCode::PushNotSupported),
            -32004 => Some(A2AErrorCode::UnsupportedOperation),
            -32005 => Some(A2AErrorCode::IncompatibleContentTypes),
            -32006 => Some(A2AErrorCode::ExtensionsNotSupported),
            -32007 => Some(A2AErrorCode::AgentCardNotFound),
            -31401 => Some(A2AErrorCode::Unauthenticated),
            -31403 => Some(A2AErrorCode::Unauthorized),
            _ => None,
        }
    }
}

/// A2A error as transmitted in JSON-RPC error responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl A2AError {
    pub fn new(code: A2AErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code.code(),
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn error_code(&self) -> Option<A2AErrorCode> {
        A2AErrorCode::from_code(self.code)
    }
}

impl From<A2AErrorCode> for A2AError {
    fn from(code: A2AErrorCode) -> Self {
        Self {
            code: code.code(),
            message: code.default_message().into(),
            data: None,
        }
    }
}

impl std::fmt::Display for A2AError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for A2AError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(A2AErrorCode::ParseError.code(), -32700);
        assert_eq!(A2AErrorCode::TaskNotFound.code(), -32001);
        assert_eq!(A2AErrorCode::Unauthenticated.code(), -31401);
    }

    #[test]
    fn test_error_from_code() {
        assert_eq!(
            A2AErrorCode::from_code(-32700),
            Some(A2AErrorCode::ParseError)
        );
        assert_eq!(A2AErrorCode::from_code(-99999), None);
    }

    #[test]
    fn test_a2a_error_serde() {
        let err = A2AError::new(A2AErrorCode::TaskNotFound, "Task abc not found");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("-32001"));

        let deserialized: A2AError = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.code, -32001);
        assert_eq!(deserialized.message, "Task abc not found");
    }

    #[test]
    fn test_a2a_error_from_code() {
        let err = A2AError::from(A2AErrorCode::InternalError);
        assert_eq!(err.code, -32603);
        assert_eq!(err.message, "Internal error");
    }
}
