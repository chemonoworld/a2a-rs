use serde::{Deserialize, Serialize};

use crate::message::Message;
use crate::push::TaskPushNotificationConfig;

/// JSON-RPC 2.0 request ID
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(i64),
    String(String),
    Null,
}

/// JSON-RPC 2.0 request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    pub id: JsonRpcId,
}

/// JSON-RPC 2.0 response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: JsonRpcId,
}

/// JSON-RPC 2.0 error object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    pub fn success(id: JsonRpcId, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: JsonRpcId, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

// --- A2A Request Parameter Types ---

/// Parameters for message/send and message/stream
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub message: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration: Option<MessageSendConfiguration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSendConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_output_modes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_notification_config: Option<TaskPushNotificationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
}

/// Parameters for tasks/get
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTaskRequest {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_length: Option<i32>,
}

/// Parameters for tasks/cancel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelTaskRequest {
    pub id: String,
}

/// Parameters for tasks/resubscribe
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResubscriptionRequest {
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_id_variants() {
        let num: JsonRpcId = serde_json::from_str("42").unwrap();
        match num {
            JsonRpcId::Number(n) => assert_eq!(n, 42),
            _ => panic!("Expected Number"),
        }

        let s: JsonRpcId = serde_json::from_str("\"abc\"").unwrap();
        match s {
            JsonRpcId::String(ref s) => assert_eq!(s, "abc"),
            _ => panic!("Expected String"),
        }

        let null: JsonRpcId = serde_json::from_str("null").unwrap();
        match null {
            JsonRpcId::Null => {}
            _ => panic!("Expected Null"),
        }
    }

    #[test]
    fn test_jsonrpc_request_serde() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "message/send".into(),
            params: Some(serde_json::json!({"message": {}})),
            id: JsonRpcId::Number(1),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"message/send\""));

        let deserialized: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.method, "message/send");
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let resp = JsonRpcResponse::success(
            JsonRpcId::Number(1),
            serde_json::json!({"id": "task-1"}),
        );

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\":"));
        assert!(!json.contains("\"error\":"));
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let resp = JsonRpcResponse::error(
            JsonRpcId::Number(1),
            JsonRpcError {
                code: -32601,
                message: "Method not found".into(),
                data: None,
            },
        );

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\":"));
        assert!(!json.contains("\"result\":"));
    }

    #[test]
    fn test_send_message_request_serde() {
        use crate::message::Role;
        use crate::part::{Part, PartContent};

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
                context_id: None,
                task_id: None,
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            },
            configuration: Some(MessageSendConfiguration {
                blocking: Some(true),
                accepted_output_modes: None,
                push_notification_config: None,
                history_length: None,
            }),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"blocking\":true"));

        let deserialized: SendMessageRequest = serde_json::from_str(&json).unwrap();
        assert!(deserialized.configuration.unwrap().blocking.unwrap());
    }
}
