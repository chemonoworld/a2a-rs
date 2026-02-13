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

    #[test]
    fn test_jsonrpc_request_no_params() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: "tasks/get".into(),
            params: None,
            id: JsonRpcId::String("req-1".into()),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("\"params\""));

        let deserialized: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert!(deserialized.params.is_none());
    }

    #[test]
    fn test_jsonrpc_response_roundtrip_with_string_id() {
        let resp = JsonRpcResponse::success(
            JsonRpcId::String("req-abc".into()),
            serde_json::json!({"id": "task-1", "status": "ok"}),
        );

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert!(deserialized.result.is_some());
        assert!(deserialized.error.is_none());
        match deserialized.id {
            JsonRpcId::String(s) => assert_eq!(s, "req-abc"),
            _ => panic!("Expected String ID"),
        }
    }

    #[test]
    fn test_jsonrpc_response_with_error_data() {
        let resp = JsonRpcResponse::error(
            JsonRpcId::Number(42),
            JsonRpcError {
                code: -32001,
                message: "Task not found".into(),
                data: Some(serde_json::json!({"task_id": "t-missing"})),
            },
        );

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"data\""));
        assert!(json.contains("t-missing"));

        let deserialized: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        let err = deserialized.error.unwrap();
        assert_eq!(err.code, -32001);
        assert!(err.data.is_some());
    }

    #[test]
    fn test_jsonrpc_null_id_response() {
        let resp = JsonRpcResponse::error(
            JsonRpcId::Null,
            JsonRpcError {
                code: -32700,
                message: "Parse error".into(),
                data: None,
            },
        );

        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        match deserialized.id {
            JsonRpcId::Null => {}
            _ => panic!("Expected Null ID"),
        }
    }

    #[test]
    fn test_message_send_configuration_all_fields() {
        use crate::push::TaskPushNotificationConfig;

        let config = MessageSendConfiguration {
            blocking: Some(true),
            accepted_output_modes: Some(vec!["text/plain".into(), "application/json".into()]),
            push_notification_config: Some(TaskPushNotificationConfig {
                url: "https://webhook.example.com/notify".into(),
                token: Some("bearer-token".into()),
                authentication: None,
            }),
            history_length: Some(10),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"blocking\":true"));
        assert!(json.contains("\"acceptedOutputModes\""));
        assert!(json.contains("\"pushNotificationConfig\""));
        assert!(json.contains("\"historyLength\":10"));

        let deserialized: MessageSendConfiguration = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.accepted_output_modes.as_ref().unwrap().len(), 2);
        assert!(deserialized.push_notification_config.is_some());
        assert_eq!(deserialized.history_length, Some(10));
    }

    #[test]
    fn test_get_task_request_serde() {
        let req = GetTaskRequest {
            id: "task-123".into(),
            history_length: Some(5),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"historyLength\":5"));

        let deserialized: GetTaskRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "task-123");
        assert_eq!(deserialized.history_length, Some(5));
    }

    #[test]
    fn test_cancel_task_request_serde() {
        let req = CancelTaskRequest {
            id: "task-456".into(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: CancelTaskRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "task-456");
    }

    #[test]
    fn test_message_send_configuration_minimal() {
        let config = MessageSendConfiguration {
            blocking: None,
            accepted_output_modes: None,
            push_notification_config: None,
            history_length: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        // All fields are None, so JSON should be empty object
        assert_eq!(json, "{}");

        let deserialized: MessageSendConfiguration = serde_json::from_str(&json).unwrap();
        assert!(deserialized.blocking.is_none());
        assert!(deserialized.accepted_output_modes.is_none());
        assert!(deserialized.push_notification_config.is_none());
        assert!(deserialized.history_length.is_none());
    }

    #[test]
    fn test_get_task_request_minimal() {
        let json = r#"{"id": "task-1"}"#;
        let req: GetTaskRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "task-1");
        assert!(req.history_length.is_none());
    }

    #[test]
    fn test_jsonrpc_response_both_result_and_error_absent() {
        // Valid according to our model - both can be None
        let json = r#"{"jsonrpc": "2.0", "id": 1}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.result.is_none());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_resubscribe_request_serde() {
        let req = TaskResubscriptionRequest {
            id: "task-789".into(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: TaskResubscriptionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "task-789");
    }

    #[test]
    fn test_message_send_configuration_from_raw_json() {
        let json = r#"{
            "blocking": false,
            "acceptedOutputModes": ["text/plain", "image/png"],
            "historyLength": 25,
            "pushNotificationConfig": {
                "url": "https://webhook.test/notify",
                "token": "secret"
            }
        }"#;

        let config: MessageSendConfiguration = serde_json::from_str(json).unwrap();
        assert_eq!(config.blocking, Some(false));
        assert_eq!(config.accepted_output_modes.as_ref().unwrap().len(), 2);
        assert_eq!(config.history_length, Some(25));
        let push = config.push_notification_config.unwrap();
        assert_eq!(push.url, "https://webhook.test/notify");
        assert_eq!(push.token.as_deref(), Some("secret"));
    }

    #[test]
    fn test_jsonrpc_request_from_raw_json_with_extra_fields() {
        // JSON-RPC request with extra fields should be deserializable
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "message/send",
            "params": {"message": {"messageId": "m-1", "role": "ROLE_USER", "parts": []}},
            "id": 99
        }"#;

        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "message/send");
        match req.id {
            JsonRpcId::Number(n) => assert_eq!(n, 99),
            _ => panic!("Expected Number ID"),
        }
    }

    #[test]
    fn test_send_message_request_from_raw_json_full() {
        let json = r#"{
            "message": {
                "messageId": "m-raw",
                "role": "ROLE_USER",
                "parts": [
                    {"text": "Generate report"},
                    {"data": {"format": "pdf"}}
                ],
                "contextId": "ctx-existing",
                "taskId": "t-existing",
                "referenceTaskIds": ["ref-1", "ref-2"]
            },
            "configuration": {
                "blocking": true,
                "acceptedOutputModes": ["text/plain", "application/pdf"],
                "historyLength": 50,
                "pushNotificationConfig": {
                    "url": "https://webhook.example.com/a2a",
                    "token": "secret-token"
                }
            }
        }"#;

        let req: SendMessageRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.message.message_id, "m-raw");
        assert_eq!(req.message.parts.len(), 2);
        assert_eq!(req.message.context_id.as_deref(), Some("ctx-existing"));
        assert_eq!(req.message.task_id.as_deref(), Some("t-existing"));
        assert_eq!(req.message.reference_task_ids.as_ref().unwrap().len(), 2);

        let config = req.configuration.unwrap();
        assert_eq!(config.blocking, Some(true));
        assert_eq!(config.accepted_output_modes.as_ref().unwrap().len(), 2);
        assert_eq!(config.history_length, Some(50));
        let push = config.push_notification_config.unwrap();
        assert_eq!(push.url, "https://webhook.example.com/a2a");
        assert_eq!(push.token.as_deref(), Some("secret-token"));
    }

    #[test]
    fn test_jsonrpc_negative_id() {
        let json = r#"{"jsonrpc":"2.0","method":"test","id":-1}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        match req.id {
            JsonRpcId::Number(n) => assert_eq!(n, -1),
            _ => panic!("Expected negative Number ID"),
        }
    }

    #[test]
    fn test_jsonrpc_response_success_constructor() {
        let resp = JsonRpcResponse::success(
            JsonRpcId::Number(42),
            serde_json::json!({"status": "ok"}),
        );
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        match resp.id {
            JsonRpcId::Number(n) => assert_eq!(n, 42),
            _ => panic!("Expected Number ID"),
        }
        assert_eq!(resp.result.unwrap()["status"], "ok");
    }

    #[test]
    fn test_jsonrpc_response_error_constructor() {
        let resp = JsonRpcResponse::error(
            JsonRpcId::String("req-abc".into()),
            JsonRpcError {
                code: -32600,
                message: "Invalid Request".into(),
                data: Some(serde_json::json!({"details": "missing method"})),
            },
        );
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.data.unwrap()["details"], "missing method");
    }

    #[test]
    fn test_send_message_request_missing_message_fails() {
        let json = r#"{
            "configuration": {"blocking": true}
        }"#;
        let result: Result<SendMessageRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing message should fail deserialization");
    }

    #[test]
    fn test_get_task_request_missing_id_fails() {
        let json = r#"{"historyLength": 5}"#;
        let result: Result<GetTaskRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing id should fail deserialization");
    }

    #[test]
    fn test_cancel_task_request_missing_id_fails() {
        let json = r#"{}"#;
        let result: Result<CancelTaskRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing id should fail deserialization");
    }

    #[test]
    fn test_jsonrpc_error_from_raw_json() {
        let json = r#"{
            "code": -32600,
            "message": "Invalid Request",
            "data": {"details": "missing method field"}
        }"#;
        let err: JsonRpcError = serde_json::from_str(json).unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "Invalid Request");
        assert_eq!(err.data.as_ref().unwrap()["details"], "missing method field");
    }

    #[test]
    fn test_jsonrpc_request_missing_required_fields_fails() {
        // Missing jsonrpc
        let json = r#"{"method": "test", "id": 1}"#;
        let result: Result<JsonRpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing jsonrpc should fail");

        // Missing method
        let json = r#"{"jsonrpc": "2.0", "id": 1}"#;
        let result: Result<JsonRpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing method should fail");

        // Missing id
        let json = r#"{"jsonrpc": "2.0", "method": "test"}"#;
        let result: Result<JsonRpcRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing id should fail");
    }

    #[test]
    fn test_jsonrpc_id_boundary_values() {
        // i64::MAX
        let json_max = format!("{}", i64::MAX);
        let id_max: JsonRpcId = serde_json::from_str(&json_max).unwrap();
        match id_max {
            JsonRpcId::Number(n) => assert_eq!(n, i64::MAX),
            _ => panic!("Expected Number"),
        }

        // i64::MIN
        let json_min = format!("{}", i64::MIN);
        let id_min: JsonRpcId = serde_json::from_str(&json_min).unwrap();
        match id_min {
            JsonRpcId::Number(n) => assert_eq!(n, i64::MIN),
            _ => panic!("Expected Number"),
        }

        // Roundtrip preserves extreme values
        let resp = JsonRpcResponse::success(JsonRpcId::Number(i64::MAX), serde_json::json!(null));
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        match parsed.id {
            JsonRpcId::Number(n) => assert_eq!(n, i64::MAX),
            _ => panic!("Expected Number"),
        }
    }

    #[test]
    fn test_jsonrpc_response_both_result_and_error_present() {
        // Per JSON-RPC 2.0 spec, both result and error should not be present,
        // but our type allows it for flexibility in parsing
        let json = r#"{
            "jsonrpc": "2.0",
            "result": {"status": "ok"},
            "error": {"code": -32600, "message": "Invalid"},
            "id": 1
        }"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.result.is_some(), "Result should be present");
        assert!(resp.error.is_some(), "Error should be present");
        assert_eq!(resp.error.unwrap().code, -32600);
    }

    #[test]
    fn test_jsonrpc_request_params_null_vs_absent() {
        // Params explicitly null
        let json_null = r#"{"jsonrpc":"2.0","method":"test","params":null,"id":1}"#;
        let req_null: JsonRpcRequest = serde_json::from_str(json_null).unwrap();
        // serde deserializes `null` as None for Option
        assert!(req_null.params.is_none());

        // Params absent
        let json_absent = r#"{"jsonrpc":"2.0","method":"test","id":1}"#;
        let req_absent: JsonRpcRequest = serde_json::from_str(json_absent).unwrap();
        assert!(req_absent.params.is_none());
    }

    #[test]
    fn test_send_message_request_with_push_notification_config() {
        let json = r#"{
            "message": {
                "messageId": "m-push",
                "role": "ROLE_USER",
                "parts": [{"text": "hello"}]
            },
            "configuration": {
                "blocking": false,
                "pushNotificationConfig": {
                    "url": "https://webhook.example.com/notify",
                    "token": "bearer-token-123"
                }
            }
        }"#;
        let req: SendMessageRequest = serde_json::from_str(json).unwrap();
        let config = req.configuration.unwrap();
        assert_eq!(config.blocking, Some(false));
        let push = config.push_notification_config.unwrap();
        assert_eq!(push.url, "https://webhook.example.com/notify");
        assert_eq!(push.token.as_deref(), Some("bearer-token-123"));
    }
}
