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
}
