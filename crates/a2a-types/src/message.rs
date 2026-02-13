use serde::{Deserialize, Serialize};

use crate::part::Part;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    #[serde(rename = "ROLE_USER")]
    User,
    #[serde(rename = "ROLE_AGENT")]
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub message_id: String,
    pub role: Role,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_task_ids: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::part::PartContent;

    #[test]
    fn test_role_serde() {
        let json = serde_json::to_string(&Role::User).unwrap();
        assert_eq!(json, "\"ROLE_USER\"");

        let agent: Role = serde_json::from_str("\"ROLE_AGENT\"").unwrap();
        assert_eq!(agent, Role::Agent);
    }

    #[test]
    fn test_message_serde_roundtrip() {
        let msg = Message {
            message_id: "msg-1".into(),
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
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"messageId\""));
        assert!(json.contains("ROLE_USER"));

        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.message_id, "msg-1");
        assert_eq!(deserialized.role, Role::User);
    }

    #[test]
    fn test_message_optional_fields_omitted() {
        let msg = Message {
            message_id: "msg-1".into(),
            role: Role::Agent,
            parts: vec![],
            context_id: None,
            task_id: None,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("contextId"));
        assert!(!json.contains("taskId"));
        assert!(!json.contains("metadata"));
    }

    #[test]
    fn test_message_all_fields() {
        let msg = Message {
            message_id: "msg-42".into(),
            role: Role::Agent,
            parts: vec![
                Part {
                    content: PartContent::Text {
                        text: "Hello".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                },
                Part {
                    content: PartContent::Text {
                        text: "World".into(),
                    },
                    metadata: None,
                    filename: Some("output.txt".into()),
                    media_type: Some("text/plain".into()),
                },
            ],
            context_id: Some("ctx-1".into()),
            task_id: Some("task-1".into()),
            metadata: Some(serde_json::json!({"source": "test", "priority": 1})),
            extensions: Some(vec!["ext-1".into(), "ext-2".into()]),
            reference_task_ids: Some(vec!["ref-task-1".into(), "ref-task-2".into()]),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"contextId\""));
        assert!(json.contains("\"taskId\""));
        assert!(json.contains("\"metadata\""));
        assert!(json.contains("\"extensions\""));
        assert!(json.contains("\"referenceTaskIds\""));

        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.parts.len(), 2);
        assert_eq!(deserialized.context_id.as_deref(), Some("ctx-1"));
        assert_eq!(deserialized.task_id.as_deref(), Some("task-1"));
        assert_eq!(deserialized.extensions.as_ref().unwrap().len(), 2);
        assert_eq!(deserialized.reference_task_ids.as_ref().unwrap().len(), 2);
        assert_eq!(deserialized.metadata.as_ref().unwrap()["priority"], 1);
    }

    #[test]
    fn test_message_with_multiple_part_types() {
        let msg = Message {
            message_id: "msg-multi".into(),
            role: Role::User,
            parts: vec![
                Part {
                    content: PartContent::Text {
                        text: "See the image:".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                },
                Part {
                    content: PartContent::Data {
                        data: "aGVsbG8=".into(),
                    },
                    metadata: None,
                    filename: Some("image.png".into()),
                    media_type: Some("image/png".into()),
                },
            ],
            context_id: None,
            task_id: None,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.parts.len(), 2);

        match &deserialized.parts[0].content {
            PartContent::Text { text } => assert_eq!(text, "See the image:"),
            _ => panic!("Expected Text part"),
        }
        match &deserialized.parts[1].content {
            PartContent::Data { data } => assert_eq!(data, "aGVsbG8="),
            _ => panic!("Expected Data part"),
        }
        assert_eq!(deserialized.parts[1].filename.as_deref(), Some("image.png"));
    }

    #[test]
    fn test_invalid_role_deserialization() {
        let json = r#""INVALID_ROLE""#;
        let result: Result<Role, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_from_raw_json() {
        let json = r#"{
            "messageId": "msg-raw",
            "role": "ROLE_AGENT",
            "parts": [
                {"text": "Hello from agent"},
                {"data": "base64data=="}
            ],
            "contextId": "ctx-raw",
            "taskId": "task-raw",
            "referenceTaskIds": ["ref-1", "ref-2", "ref-3"]
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_id, "msg-raw");
        assert_eq!(msg.role, Role::Agent);
        assert_eq!(msg.parts.len(), 2);
        assert_eq!(msg.context_id.as_deref(), Some("ctx-raw"));
        assert_eq!(msg.task_id.as_deref(), Some("task-raw"));
        assert_eq!(msg.reference_task_ids.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_message_empty_parts_from_raw_json() {
        let json = r#"{
            "messageId": "msg-empty",
            "role": "ROLE_USER",
            "parts": []
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_id, "msg-empty");
        assert_eq!(msg.role, Role::User);
        assert!(msg.parts.is_empty());
        assert!(msg.context_id.is_none());
        assert!(msg.task_id.is_none());
        assert!(msg.metadata.is_none());
        assert!(msg.extensions.is_none());
        assert!(msg.reference_task_ids.is_none());

        // Roundtrip
        let json_out = serde_json::to_string(&msg).unwrap();
        assert!(!json_out.contains("contextId"));
        assert!(!json_out.contains("extensions"));
    }

    #[test]
    fn test_message_with_raw_binary_part() {
        let msg = Message {
            message_id: "msg-bin".into(),
            role: Role::User,
            parts: vec![Part {
                content: PartContent::Raw {
                    raw: vec![0xCA, 0xFE, 0xBA, 0xBE],
                },
                metadata: None,
                filename: Some("data.bin".into()),
                media_type: Some("application/octet-stream".into()),
            }],
            context_id: None,
            task_id: None,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"raw\""));
        assert!(json.contains("\"filename\":\"data.bin\""));

        let deserialized: Message = serde_json::from_str(&json).unwrap();
        match &deserialized.parts[0].content {
            PartContent::Raw { raw } => assert_eq!(raw, &[0xCA, 0xFE, 0xBA, 0xBE]),
            _ => panic!("Expected Raw part"),
        }
    }

    #[test]
    fn test_message_with_extensions_from_raw_json() {
        let json = r#"{
            "messageId": "msg-ext",
            "role": "ROLE_USER",
            "parts": [{"text": "hello"}],
            "extensions": ["urn:a2a:ext:custom1", "urn:a2a:ext:custom2"],
            "metadata": {"source": "test"}
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_id, "msg-ext");
        let exts = msg.extensions.as_ref().unwrap();
        assert_eq!(exts.len(), 2);
        assert_eq!(exts[0], "urn:a2a:ext:custom1");
        assert!(msg.metadata.is_some());
    }

    #[test]
    fn test_message_missing_required_message_id_fails() {
        let json = r#"{
            "role": "ROLE_USER",
            "parts": [{"text": "hello"}]
        }"#;
        let result: Result<Message, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_missing_required_role_fails() {
        let json = r#"{
            "messageId": "m-1",
            "parts": [{"text": "hello"}]
        }"#;
        let result: Result<Message, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_missing_required_parts_fails() {
        let json = r#"{
            "messageId": "m-1",
            "role": "ROLE_USER"
        }"#;
        let result: Result<Message, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_with_url_part() {
        let msg = Message {
            message_id: "msg-url".into(),
            role: Role::Agent,
            parts: vec![Part {
                content: PartContent::Url {
                    url: "https://cdn.example.com/output.pdf".into(),
                },
                metadata: None,
                filename: Some("output.pdf".into()),
                media_type: Some("application/pdf".into()),
            }],
            context_id: None,
            task_id: None,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"url\":"));
        assert!(json.contains("output.pdf"));

        let deserialized: Message = serde_json::from_str(&json).unwrap();
        match &deserialized.parts[0].content {
            PartContent::Url { url } => assert!(url.contains("output.pdf")),
            _ => panic!("Expected Url part"),
        }
        assert_eq!(deserialized.parts[0].filename.as_deref(), Some("output.pdf"));
        assert_eq!(deserialized.parts[0].media_type.as_deref(), Some("application/pdf"));
    }

    #[test]
    fn test_message_unknown_fields_ignored() {
        let json = r#"{
            "messageId": "m-unk",
            "role": "ROLE_USER",
            "parts": [{"text": "hello"}],
            "unknownField": "should be ignored",
            "extraNumber": 42
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.message_id, "m-unk");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.parts.len(), 1);
    }

    #[test]
    fn test_message_with_url_and_data_parts_from_raw_json() {
        let json = r#"{
            "messageId": "msg-mixed",
            "role": "ROLE_AGENT",
            "parts": [
                {
                    "text": "Here is the result:",
                    "metadata": {"source": "generated"}
                },
                {
                    "url": "https://storage.example.com/document.pdf",
                    "filename": "document.pdf",
                    "mediaType": "application/pdf"
                },
                {
                    "data": {"key": "value", "items": [1, 2, 3]},
                    "metadata": {"format": "structured"}
                }
            ]
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.parts.len(), 3);

        // First part: text with metadata
        match &msg.parts[0].content {
            PartContent::Text { text } => assert_eq!(text, "Here is the result:"),
            _ => panic!("Expected Text part"),
        }
        assert!(msg.parts[0].metadata.is_some());
        assert_eq!(msg.parts[0].metadata.as_ref().unwrap()["source"], "generated");

        // Second part: url with filename/mediaType
        match &msg.parts[1].content {
            PartContent::Url { url } => {
                assert_eq!(url, "https://storage.example.com/document.pdf");
            }
            _ => panic!("Expected Url part"),
        }
        assert_eq!(msg.parts[1].filename.as_deref(), Some("document.pdf"));
        assert_eq!(msg.parts[1].media_type.as_deref(), Some("application/pdf"));

        // Third part: structured data with metadata
        match &msg.parts[2].content {
            PartContent::Data { data } => {
                assert_eq!(data["key"], "value");
                assert_eq!(data["items"][1], 2);
            }
            _ => panic!("Expected Data part"),
        }
        assert!(msg.parts[2].metadata.is_some());
    }
}
