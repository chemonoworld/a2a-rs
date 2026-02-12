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
}
