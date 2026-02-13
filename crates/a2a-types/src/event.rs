use serde::{Deserialize, Serialize};

use crate::artifact::Artifact;
use crate::message::Message;
use crate::task::{Task, TaskStatus};

/// Task status update event - emitted during streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusUpdateEvent {
    pub task_id: String,
    pub context_id: String,
    pub status: TaskStatus,
    #[serde(rename = "final")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_final: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Task artifact update event - emitted during streaming for chunk delivery
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactUpdateEvent {
    pub task_id: String,
    pub context_id: String,
    pub artifact: Artifact,
    #[serde(default)]
    pub append: bool,
    #[serde(default)]
    pub last_chunk: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// A2A Event - top-level type exchanged between server and client
///
/// v1.0: discriminated by JSON member presence (no "kind" field)
/// - Task: has "id", "contextId", "status" (no "role")
/// - Message: has "role", "parts", "messageId"
/// - TaskStatusUpdate: has "taskId", "status" (no "id", no "artifact")
/// - TaskArtifactUpdate: has "taskId", "artifact"
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Event {
    Task(Task),
    Message(Message),
    TaskStatusUpdate(TaskStatusUpdateEvent),
    TaskArtifactUpdate(TaskArtifactUpdateEvent),
}

impl Serialize for Event {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Event::Task(t) => t.serialize(serializer),
            Event::Message(m) => m.serialize(serializer),
            Event::TaskStatusUpdate(u) => u.serialize(serializer),
            Event::TaskArtifactUpdate(u) => u.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("Event must be a JSON object"))?;

        // Discriminate by member presence:
        // 1. "artifact" + "taskId" → TaskArtifactUpdate
        // 2. "taskId" + "status" (no "artifact", no "id") → TaskStatusUpdate
        // 3. "role" + "messageId" → Message
        // 4. "id" + "status" → Task

        if obj.contains_key("artifact") && obj.contains_key("taskId") {
            let event: TaskArtifactUpdateEvent =
                serde_json::from_value(value).map_err(serde::de::Error::custom)?;
            Ok(Event::TaskArtifactUpdate(event))
        } else if obj.contains_key("taskId") && obj.contains_key("status") && !obj.contains_key("id")
        {
            let event: TaskStatusUpdateEvent =
                serde_json::from_value(value).map_err(serde::de::Error::custom)?;
            Ok(Event::TaskStatusUpdate(event))
        } else if obj.contains_key("role") && obj.contains_key("messageId") {
            let msg: Message = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
            Ok(Event::Message(msg))
        } else if obj.contains_key("id") && obj.contains_key("status") {
            let task: Task = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
            Ok(Event::Task(task))
        } else {
            Err(serde::de::Error::custom(
                "Cannot determine Event variant from JSON fields",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Role;
    use crate::part::{Part, PartContent};
    use crate::task::TaskState;

    #[test]
    fn test_event_task_roundtrip() {
        let task = Task {
            id: "t-1".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Submitted,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        };
        let event = Event::Task(task);

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        match deserialized {
            Event::Task(t) => {
                assert_eq!(t.id, "t-1");
                assert_eq!(t.status.state, TaskState::Submitted);
            }
            _ => panic!("Expected Task variant"),
        }
    }

    #[test]
    fn test_event_message_roundtrip() {
        let msg = Message {
            message_id: "m-1".into(),
            role: Role::User,
            parts: vec![Part {
                content: PartContent::Text {
                    text: "hi".into(),
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
        let event = Event::Message(msg);

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        match deserialized {
            Event::Message(m) => {
                assert_eq!(m.message_id, "m-1");
                assert_eq!(m.role, Role::User);
            }
            _ => panic!("Expected Message variant"),
        }
    }

    #[test]
    fn test_event_status_update_roundtrip() {
        let update = TaskStatusUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        };
        let event = Event::TaskStatusUpdate(update);

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        match deserialized {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.task_id, "t-1");
                assert_eq!(u.status.state, TaskState::Working);
            }
            _ => panic!("Expected TaskStatusUpdate variant"),
        }
    }

    #[test]
    fn test_event_artifact_update_roundtrip() {
        let update = TaskArtifactUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            artifact: crate::artifact::Artifact {
                artifact_id: "a-1".into(),
                name: None,
                description: None,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "result".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                }],
                metadata: None,
                extensions: None,
            },
            append: false,
            last_chunk: true,
            metadata: None,
        };
        let event = Event::TaskArtifactUpdate(update);

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        match deserialized {
            Event::TaskArtifactUpdate(u) => {
                assert_eq!(u.task_id, "t-1");
                assert!(u.last_chunk);
            }
            _ => panic!("Expected TaskArtifactUpdate variant"),
        }
    }

    #[test]
    fn test_event_deserialization_ambiguous_missing_fields() {
        // JSON with no recognizable fields should fail
        let json = r#"{"unknown": "value"}"#;
        let result: Result<Event, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_event_deserialization_non_object() {
        // Non-object JSON should fail
        let json = r#""just a string""#;
        let result: Result<Event, _> = serde_json::from_str(json);
        assert!(result.is_err());

        let json = r#"42"#;
        let result: Result<Event, _> = serde_json::from_str(json);
        assert!(result.is_err());

        let json = r#"[1,2,3]"#;
        let result: Result<Event, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_event_task_with_all_optional_fields() {
        let task = Task {
            id: "t-full".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Working,
                message: Some(Message {
                    message_id: "m-status".into(),
                    role: Role::Agent,
                    parts: vec![Part {
                        content: PartContent::Text {
                            text: "Working on it".into(),
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
                }),
                timestamp: Some("2026-02-12T12:00:00Z".into()),
            },
            artifacts: Some(vec![crate::artifact::Artifact {
                artifact_id: "a-1".into(),
                name: Some("result".into()),
                description: Some("The result artifact".into()),
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "result data".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                }],
                metadata: None,
                extensions: None,
            }]),
            history: Some(vec![Message {
                message_id: "m-hist".into(),
                role: Role::User,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "original request".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                }],
                context_id: Some("ctx-1".into()),
                task_id: Some("t-full".into()),
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            }]),
            metadata: Some(serde_json::json!({"key": "value"})),
        };

        let event = Event::Task(task);
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: Event = serde_json::from_str(&json).unwrap();

        match deserialized {
            Event::Task(t) => {
                assert_eq!(t.id, "t-full");
                assert!(t.artifacts.is_some());
                assert_eq!(t.artifacts.as_ref().unwrap().len(), 1);
                assert!(t.history.is_some());
                assert_eq!(t.history.as_ref().unwrap().len(), 1);
                assert!(t.metadata.is_some());
                assert!(t.status.message.is_some());
                assert!(t.status.timestamp.is_some());
            }
            _ => panic!("Expected Task variant"),
        }
    }

    #[test]
    fn test_event_artifact_update_with_append() {
        let update = TaskArtifactUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            artifact: crate::artifact::Artifact {
                artifact_id: "a-1".into(),
                name: None,
                description: None,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "chunk 2".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                }],
                metadata: None,
                extensions: None,
            },
            append: true,
            last_chunk: false,
            metadata: Some(serde_json::json!({"chunk_index": 1})),
        };
        let event = Event::TaskArtifactUpdate(update);

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"append\":true"));
        assert!(json.contains("\"lastChunk\":false"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        match deserialized {
            Event::TaskArtifactUpdate(u) => {
                assert!(u.append);
                assert!(!u.last_chunk);
                assert!(u.metadata.is_some());
            }
            _ => panic!("Expected TaskArtifactUpdate variant"),
        }
    }

    #[test]
    fn test_status_update_from_raw_json_with_final() {
        // Verify that the "final" JSON key maps to is_final field
        let json = r#"{
            "taskId": "t-raw",
            "contextId": "ctx-raw",
            "status": {
                "state": "TASK_STATE_COMPLETED"
            },
            "final": true,
            "metadata": {"source": "external"}
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.task_id, "t-raw");
                assert_eq!(u.context_id, "ctx-raw");
                assert_eq!(u.status.state, TaskState::Completed);
                assert_eq!(u.is_final, Some(true));
                assert!(u.metadata.is_some());
                assert_eq!(u.metadata.unwrap()["source"], "external");
            }
            _ => panic!("Expected TaskStatusUpdate variant"),
        }
    }

    #[test]
    fn test_event_message_with_all_fields() {
        // Test Message Event with all optional fields populated
        let json = r#"{
            "messageId": "m-full",
            "role": "ROLE_AGENT",
            "parts": [{"text": "Hello"}],
            "contextId": "ctx-1",
            "taskId": "t-1",
            "metadata": {"key": "value"},
            "extensions": ["ext1"],
            "referenceTaskIds": ["ref-1", "ref-2"]
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::Message(m) => {
                assert_eq!(m.message_id, "m-full");
                assert_eq!(m.role, Role::Agent);
                assert_eq!(m.context_id.as_deref(), Some("ctx-1"));
                assert_eq!(m.task_id.as_deref(), Some("t-1"));
                assert!(m.metadata.is_some());
                assert_eq!(m.extensions.as_ref().unwrap().len(), 1);
                assert_eq!(m.reference_task_ids.as_ref().unwrap().len(), 2);
            }
            _ => panic!("Expected Message variant"),
        }
    }

    #[test]
    fn test_event_artifact_update_from_raw_json() {
        let json = r#"{
            "taskId": "t-art",
            "contextId": "ctx-art",
            "artifact": {
                "artifactId": "a-1",
                "parts": [{"text": "chunk data"}]
            },
            "append": true,
            "lastChunk": false,
            "metadata": {"chunkIndex": 5}
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::TaskArtifactUpdate(u) => {
                assert_eq!(u.task_id, "t-art");
                assert!(u.append);
                assert!(!u.last_chunk);
                assert_eq!(u.artifact.artifact_id, "a-1");
                assert_eq!(u.metadata.unwrap()["chunkIndex"], 5);
            }
            _ => panic!("Expected TaskArtifactUpdate variant"),
        }
    }

    #[test]
    fn test_event_deserialization_ignores_extra_fields() {
        // Unknown fields in JSON should be gracefully ignored
        let json = r#"{
            "taskId": "t-extra",
            "contextId": "ctx-extra",
            "status": {"state": "TASK_STATE_WORKING"},
            "unknownField": "should be ignored",
            "anotherExtra": 42
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.task_id, "t-extra");
                assert_eq!(u.status.state, TaskState::Working);
            }
            _ => panic!("Expected TaskStatusUpdate variant"),
        }
    }

    #[test]
    fn test_event_task_has_both_id_and_taskid() {
        // Edge case: JSON has both "id" and "taskId" fields
        // If "artifact" is present with "taskId" → TaskArtifactUpdate takes priority
        let json = r#"{
            "taskId": "t-1",
            "contextId": "ctx-1",
            "artifact": {
                "artifactId": "a-1",
                "parts": [{"text": "data"}]
            },
            "id": "should-not-affect",
            "append": false,
            "lastChunk": true
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::TaskArtifactUpdate(u) => {
                assert_eq!(u.task_id, "t-1");
                assert_eq!(u.artifact.artifact_id, "a-1");
            }
            _ => panic!("Expected TaskArtifactUpdate (artifact + taskId takes priority)"),
        }
    }

    #[test]
    fn test_status_update_with_embedded_message() {
        // InputRequired state with a message in the status
        let json = r#"{
            "taskId": "t-input",
            "contextId": "ctx-input",
            "status": {
                "state": "TASK_STATE_INPUT_REQUIRED",
                "message": {
                    "messageId": "m-prompt",
                    "role": "ROLE_AGENT",
                    "parts": [{"text": "Please provide your API key"}]
                },
                "timestamp": "2026-02-12T10:00:00Z"
            },
            "final": false
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.task_id, "t-input");
                assert_eq!(u.status.state, TaskState::InputRequired);
                assert_eq!(u.is_final, Some(false));
                let msg = u.status.message.as_ref().expect("Should have message");
                assert_eq!(msg.message_id, "m-prompt");
                assert_eq!(msg.role, Role::Agent);
                assert_eq!(msg.parts.len(), 1);
                match &msg.parts[0].content {
                    PartContent::Text { text } => {
                        assert!(text.contains("API key"));
                    }
                    _ => panic!("Expected Text part"),
                }
                assert_eq!(u.status.timestamp.as_deref(), Some("2026-02-12T10:00:00Z"));
            }
            _ => panic!("Expected TaskStatusUpdate with InputRequired"),
        }
    }

    #[test]
    fn test_artifact_update_defaults_for_append_and_last_chunk() {
        // When append and lastChunk are omitted, they should default to false
        let json = r#"{
            "taskId": "t-def",
            "contextId": "ctx-def",
            "artifact": {
                "artifactId": "a-1",
                "parts": [{"text": "chunk"}]
            }
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::TaskArtifactUpdate(u) => {
                assert!(!u.append, "append should default to false");
                assert!(!u.last_chunk, "lastChunk should default to false");
            }
            _ => panic!("Expected TaskArtifactUpdate"),
        }
    }

    #[test]
    fn test_event_status_update_auth_required_from_raw_json() {
        let json = r#"{
            "taskId": "t-auth",
            "contextId": "ctx-auth",
            "status": {
                "state": "TASK_STATE_AUTH_REQUIRED",
                "message": {
                    "messageId": "m-auth-prompt",
                    "role": "ROLE_AGENT",
                    "parts": [{"text": "OAuth2 authentication required. Redirect to: https://auth.example.com/authorize"}]
                },
                "timestamp": "2026-02-12T16:00:00Z"
            },
            "final": false
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.task_id, "t-auth");
                assert_eq!(u.context_id, "ctx-auth");
                assert_eq!(u.status.state, TaskState::AuthRequired);
                assert!(u.status.state.is_interrupt());
                assert!(!u.status.state.is_terminal());
                assert_eq!(u.is_final, Some(false));
                let msg = u.status.message.as_ref().expect("Should have auth message");
                assert_eq!(msg.message_id, "m-auth-prompt");
                assert_eq!(msg.role, Role::Agent);
                match &msg.parts[0].content {
                    PartContent::Text { text } => {
                        assert!(text.contains("OAuth2"));
                        assert!(text.contains("auth.example.com"));
                    }
                    _ => panic!("Expected Text part"),
                }
                assert_eq!(u.status.timestamp.as_deref(), Some("2026-02-12T16:00:00Z"));
            }
            _ => panic!("Expected TaskStatusUpdate with AuthRequired"),
        }
    }

    #[test]
    fn test_event_task_from_raw_json_with_all_states() {
        // Test Task Event deserialization across all 9 states
        let states = vec![
            "TASK_STATE_UNSPECIFIED",
            "TASK_STATE_SUBMITTED",
            "TASK_STATE_WORKING",
            "TASK_STATE_COMPLETED",
            "TASK_STATE_FAILED",
            "TASK_STATE_CANCELED",
            "TASK_STATE_REJECTED",
            "TASK_STATE_INPUT_REQUIRED",
            "TASK_STATE_AUTH_REQUIRED",
        ];

        for state_str in &states {
            let json = format!(
                r#"{{"id":"t-1","contextId":"c-1","status":{{"state":"{}"}}}}"#,
                state_str
            );
            let event: Event = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("Failed to deserialize state {state_str}: {e}"));
            match event {
                Event::Task(t) => {
                    let re_json = serde_json::to_string(&t.status.state).unwrap();
                    assert_eq!(re_json, format!("\"{}\"", state_str));
                }
                _ => panic!("Expected Task variant for state {state_str}"),
            }
        }
    }

    #[test]
    fn test_status_update_final_field_rename() {
        let update = TaskStatusUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            is_final: Some(true),
            metadata: None,
        };

        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("\"final\":true"));
    }

    #[test]
    fn test_event_message_from_raw_json() {
        let json = r#"{
            "messageId": "m-raw",
            "role": "ROLE_USER",
            "parts": [
                {"text": "Hello from raw JSON"},
                {"data": {"key": "value"}}
            ],
            "contextId": "ctx-msg",
            "taskId": "t-msg",
            "referenceTaskIds": ["ref-1"]
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match &event {
            Event::Message(m) => {
                assert_eq!(m.message_id, "m-raw");
                assert_eq!(m.role, Role::User);
                assert_eq!(m.parts.len(), 2);
                assert_eq!(m.context_id.as_deref(), Some("ctx-msg"));
                assert_eq!(m.task_id.as_deref(), Some("t-msg"));
                assert_eq!(m.reference_task_ids.as_ref().unwrap().len(), 1);
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_event_empty_json_object_fails() {
        let json = r#"{}"#;
        let result: Result<Event, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Empty JSON object should fail Event deserialization");
    }

    #[test]
    fn test_event_discrimination_both_id_and_taskid_no_artifact() {
        // Edge case: JSON has both "id" and "taskId" with "status" but no "artifact"
        // The discriminator checks !contains_key("id") for TaskStatusUpdate,
        // so this falls through to the Task branch (id + status)
        let json = r#"{
            "id": "t-1",
            "contextId": "ctx-1",
            "taskId": "t-1",
            "status": {"state": "TASK_STATE_COMPLETED"}
        }"#;

        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::Task(t) => {
                assert_eq!(t.id, "t-1");
                assert_eq!(t.status.state, TaskState::Completed);
            }
            _ => panic!("Expected Task variant (id + status takes precedence when both id and taskId present)"),
        }
    }

    #[test]
    fn test_task_status_update_event_missing_required_fields() {
        // Missing taskId
        let json = r#"{
            "contextId": "ctx-1",
            "status": {"state": "TASK_STATE_WORKING"}
        }"#;
        // This should fail because without "taskId" it won't match TaskStatusUpdate,
        // and without "id" it won't match Task, so it fails
        let result: Result<Event, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing taskId and id should fail");

        // Missing contextId in TaskStatusUpdate (has taskId and status but no id)
        let json = r#"{
            "taskId": "t-1",
            "status": {"state": "TASK_STATE_WORKING"}
        }"#;
        let result: Result<Event, _> = serde_json::from_str(json);
        // This matches the TaskStatusUpdate branch (taskId + status + !id),
        // but contextId is required in TaskStatusUpdateEvent
        assert!(result.is_err(), "Missing contextId should fail");
    }

    #[test]
    fn test_event_task_with_artifacts_serialization_roundtrip() {
        use crate::part::{Part, PartContent};
        use crate::task::TaskState;

        let task = Task {
            id: "t-art-rt".into(),
            context_id: "ctx-art".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: Some(vec![
                Artifact {
                    artifact_id: "a-1".into(),
                    name: Some("First".into()),
                    description: None,
                    parts: vec![Part {
                        content: PartContent::Text {
                            text: "artifact text".into(),
                        },
                        metadata: None,
                        filename: None,
                        media_type: None,
                    }],
                    metadata: None,
                    extensions: None,
                },
                Artifact {
                    artifact_id: "a-2".into(),
                    name: None,
                    description: None,
                    parts: vec![Part {
                        content: PartContent::Data {
                            data: serde_json::json!({"key": "value"}),
                        },
                        metadata: None,
                        filename: None,
                        media_type: Some("application/json".into()),
                    }],
                    metadata: None,
                    extensions: None,
                },
            ]),
            history: None,
            metadata: None,
        };

        let event = Event::Task(task);
        let json = serde_json::to_string(&event).unwrap();

        // Roundtrip through Event deserialization
        let deserialized: Event = serde_json::from_str(&json).unwrap();
        match deserialized {
            Event::Task(t) => {
                assert_eq!(t.id, "t-art-rt");
                let artifacts = t.artifacts.unwrap();
                assert_eq!(artifacts.len(), 2);
                assert_eq!(artifacts[0].artifact_id, "a-1");
                assert_eq!(artifacts[0].name.as_deref(), Some("First"));
                match &artifacts[1].parts[0].content {
                    PartContent::Data { data } => assert_eq!(data["key"], "value"),
                    _ => panic!("Expected Data part in second artifact"),
                }
            }
            _ => panic!("Expected Task event"),
        }
    }

    #[test]
    fn test_event_message_with_url_and_data_parts() {
        use crate::message::Role;
        use crate::part::{Part, PartContent};

        let msg = Message {
            message_id: "m-multi".into(),
            role: Role::Agent,
            parts: vec![
                Part {
                    content: PartContent::Text {
                        text: "Here are the results:".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                },
                Part {
                    content: PartContent::Url {
                        url: "https://cdn.example.com/report.pdf".into(),
                    },
                    metadata: None,
                    filename: Some("report.pdf".into()),
                    media_type: Some("application/pdf".into()),
                },
            ],
            context_id: Some("ctx-multi".into()),
            task_id: Some("t-multi".into()),
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let event = Event::Message(msg);
        let json = serde_json::to_string(&event).unwrap();

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        match deserialized {
            Event::Message(m) => {
                assert_eq!(m.message_id, "m-multi");
                assert_eq!(m.parts.len(), 2);
                match &m.parts[1].content {
                    PartContent::Url { url } => assert!(url.contains("report.pdf")),
                    _ => panic!("Expected Url part"),
                }
                assert_eq!(m.parts[1].filename.as_deref(), Some("report.pdf"));
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_event_artifact_update_chunking_from_raw_json() {
        // Test chunked artifact delivery with append=true, lastChunk=false
        let json_chunk1 = r#"{
            "taskId": "t-chunk",
            "contextId": "ctx-chunk",
            "artifact": {
                "artifactId": "a-stream",
                "name": "Streaming Output",
                "parts": [{"text": "First chunk of data..."}]
            },
            "append": false,
            "lastChunk": false
        }"#;

        let event1: Event = serde_json::from_str(json_chunk1).unwrap();
        match &event1 {
            Event::TaskArtifactUpdate(u) => {
                assert_eq!(u.task_id, "t-chunk");
                assert_eq!(u.artifact.artifact_id, "a-stream");
                assert!(!u.append);
                assert!(!u.last_chunk);
            }
            _ => panic!("Expected TaskArtifactUpdate"),
        }

        // Second chunk: append=true, lastChunk=true
        let json_chunk2 = r#"{
            "taskId": "t-chunk",
            "contextId": "ctx-chunk",
            "artifact": {
                "artifactId": "a-stream",
                "parts": [{"text": "...final chunk"}]
            },
            "append": true,
            "lastChunk": true,
            "metadata": {"chunkIndex": 2}
        }"#;

        let event2: Event = serde_json::from_str(json_chunk2).unwrap();
        match &event2 {
            Event::TaskArtifactUpdate(u) => {
                assert!(u.append);
                assert!(u.last_chunk);
                assert!(u.metadata.is_some());
                assert_eq!(u.metadata.as_ref().unwrap()["chunkIndex"], 2);
            }
            _ => panic!("Expected TaskArtifactUpdate"),
        }
    }
}
