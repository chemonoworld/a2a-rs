use serde::{Deserialize, Serialize};

use crate::artifact::Artifact;
use crate::message::Message;

/// Task state machine - A2A Protocol v1.0
/// Terminal states: Completed, Failed, Canceled, Rejected
/// Interrupt states: InputRequired, AuthRequired
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TaskState {
    #[serde(rename = "TASK_STATE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "TASK_STATE_SUBMITTED")]
    Submitted,
    #[serde(rename = "TASK_STATE_WORKING")]
    Working,
    #[serde(rename = "TASK_STATE_COMPLETED")]
    Completed,
    #[serde(rename = "TASK_STATE_FAILED")]
    Failed,
    #[serde(rename = "TASK_STATE_CANCELED")]
    Canceled,
    #[serde(rename = "TASK_STATE_REJECTED")]
    Rejected,
    #[serde(rename = "TASK_STATE_INPUT_REQUIRED")]
    InputRequired,
    #[serde(rename = "TASK_STATE_AUTH_REQUIRED")]
    AuthRequired,
}

impl TaskState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Completed
                | TaskState::Failed
                | TaskState::Canceled
                | TaskState::Rejected
        )
    }

    pub fn is_interrupt(&self) -> bool {
        matches!(
            self,
            TaskState::InputRequired | TaskState::AuthRequired
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    pub state: TaskState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub context_id: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Artifact>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Optimistic concurrency version for task updates
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskVersion(pub i64);

impl TaskVersion {
    pub fn initial() -> Self {
        Self(0)
    }

    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_state_terminal() {
        assert!(TaskState::Completed.is_terminal());
        assert!(TaskState::Failed.is_terminal());
        assert!(TaskState::Canceled.is_terminal());
        assert!(TaskState::Rejected.is_terminal());
        assert!(!TaskState::Working.is_terminal());
        assert!(!TaskState::Submitted.is_terminal());
    }

    #[test]
    fn test_task_state_interrupt() {
        assert!(TaskState::InputRequired.is_interrupt());
        assert!(TaskState::AuthRequired.is_interrupt());
        assert!(!TaskState::Working.is_interrupt());
    }

    #[test]
    fn test_task_state_serde() {
        let state = TaskState::Working;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"TASK_STATE_WORKING\"");

        let deserialized: TaskState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TaskState::Working);
    }

    #[test]
    fn test_task_serde_roundtrip() {
        let task = Task {
            id: "task-123".into(),
            context_id: "ctx-456".into(),
            status: TaskStatus {
                state: TaskState::Submitted,
                message: None,
                timestamp: Some("2026-02-12T00:00:00Z".into()),
            },
            artifacts: None,
            history: None,
            metadata: None,
        };

        let json = serde_json::to_string(&task).unwrap();
        let deserialized: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "task-123");
        assert_eq!(deserialized.status.state, TaskState::Submitted);
    }

    #[test]
    fn test_task_version() {
        let v = TaskVersion::initial();
        assert_eq!(v.0, 0);
        assert_eq!(v.next().0, 1);
        assert_eq!(v.next().next().0, 2);
    }

    #[test]
    fn test_all_task_states_serde() {
        let states = vec![
            (TaskState::Unspecified, "\"TASK_STATE_UNSPECIFIED\""),
            (TaskState::Submitted, "\"TASK_STATE_SUBMITTED\""),
            (TaskState::Working, "\"TASK_STATE_WORKING\""),
            (TaskState::Completed, "\"TASK_STATE_COMPLETED\""),
            (TaskState::Failed, "\"TASK_STATE_FAILED\""),
            (TaskState::Canceled, "\"TASK_STATE_CANCELED\""),
            (TaskState::Rejected, "\"TASK_STATE_REJECTED\""),
            (TaskState::InputRequired, "\"TASK_STATE_INPUT_REQUIRED\""),
            (TaskState::AuthRequired, "\"TASK_STATE_AUTH_REQUIRED\""),
        ];

        for (state, expected_json) in states {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, expected_json, "Serialization failed for {state:?}");
            let deserialized: TaskState = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, state, "Deserialization failed for {state:?}");
        }
    }

    #[test]
    fn test_task_state_unspecified_not_terminal_not_interrupt() {
        assert!(!TaskState::Unspecified.is_terminal());
        assert!(!TaskState::Unspecified.is_interrupt());
    }

    #[test]
    fn test_task_version_ordering() {
        let v0 = TaskVersion(0);
        let v1 = TaskVersion(1);
        let v2 = TaskVersion(2);

        assert!(v0 < v1);
        assert!(v1 < v2);
        assert!(v0 < v2);
        assert_eq!(v1, TaskVersion(1));
    }

    #[test]
    fn test_task_version_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(TaskVersion(0));
        set.insert(TaskVersion(1));
        set.insert(TaskVersion(0)); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_invalid_task_state_deserialization() {
        let json = r#""INVALID_STATE""#;
        let result: Result<TaskState, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_task_status_with_message() {
        use crate::message::{Message, Role};
        use crate::part::{Part, PartContent};

        let status = TaskStatus {
            state: TaskState::InputRequired,
            message: Some(Message {
                message_id: "m-1".into(),
                role: Role::Agent,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "Please provide more info".into(),
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
            timestamp: Some("2026-02-12T10:00:00Z".into()),
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("TASK_STATE_INPUT_REQUIRED"));
        assert!(json.contains("\"timestamp\""));

        let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.state, TaskState::InputRequired);
        assert!(deserialized.message.is_some());
        assert!(deserialized.timestamp.is_some());
    }

    #[test]
    fn test_task_from_raw_json_minimal() {
        let json = r#"{
            "id": "t-raw",
            "contextId": "ctx-raw",
            "status": {"state": "TASK_STATE_SUBMITTED"}
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.id, "t-raw");
        assert_eq!(task.context_id, "ctx-raw");
        assert_eq!(task.status.state, TaskState::Submitted);
        assert!(task.artifacts.is_none());
        assert!(task.history.is_none());
        assert!(task.metadata.is_none());
    }

    #[test]
    fn test_task_status_from_raw_json_with_message() {
        use crate::message::Role;

        let json = r#"{
            "state": "TASK_STATE_AUTH_REQUIRED",
            "message": {
                "messageId": "m-auth",
                "role": "ROLE_AGENT",
                "parts": [{"text": "Authentication needed"}]
            },
            "timestamp": "2026-02-12T15:30:00Z"
        }"#;

        let status: TaskStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.state, TaskState::AuthRequired);
        assert!(status.state.is_interrupt());
        assert!(!status.state.is_terminal());
        let msg = status.message.unwrap();
        assert_eq!(msg.message_id, "m-auth");
        assert_eq!(msg.role, Role::Agent);
        assert_eq!(status.timestamp.as_deref(), Some("2026-02-12T15:30:00Z"));
    }

    #[test]
    fn test_task_state_cancelable_per_spec() {
        // Per A2A v1.0 spec: only submitted, working, input-required, auth-required are cancelable
        let cancelable = vec![
            TaskState::Submitted,
            TaskState::Working,
            TaskState::InputRequired,
            TaskState::AuthRequired,
        ];
        for state in &cancelable {
            assert!(
                !state.is_terminal(),
                "{state:?} should be cancelable (not terminal)"
            );
        }

        let not_cancelable = vec![
            TaskState::Completed,
            TaskState::Failed,
            TaskState::Canceled,
            TaskState::Rejected,
        ];
        for state in &not_cancelable {
            assert!(
                state.is_terminal(),
                "{state:?} should NOT be cancelable (terminal)"
            );
        }
    }

    #[test]
    fn test_task_from_full_raw_json() {
        let json = r#"{
            "id": "t-full",
            "contextId": "ctx-full",
            "status": {
                "state": "TASK_STATE_COMPLETED",
                "message": {
                    "messageId": "m-done",
                    "role": "ROLE_AGENT",
                    "parts": [{"text": "Task completed successfully"}]
                },
                "timestamp": "2026-02-12T18:00:00Z"
            },
            "artifacts": [
                {
                    "artifactId": "a-1",
                    "name": "Report",
                    "description": "Generated report",
                    "parts": [{"text": "Report content here"}],
                    "metadata": {"format": "markdown"}
                },
                {
                    "artifactId": "a-2",
                    "parts": [{"url": "https://storage.example.com/output.pdf"}],
                    "extensions": ["urn:a2a:ext:pdf"]
                }
            ],
            "history": [
                {
                    "messageId": "m-1",
                    "role": "ROLE_USER",
                    "parts": [{"text": "Generate a report"}]
                },
                {
                    "messageId": "m-2",
                    "role": "ROLE_AGENT",
                    "parts": [{"text": "Working on it..."}]
                }
            ],
            "metadata": {
                "processingTimeMs": 1500,
                "model": "agent-v2",
                "tags": ["report", "generated"]
            }
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.id, "t-full");
        assert_eq!(task.context_id, "ctx-full");
        assert_eq!(task.status.state, TaskState::Completed);
        assert!(task.status.state.is_terminal());
        assert!(task.status.message.is_some());
        assert!(task.status.timestamp.is_some());

        let artifacts = task.artifacts.unwrap();
        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0].name.as_deref(), Some("Report"));
        assert_eq!(artifacts[0].description.as_deref(), Some("Generated report"));
        assert_eq!(artifacts[1].extensions.as_ref().unwrap().len(), 1);

        let history = task.history.unwrap();
        assert_eq!(history.len(), 2);

        let meta = task.metadata.unwrap();
        assert_eq!(meta["processingTimeMs"], 1500);
        assert_eq!(meta["tags"][0], "report");
    }

    #[test]
    fn test_task_from_raw_json_failed_with_error_metadata() {
        use crate::part::PartContent;

        let json = r#"{
            "id": "t-fail",
            "contextId": "ctx-fail",
            "status": {
                "state": "TASK_STATE_FAILED",
                "message": {
                    "messageId": "m-err",
                    "role": "ROLE_AGENT",
                    "parts": [{"text": "Internal processing error: timeout after 30s"}]
                },
                "timestamp": "2026-02-12T20:00:00Z"
            },
            "metadata": {
                "errorCode": "TIMEOUT",
                "retryable": true,
                "attempts": 3
            }
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.id, "t-fail");
        assert_eq!(task.status.state, TaskState::Failed);
        assert!(task.status.state.is_terminal());
        assert!(!task.status.state.is_interrupt());

        let msg = task.status.message.unwrap();
        match &msg.parts[0].content {
            PartContent::Text { text } => assert!(text.contains("timeout")),
            _ => panic!("Expected Text part with timeout message"),
        }

        let meta = task.metadata.unwrap();
        assert_eq!(meta["errorCode"], "TIMEOUT");
        assert_eq!(meta["retryable"], true);
        assert_eq!(meta["attempts"], 3);
    }

    #[test]
    fn test_task_with_mixed_artifact_part_types() {
        use crate::part::PartContent;

        let json = r#"{
            "id": "t-mixed",
            "contextId": "ctx-mixed",
            "status": {"state": "TASK_STATE_COMPLETED"},
            "artifacts": [
                {
                    "artifactId": "a-text",
                    "parts": [{"text": "Summary of findings"}]
                },
                {
                    "artifactId": "a-data",
                    "parts": [{"data": {"results": [1, 2, 3], "count": 3}}]
                },
                {
                    "artifactId": "a-url",
                    "parts": [
                        {"url": "https://example.com/report.pdf", "mediaType": "application/pdf"}
                    ]
                }
            ]
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        let artifacts = task.artifacts.unwrap();
        assert_eq!(artifacts.len(), 3);

        match &artifacts[0].parts[0].content {
            PartContent::Text { text } => assert!(text.contains("findings")),
            _ => panic!("Expected Text part"),
        }
        match &artifacts[1].parts[0].content {
            PartContent::Data { data } => assert_eq!(data["count"], 3),
            _ => panic!("Expected Data part"),
        }
        match &artifacts[2].parts[0].content {
            PartContent::Url { url } => assert!(url.contains("report.pdf")),
            _ => panic!("Expected Url part"),
        }
    }

    #[test]
    fn test_task_missing_required_id_fails() {
        let json = r#"{
            "contextId": "ctx-1",
            "status": {"state": "TASK_STATE_SUBMITTED"}
        }"#;
        let result: Result<Task, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing id should fail deserialization");
    }

    #[test]
    fn test_task_missing_required_context_id_fails() {
        let json = r#"{
            "id": "t-1",
            "status": {"state": "TASK_STATE_SUBMITTED"}
        }"#;
        let result: Result<Task, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing contextId should fail deserialization");
    }

    #[test]
    fn test_task_missing_required_status_fails() {
        let json = r#"{
            "id": "t-1",
            "contextId": "ctx-1"
        }"#;
        let result: Result<Task, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing status should fail deserialization");
    }

    #[test]
    fn test_task_status_missing_required_state_fails() {
        let json = r#"{
            "message": {
                "messageId": "m-1",
                "role": "ROLE_AGENT",
                "parts": [{"text": "hello"}]
            },
            "timestamp": "2026-02-12T00:00:00Z"
        }"#;
        let result: Result<TaskStatus, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing state should fail deserialization");
    }

    #[test]
    fn test_task_metadata_deeply_nested_json() {
        let json = r#"{
            "id": "t-deep",
            "contextId": "ctx-1",
            "status": {"state": "TASK_STATE_COMPLETED"},
            "metadata": {
                "level1": {
                    "level2": {
                        "level3": {
                            "level4": {
                                "value": "deep",
                                "array": [1, [2, [3, [4]]]]
                            }
                        }
                    }
                }
            }
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        let meta = task.metadata.unwrap();
        assert_eq!(meta["level1"]["level2"]["level3"]["level4"]["value"], "deep");
        assert_eq!(meta["level1"]["level2"]["level3"]["level4"]["array"][1][1][1][0], 4);

        // Roundtrip preserves deep nesting
        let serialized = serde_json::to_string(&Task {
            id: "t-deep".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: Some(meta.clone()),
        })
        .unwrap();
        let roundtripped: Task = serde_json::from_str(&serialized).unwrap();
        assert_eq!(roundtripped.metadata.unwrap(), meta);
    }

    #[test]
    fn test_task_unicode_fields() {
        let json = r#"{
            "id": "task-日本語",
            "contextId": "ctx-中文-émoji-🎉",
            "status": {"state": "TASK_STATE_WORKING"},
            "metadata": {"label": "тест", "emoji": "🤖"}
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.id, "task-日本語");
        assert_eq!(task.context_id, "ctx-中文-émoji-🎉");
        assert_eq!(task.metadata.as_ref().unwrap()["label"], "тест");
        assert_eq!(task.metadata.as_ref().unwrap()["emoji"], "🤖");

        // Roundtrip preserves unicode
        let json_out = serde_json::to_string(&task).unwrap();
        let roundtripped: Task = serde_json::from_str(&json_out).unwrap();
        assert_eq!(roundtripped.id, "task-日本語");
    }

    #[test]
    fn test_task_state_classify_all_variants() {
        // Exhaustive classification test for each state
        let states = vec![
            (TaskState::Unspecified, false, false),
            (TaskState::Submitted, false, false),
            (TaskState::Working, false, false),
            (TaskState::Completed, true, false),
            (TaskState::Failed, true, false),
            (TaskState::Canceled, true, false),
            (TaskState::Rejected, true, false),
            (TaskState::InputRequired, false, true),
            (TaskState::AuthRequired, false, true),
        ];

        for (state, expected_terminal, expected_interrupt) in states {
            assert_eq!(
                state.is_terminal(),
                expected_terminal,
                "{state:?}.is_terminal() should be {expected_terminal}"
            );
            assert_eq!(
                state.is_interrupt(),
                expected_interrupt,
                "{state:?}.is_interrupt() should be {expected_interrupt}"
            );
            // No state should be both terminal AND interrupt
            assert!(
                !(state.is_terminal() && state.is_interrupt()),
                "{state:?} cannot be both terminal and interrupt"
            );
        }
    }

    #[test]
    fn test_task_version_copy_semantics() {
        let v1 = TaskVersion(5);
        let v2 = v1; // Copy
        assert_eq!(v1, v2);
        assert_eq!(v1.0, 5);
        assert_eq!(v2.0, 5);
    }

    #[test]
    fn test_task_with_empty_optional_collections() {
        let json = r#"{
            "id": "t-empty",
            "contextId": "ctx-1",
            "status": {"state": "TASK_STATE_SUBMITTED"},
            "artifacts": [],
            "history": []
        }"#;

        let task: Task = serde_json::from_str(json).unwrap();
        assert!(task.artifacts.unwrap().is_empty());
        assert!(task.history.unwrap().is_empty());
    }

    #[test]
    fn test_task_with_artifacts_and_history() {
        use crate::artifact::Artifact;
        use crate::message::{Message, Role};
        use crate::part::{Part, PartContent};

        let task = Task {
            id: "t-rich".into(),
            context_id: "ctx-1".into(),
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
                            text: "first artifact".into(),
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
                    name: Some("Second".into()),
                    description: None,
                    parts: vec![Part {
                        content: PartContent::Text {
                            text: "second artifact".into(),
                        },
                        metadata: None,
                        filename: None,
                        media_type: None,
                    }],
                    metadata: None,
                    extensions: None,
                },
            ]),
            history: Some(vec![Message {
                message_id: "m-1".into(),
                role: Role::User,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "hello".into(),
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
            }]),
            metadata: Some(serde_json::json!({"custom": true, "score": 0.95})),
        };

        let json = serde_json::to_string(&task).unwrap();
        let deserialized: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.artifacts.as_ref().unwrap().len(), 2);
        assert_eq!(deserialized.history.as_ref().unwrap().len(), 1);
        assert_eq!(
            deserialized.metadata.as_ref().unwrap()["score"],
            serde_json::json!(0.95)
        );
    }
}
