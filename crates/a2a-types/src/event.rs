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
}
