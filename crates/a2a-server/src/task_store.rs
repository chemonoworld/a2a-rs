use std::collections::HashMap;

use a2a_types::{Event, Task, TaskVersion};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::error::ServerError;

/// Persistent store for tasks.
#[async_trait]
pub trait TaskStore: Send + Sync + 'static {
    async fn save(&self, event: &Event, version: TaskVersion) -> Result<(), ServerError>;
    async fn get(&self, task_id: &str) -> Result<Option<(Task, TaskVersion)>, ServerError>;
    async fn list(
        &self,
        context_id: Option<&str>,
        page_size: Option<i32>,
        page_token: Option<&str>,
    ) -> Result<(Vec<Task>, Option<String>), ServerError>;
}

/// In-memory task store backed by `RwLock<HashMap>`.
pub struct InMemoryTaskStore {
    tasks: RwLock<HashMap<String, (Task, TaskVersion)>>,
}

impl InMemoryTaskStore {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryTaskStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskStore for InMemoryTaskStore {
    async fn save(&self, event: &Event, version: TaskVersion) -> Result<(), ServerError> {
        let mut tasks = self.tasks.write().await;

        match event {
            Event::Task(task) => {
                tasks.insert(task.id.clone(), (task.clone(), version));
            }
            Event::Message(msg) => {
                let task_id = msg.task_id.as_deref().ok_or_else(|| {
                    ServerError::Internal("Message event missing task_id".into())
                })?;
                if let Some((task, ver)) = tasks.get_mut(task_id) {
                    let history = task.history.get_or_insert_with(Vec::new);
                    history.push(msg.clone());
                    *ver = version;
                } else {
                    return Err(ServerError::TaskNotFound(task_id.to_string()));
                }
            }
            Event::TaskStatusUpdate(update) => {
                if let Some((task, ver)) = tasks.get_mut(&update.task_id) {
                    task.status = update.status.clone();
                    *ver = version;
                } else {
                    return Err(ServerError::TaskNotFound(update.task_id.clone()));
                }
            }
            Event::TaskArtifactUpdate(update) => {
                if let Some((task, ver)) = tasks.get_mut(&update.task_id) {
                    let artifacts = task.artifacts.get_or_insert_with(Vec::new);
                    if update.append {
                        // Append to existing artifact with same ID
                        if let Some(existing) = artifacts
                            .iter_mut()
                            .find(|a| a.artifact_id == update.artifact.artifact_id)
                        {
                            existing.parts.extend(update.artifact.parts.clone());
                        } else {
                            artifacts.push(update.artifact.clone());
                        }
                    } else {
                        // Replace or add
                        if let Some(existing) = artifacts
                            .iter_mut()
                            .find(|a| a.artifact_id == update.artifact.artifact_id)
                        {
                            *existing = update.artifact.clone();
                        } else {
                            artifacts.push(update.artifact.clone());
                        }
                    }
                    *ver = version;
                } else {
                    return Err(ServerError::TaskNotFound(update.task_id.clone()));
                }
            }
            _ => {
                return Err(ServerError::Internal("Unknown event variant".into()));
            }
        }

        Ok(())
    }

    async fn get(&self, task_id: &str) -> Result<Option<(Task, TaskVersion)>, ServerError> {
        let tasks = self.tasks.read().await;
        Ok(tasks.get(task_id).cloned())
    }

