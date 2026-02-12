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
    }
}