    async fn list(
        &self,
        context_id: Option<&str>,
        page_size: Option<i32>,
        page_token: Option<&str>,
    ) -> Result<(Vec<Task>, Option<String>), ServerError> {
        let tasks = self.tasks.read().await;
        let mut items: Vec<&Task> = tasks.values().map(|(t, _)| t).collect();

        // Filter by context_id
        if let Some(ctx) = context_id {
            items.retain(|t| t.context_id == ctx);
        }

        // Sort by ID for stable pagination
        items.sort_by(|a, b| a.id.cmp(&b.id));

        // Apply cursor (page_token = last seen task_id)
        if let Some(token) = page_token {
            items.retain(|t| t.id.as_str() > token);
        }

        let limit = page_size.unwrap_or(100) as usize;
        let has_more = items.len() > limit;
        let page: Vec<Task> = items.into_iter().take(limit).cloned().collect();
        let next_token = if has_more {
            page.last().map(|t| t.id.clone())
        } else {
            None
        };

        Ok((page, next_token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2a_types::{
        Artifact, Message, Part, PartContent, Role, TaskArtifactUpdateEvent, TaskState,
        TaskStatus, TaskStatusUpdateEvent,
    };

    fn make_task(id: &str, ctx: &str) -> Task {
        Task {
            id: id.into(),
            context_id: ctx.into(),
            status: TaskStatus {
                state: TaskState::Submitted,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_save_and_get_task() {
        let store = InMemoryTaskStore::new();
        let task = make_task("t-1", "ctx-1");
        let event = Event::Task(task.clone());

        store.save(&event, TaskVersion::initial()).await.unwrap();

        let result = store.get("t-1").await.unwrap();
        assert!(result.is_some());
        let (t, v) = result.unwrap();
        assert_eq!(t.id, "t-1");
        assert_eq!(v, TaskVersion::initial());
    }

    #[tokio::test]
    async fn test_save_status_update() {
        let store = InMemoryTaskStore::new();
        let task = make_task("t-1", "ctx-1");
        store
            .save(&Event::Task(task), TaskVersion::initial())
            .await
            .unwrap();

        let update = Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        });

        store.save(&update, TaskVersion(1)).await.unwrap();

        let (t, v) = store.get("t-1").await.unwrap().unwrap();
        assert_eq!(t.status.state, TaskState::Working);
        assert_eq!(v, TaskVersion(1));
    }

    #[tokio::test]
    async fn test_save_artifact_update() {
        let store = InMemoryTaskStore::new();
        let task = make_task("t-1", "ctx-1");
        store
            .save(&Event::Task(task), TaskVersion::initial())
            .await
            .unwrap();

        let artifact = Artifact {
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
        };

        let update = Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            artifact,
            append: false,
            last_chunk: true,
            metadata: None,
        });

        store.save(&update, TaskVersion(1)).await.unwrap();

        let (t, _) = store.get("t-1").await.unwrap().unwrap();
        assert_eq!(t.artifacts.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_save_message_event() {
        let store = InMemoryTaskStore::new();
        let task = make_task("t-1", "ctx-1");
        store
            .save(&Event::Task(task), TaskVersion::initial())
            .await
            .unwrap();

        let msg = Message {
            message_id: "m-1".into(),
            role: Role::Agent,
            parts: vec![Part {
                content: PartContent::Text {
                    text: "hello".into(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            }],
            context_id: None,
            task_id: Some("t-1".into()),
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        store
            .save(&Event::Message(msg), TaskVersion(1))
            .await
            .unwrap();

        let (t, _) = store.get("t-1").await.unwrap().unwrap();
        assert_eq!(t.history.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_list_with_context_filter() {
        let store = InMemoryTaskStore::new();
        store
            .save(&Event::Task(make_task("t-1", "ctx-a")), TaskVersion(0))
            .await
            .unwrap();
        store
            .save(&Event::Task(make_task("t-2", "ctx-b")), TaskVersion(0))
            .await
            .unwrap();
        store
            .save(&Event::Task(make_task("t-3", "ctx-a")), TaskVersion(0))
            .await
            .unwrap();

        let (tasks, _) = store.list(Some("ctx-a"), None, None).await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().all(|t| t.context_id == "ctx-a"));
    }

    #[tokio::test]
    async fn test_list_pagination() {
        let store = InMemoryTaskStore::new();
        for i in 0..5 {
            store
                .save(
                    &Event::Task(make_task(&format!("t-{i}"), "ctx-1")),
                    TaskVersion(0),
                )
                .await
                .unwrap();
        }

        let (page1, token) = store.list(None, Some(2), None).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert!(token.is_some());

        let (page2, _) = store
            .list(None, Some(2), token.as_deref())
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);
        // Pages should not overlap
        assert_ne!(page1[0].id, page2[0].id);
    }

    #[tokio::test]
    async fn test_get_nonexistent_task() {
        let store = InMemoryTaskStore::new();
        let result = store.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_status_update_nonexistent_task() {
        let store = InMemoryTaskStore::new();
        let update = Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "nonexistent".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        });

        let result = store.save(&update, TaskVersion(1)).await;
        assert!(matches!(result, Err(ServerError::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_artifact_update_nonexistent_task() {
        let store = InMemoryTaskStore::new();
        let update = Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
            task_id: "nonexistent".into(),
            context_id: "ctx-1".into(),
            artifact: Artifact {
                artifact_id: "a-1".into(),
                name: None,
                description: None,
                parts: vec![],
                metadata: None,
                extensions: None,
            },
            append: false,
            last_chunk: true,
            metadata: None,
        });

        let result = store.save(&update, TaskVersion(1)).await;
        assert!(matches!(result, Err(ServerError::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_message_event_without_task_id() {
        let store = InMemoryTaskStore::new();
        let msg = Message {
            message_id: "m-1".into(),
            role: Role::User,
            parts: vec![],
            context_id: None,
            task_id: None, // No task_id
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let result = store.save(&Event::Message(msg), TaskVersion(1)).await;
        assert!(matches!(result, Err(ServerError::Internal(_))));
    }

    #[tokio::test]
    async fn test_message_event_task_not_found() {
        let store = InMemoryTaskStore::new();
        let msg = Message {
            message_id: "m-1".into(),
            role: Role::User,
            parts: vec![],
            context_id: None,
            task_id: Some("nonexistent".into()),
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let result = store.save(&Event::Message(msg), TaskVersion(1)).await;
        assert!(matches!(result, Err(ServerError::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_artifact_append_mode() {
        let store = InMemoryTaskStore::new();
        let task = make_task("t-1", "ctx-1");
        store
            .save(&Event::Task(task), TaskVersion::initial())
            .await
            .unwrap();

        // First artifact chunk
        let artifact1 = Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            artifact: Artifact {
                artifact_id: "a-1".into(),
                name: None,
                description: None,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "chunk 1".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                }],
                metadata: None,
                extensions: None,
            },
            append: false,
            last_chunk: false,
            metadata: None,
        });
        store.save(&artifact1, TaskVersion(1)).await.unwrap();

        // Second artifact chunk (append=true)
        let artifact2 = Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            artifact: Artifact {
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
            last_chunk: true,
            metadata: None,
        });
        store.save(&artifact2, TaskVersion(2)).await.unwrap();

        let (t, _) = store.get("t-1").await.unwrap().unwrap();
        let artifacts = t.artifacts.unwrap();
        assert_eq!(artifacts.len(), 1); // Same artifact_id, so merged
        assert_eq!(artifacts[0].parts.len(), 2); // Two chunks appended
    }

    #[tokio::test]
    async fn test_artifact_replace_mode() {
        let store = InMemoryTaskStore::new();
        let task = make_task("t-1", "ctx-1");
        store
            .save(&Event::Task(task), TaskVersion::initial())
            .await
            .unwrap();

        // First artifact
        let artifact1 = Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            artifact: Artifact {
                artifact_id: "a-1".into(),
                name: Some("v1".into()),
                description: None,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "old content".into(),
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
        });
        store.save(&artifact1, TaskVersion(1)).await.unwrap();

        // Replace with same artifact_id (append=false)
        let artifact2 = Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            artifact: Artifact {
                artifact_id: "a-1".into(),
                name: Some("v2".into()),
                description: None,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "new content".into(),
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
        });
        store.save(&artifact2, TaskVersion(2)).await.unwrap();

        let (t, _) = store.get("t-1").await.unwrap().unwrap();
        let artifacts = t.artifacts.unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].name.as_deref(), Some("v2")); // Replaced
        assert_eq!(artifacts[0].parts.len(), 1);
    }

    #[tokio::test]
    async fn test_multiple_artifacts_different_ids() {
        let store = InMemoryTaskStore::new();
        let task = make_task("t-1", "ctx-1");
        store
            .save(&Event::Task(task), TaskVersion::initial())
            .await
            .unwrap();

        for i in 1..=3 {
            let update = Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                task_id: "t-1".into(),
                context_id: "ctx-1".into(),
                artifact: Artifact {
                    artifact_id: format!("a-{i}"),
                    name: None,
                    description: None,
                    parts: vec![Part {
                        content: PartContent::Text {
                            text: format!("artifact {i}"),
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
            });
            store.save(&update, TaskVersion(i)).await.unwrap();
        }

        let (t, _) = store.get("t-1").await.unwrap().unwrap();
        assert_eq!(t.artifacts.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_multiple_messages_in_history() {
        let store = InMemoryTaskStore::new();
        let task = make_task("t-1", "ctx-1");
        store
            .save(&Event::Task(task), TaskVersion::initial())
            .await
            .unwrap();

        for i in 1..=3 {
            let msg = Message {
                message_id: format!("m-{i}"),
                role: if i % 2 == 1 {
                    Role::User
                } else {
                    Role::Agent
                },
                parts: vec![Part {
                    content: PartContent::Text {
                        text: format!("message {i}"),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                }],
                context_id: None,
                task_id: Some("t-1".into()),
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            };
            store.save(&Event::Message(msg), TaskVersion(i)).await.unwrap();
        }

        let (t, _) = store.get("t-1").await.unwrap().unwrap();
        let history = t.history.unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].role, Role::User);
        assert_eq!(history[1].role, Role::Agent);
        assert_eq!(history[2].role, Role::User);
    }

    #[tokio::test]
    async fn test_list_empty_store() {
        let store = InMemoryTaskStore::new();
        let (tasks, token) = store.list(None, None, None).await.unwrap();
        assert!(tasks.is_empty());
        assert!(token.is_none());
    }

    #[tokio::test]
    async fn test_concurrent_reads_and_writes() {
        let store = std::sync::Arc::new(InMemoryTaskStore::new());

        // Write 10 tasks concurrently
        let mut handles = Vec::new();
        for i in 0..10 {
            let store = store.clone();
            handles.push(tokio::spawn(async move {
                let task = make_task(&format!("t-{i}"), "ctx-concurrent");
                store.save(&Event::Task(task), TaskVersion(0)).await.unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        // Read all concurrently
        let mut read_handles = Vec::new();
        for i in 0..10 {
            let store = store.clone();
            read_handles.push(tokio::spawn(async move {
                let result = store.get(&format!("t-{i}")).await.unwrap();
                assert!(result.is_some(), "Task t-{i} should exist");
            }));
        }
        for h in read_handles {
            h.await.unwrap();
        }

        // Verify list returns all
        let (tasks, _) = store.list(Some("ctx-concurrent"), None, None).await.unwrap();
        assert_eq!(tasks.len(), 10);
    }

    #[tokio::test]
    async fn test_version_tracking_through_transitions() {
        let store = InMemoryTaskStore::new();
        let task = make_task("t-ver", "ctx-1");

        // Save initial task: version 0
        store.save(&Event::Task(task), TaskVersion::initial()).await.unwrap();
        let (_, v) = store.get("t-ver").await.unwrap().unwrap();
        assert_eq!(v, TaskVersion(0));

        // Update status: version 1
        store.save(&Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "t-ver".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        }), TaskVersion(1)).await.unwrap();
        let (_, v) = store.get("t-ver").await.unwrap().unwrap();
        assert_eq!(v, TaskVersion(1));

        // Add artifact: version 2
        store.save(&Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
            task_id: "t-ver".into(),
            context_id: "ctx-1".into(),
            artifact: Artifact {
                artifact_id: "a-1".into(),
                name: None,
                description: None,
                parts: vec![],
                metadata: None,
                extensions: None,
            },
            append: false,
            last_chunk: true,
            metadata: None,
        }), TaskVersion(2)).await.unwrap();
        let (_, v) = store.get("t-ver").await.unwrap().unwrap();
        assert_eq!(v, TaskVersion(2));

        // Add message: version 3
        store.save(&Event::Message(Message {
            message_id: "m-1".into(),
            role: Role::Agent,
            parts: vec![],
            context_id: None,
            task_id: Some("t-ver".into()),
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        }), TaskVersion(3)).await.unwrap();
        let (_, v) = store.get("t-ver").await.unwrap().unwrap();
        assert_eq!(v, TaskVersion(3));
    }

    #[tokio::test]
    async fn test_list_all_pages() {
        let store = InMemoryTaskStore::new();
        for i in 0..7 {
            store
                .save(
                    &Event::Task(make_task(&format!("t-{i:03}"), "ctx-1")),
                    TaskVersion(0),
                )
                .await
                .unwrap();
        }

        // Iterate page by page (page_size=3)
        let mut all_tasks = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let (page, next_token) = store.list(None, Some(3), token.as_deref()).await.unwrap();
            if page.is_empty() {
                break;
            }
            all_tasks.extend(page);
            token = next_token;
            if token.is_none() {
                break;
            }
        }

        assert_eq!(all_tasks.len(), 7, "Should collect all 7 tasks across pages");
        // Verify no duplicates
        let mut ids: Vec<String> = all_tasks.iter().map(|t| t.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 7, "All task IDs should be unique");
    }

    #[tokio::test]
    async fn test_list_no_matching_context() {
        let store = InMemoryTaskStore::new();
        store
            .save(&Event::Task(make_task("t-1", "ctx-a")), TaskVersion(0))
            .await
            .unwrap();

        let (tasks, _) = store.list(Some("ctx-nonexistent"), None, None).await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_artifact_append_nonexistent_creates_new() {
        // When append=true but no artifact with that ID exists, it should push a new one
        let store = InMemoryTaskStore::new();
        store
            .save(&Event::Task(make_task("t-1", "ctx-1")), TaskVersion::initial())
            .await
            .unwrap();

        let update = Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
            task_id: "t-1".into(),
            context_id: "ctx-1".into(),
            artifact: Artifact {
                artifact_id: "a-new".into(),
                name: Some("Appended New".into()),
                description: None,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "appended chunk".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                }],
                metadata: None,
                extensions: None,
            },
            append: true, // append=true but no existing artifact
            last_chunk: true,
            metadata: None,
        });
        store.save(&update, TaskVersion(1)).await.unwrap();

        let (t, _) = store.get("t-1").await.unwrap().unwrap();
        let artifacts = t.artifacts.unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].artifact_id, "a-new");
        assert_eq!(artifacts[0].parts.len(), 1);
    }

    #[tokio::test]
    async fn test_save_task_event_overwrites_existing() {
        let store = InMemoryTaskStore::new();
        store
            .save(&Event::Task(make_task("t-1", "ctx-1")), TaskVersion::initial())
            .await
            .unwrap();

        // Save a new Task event with the same id but different state
        let updated = Task {
            id: "t-1".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: Some(vec![Artifact {
                artifact_id: "a-1".into(),
                name: None,
                description: None,
                parts: vec![],
                metadata: None,
                extensions: None,
            }]),
            history: None,
            metadata: Some(serde_json::json!({"overwritten": true})),
        };
        store
            .save(&Event::Task(updated), TaskVersion(1))
            .await
            .unwrap();

        let (t, v) = store.get("t-1").await.unwrap().unwrap();
        assert_eq!(t.status.state, TaskState::Completed);
        assert!(t.artifacts.is_some());
        assert_eq!(t.metadata.unwrap()["overwritten"], true);
        assert_eq!(v, TaskVersion(1));
    }

    #[test]
    fn test_inmemory_task_store_default() {
        let s1 = InMemoryTaskStore::new();
        let s2 = InMemoryTaskStore::default();
        // Both should be created successfully (smoke test for Default impl)
        let _ = s1;
        let _ = s2;
    }

    #[tokio::test]
    async fn test_list_with_context_and_pagination_combined() {
        let store = InMemoryTaskStore::new();
        // Add 5 tasks in ctx-a and 3 in ctx-b
        for i in 0..5 {
            store
                .save(
                    &Event::Task(make_task(&format!("t-a{i:02}"), "ctx-a")),
                    TaskVersion(0),
                )
                .await
                .unwrap();
        }
        for i in 0..3 {
            store
                .save(
                    &Event::Task(make_task(&format!("t-b{i:02}"), "ctx-b")),
                    TaskVersion(0),
                )
                .await
                .unwrap();
        }

        // Paginate through ctx-a with page_size=2
        let (page1, token1) = store.list(Some("ctx-a"), Some(2), None).await.unwrap();
        assert_eq!(page1.len(), 2);
        assert!(page1.iter().all(|t| t.context_id == "ctx-a"));
        assert!(token1.is_some());

        let (page2, token2) = store
            .list(Some("ctx-a"), Some(2), token1.as_deref())
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);
        assert!(page2.iter().all(|t| t.context_id == "ctx-a"));
        assert!(token2.is_some());

        let (page3, token3) = store
            .list(Some("ctx-a"), Some(2), token2.as_deref())
            .await
            .unwrap();
        assert_eq!(page3.len(), 1);
        assert!(token3.is_none());

        // Total across all pages should be 5
        let total = page1.len() + page2.len() + page3.len();
        assert_eq!(total, 5);
    }

    #[tokio::test]
    async fn test_list_page_token_beyond_all_tasks() {
        let store = InMemoryTaskStore::new();
        store
            .save(&Event::Task(make_task("t-01", "ctx-1")), TaskVersion(0))
            .await
            .unwrap();
        store
            .save(&Event::Task(make_task("t-02", "ctx-1")), TaskVersion(0))
            .await
            .unwrap();

        // Use a page_token that sorts after all task IDs
        let (tasks, token) = store
            .list(None, None, Some("zzz-beyond-all"))
            .await
            .unwrap();
        assert!(tasks.is_empty(), "No tasks should be returned");
        assert!(token.is_none());
    }

    #[tokio::test]
    async fn test_status_update_preserves_artifacts_and_history() {
        let store = InMemoryTaskStore::new();
        let mut task = make_task("t-1", "ctx-1");
        task.artifacts = Some(vec![Artifact {
            artifact_id: "a-1".into(),
            name: None,
            description: None,
            parts: vec![Part {
                content: PartContent::Text {
                    text: "artifact data".into(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            }],
            metadata: None,
            extensions: None,
        }]);
        store
            .save(&Event::Task(task), TaskVersion::initial())
            .await
            .unwrap();

        // Add a message to history
        store
            .save(
                &Event::Message(Message {
                    message_id: "m-1".into(),
                    role: Role::User,
                    parts: vec![],
                    context_id: None,
                    task_id: Some("t-1".into()),
                    metadata: None,
                    extensions: None,
                    reference_task_ids: None,
                }),
                TaskVersion(1),
            )
            .await
            .unwrap();

        // Now update status - artifacts and history should be preserved
        store
            .save(
                &Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                    task_id: "t-1".into(),
                    context_id: "ctx-1".into(),
                    status: TaskStatus {
                        state: TaskState::Completed,
                        message: None,
                        timestamp: None,
                    },
                    is_final: Some(true),
                    metadata: None,
                }),
                TaskVersion(2),
            )
            .await
            .unwrap();

        let (t, _) = store.get("t-1").await.unwrap().unwrap();
        assert_eq!(t.status.state, TaskState::Completed);
        assert_eq!(t.artifacts.as_ref().unwrap().len(), 1, "Artifacts should be preserved");
        assert_eq!(t.history.as_ref().unwrap().len(), 1, "History should be preserved");
    }
}
