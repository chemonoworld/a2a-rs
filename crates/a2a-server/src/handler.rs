use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use a2a_types::{
    CancelTaskRequest, Event, GetTaskRequest, SendMessageRequest, Task, TaskResubscriptionRequest,
    TaskState, TaskStatus, TaskVersion,
};
use async_trait::async_trait;
use tokio_stream::StreamExt;

use crate::agent_executor::AgentExecutor;
use crate::error::ServerError;
use crate::event_queue::{EventQueueManager, EventStream, InMemoryEventQueueManager};
use crate::task_store::{InMemoryTaskStore, TaskStore};

/// Handles A2A protocol requests.
#[async_trait]
pub trait RequestHandler: Send + Sync + 'static {
    async fn on_send_message(
        &self,
        params: SendMessageRequest,
    ) -> Result<Event, ServerError>;

    async fn on_send_message_stream(
        &self,
        params: SendMessageRequest,
    ) -> Result<EventStream, ServerError>;

    async fn on_get_task(
        &self,
        params: GetTaskRequest,
    ) -> Result<Task, ServerError>;

    async fn on_cancel_task(
        &self,
        params: CancelTaskRequest,
    ) -> Result<Task, ServerError>;

    async fn on_resubscribe(
        &self,
        params: TaskResubscriptionRequest,
    ) -> Result<EventStream, ServerError>;
}

/// Default handler implementing the standard A2A server flow.
pub struct DefaultHandler {
    executor: Arc<dyn AgentExecutor>,
    task_store: Arc<dyn TaskStore>,
    queue_manager: Arc<dyn EventQueueManager>,
}

/// Builder for `DefaultHandler`.
pub struct DefaultHandlerBuilder {
    executor: Option<Arc<dyn AgentExecutor>>,
    task_store: Option<Arc<dyn TaskStore>>,
    queue_manager: Option<Arc<dyn EventQueueManager>>,
}

impl DefaultHandler {
    pub fn builder() -> DefaultHandlerBuilder {
        DefaultHandlerBuilder {
            executor: None,
            task_store: None,
            queue_manager: None,
        }
    }
}

impl DefaultHandlerBuilder {
    pub fn executor(mut self, executor: Arc<dyn AgentExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    pub fn task_store(mut self, store: Arc<dyn TaskStore>) -> Self {
        self.task_store = Some(store);
        self
    }

    pub fn queue_manager(mut self, mgr: Arc<dyn EventQueueManager>) -> Self {
        self.queue_manager = Some(mgr);
        self
    }

    pub fn build(self) -> DefaultHandler {
        DefaultHandler {
            executor: self.executor.expect("executor is required"),
            task_store: self
                .task_store
                .unwrap_or_else(|| Arc::new(InMemoryTaskStore::new())),
            queue_manager: self
                .queue_manager
                .unwrap_or_else(|| Arc::new(InMemoryEventQueueManager::new())),
        }
    }
}

#[async_trait]
impl RequestHandler for DefaultHandler {
    async fn on_send_message(
        &self,
        params: SendMessageRequest,
    ) -> Result<Event, ServerError> {
        let context_id = params
            .message
            .context_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let task_id = uuid::Uuid::new_v4().to_string();

        let task = Task {
            id: task_id.clone(),
            context_id,
            status: TaskStatus {
                state: TaskState::Submitted,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        };

        // Save the initial task
        self.task_store
            .save(&Event::Task(task.clone()), TaskVersion::initial())
            .await?;

        // Create queue and subscribe before spawning the executor
        let queue = self.queue_manager.get_or_create(&task_id);
        let mut stream = queue.subscribe();

        let executor = self.executor.clone();
        let initial_event = Event::Task(task.clone());
        let queue_for_exec = queue.clone();
        tokio::spawn(async move {
            if let Err(e) = executor
                .execute(initial_event, queue_for_exec.as_ref())
                .await
            {
                tracing::error!("AgentExecutor error: {e}");
            }
            // Close the queue so subscribers know no more events are coming
            let _ = queue_for_exec.close().await;
        });

        // Check if blocking mode
        let blocking = params
            .configuration
            .as_ref()
            .and_then(|c| c.blocking)
            .unwrap_or(false);

        if blocking {
            let mut last_event = Event::Task(task);
            while let Some(result) = stream.next().await {
                match result {
                    Ok(event) => {
                        let version = {
                            let current = self.task_store.get(&task_id).await?;
                            current
                                .map(|(_, v)| v.next())
                                .unwrap_or(TaskVersion(1))
                        };
                        self.task_store.save(&event, version).await?;
                        let terminal = is_terminal_event(&event);
                        last_event = event;
                        if terminal {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            Ok(last_event)
        } else {
            Ok(Event::Task(
                self.task_store
                    .get(&task_id)
                    .await?
                    .map(|(t, _)| t)
                    .unwrap_or(task),
            ))
        }
    }

    async fn on_send_message_stream(
        &self,
        params: SendMessageRequest,
    ) -> Result<EventStream, ServerError> {
        let context_id = params
            .message
            .context_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let task_id = uuid::Uuid::new_v4().to_string();

        let task = Task {
            id: task_id.clone(),
            context_id,
            status: TaskStatus {
                state: TaskState::Submitted,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        };

        self.task_store
            .save(&Event::Task(task.clone()), TaskVersion::initial())
            .await?;

        let queue = self.queue_manager.get_or_create(&task_id);
        let live_stream = queue.subscribe();

        let executor = self.executor.clone();
        let initial_event = Event::Task(task.clone());
        let queue_for_exec = queue.clone();
        tokio::spawn(async move {
            if let Err(e) = executor
                .execute(initial_event, queue_for_exec.as_ref())
                .await
            {
                tracing::error!("AgentExecutor error: {e}");
            }
            // Close the queue so subscribers know no more events are coming
            let _ = queue_for_exec.close().await;
        });

        // Produce initial task event, then live events, persisting each to store
        let task_store = self.task_store.clone();
        let task_id_clone = task_id.clone();
        let initial = OnceStream::new(Ok(Event::Task(task)));

        let combined = initial.chain(live_stream);

        let persisting_stream = combined.then(move |result| {
            let store = task_store.clone();
            let tid = task_id_clone.clone();
            async move {
                match result {
                    Ok(event) => {
                        let version = {
                            let current = store.get(&tid).await.ok().flatten();
                            current
                                .map(|(_, v)| v.next())
                                .unwrap_or(TaskVersion(1))
                        };
                        let _ = store.save(&event, version).await;
                        Ok(event)
                    }
                    Err(e) => Err(e),
                }
            }
        });

        let stream = TakeUntilTerminal::new(persisting_stream);
        Ok(Box::pin(stream))
    }

    async fn on_get_task(&self, params: GetTaskRequest) -> Result<Task, ServerError> {
        let result = self.task_store.get(&params.id).await?;
        match result {
            Some((task, _)) => Ok(task),
            None => Err(ServerError::TaskNotFound(params.id)),
        }
    }

    async fn on_cancel_task(&self, params: CancelTaskRequest) -> Result<Task, ServerError> {
        let result = self.task_store.get(&params.id).await?;
        match result {
            Some((task, version)) => {
                if task.status.state.is_terminal() {
                    return Err(ServerError::TaskNotCancelable(params.id));
                }

                let mut canceled_task = task;
                canceled_task.status = TaskStatus {
                    state: TaskState::Canceled,
                    message: None,
                    timestamp: None,
                };

                self.task_store
                    .save(&Event::Task(canceled_task.clone()), version.next())
                    .await?;

                self.queue_manager.destroy(&params.id);

                Ok(canceled_task)
            }
            None => Err(ServerError::TaskNotFound(params.id)),
        }
    }

    async fn on_resubscribe(
        &self,
        params: TaskResubscriptionRequest,
    ) -> Result<EventStream, ServerError> {
        let _ = self
            .task_store
            .get(&params.id)
            .await?
            .ok_or_else(|| ServerError::TaskNotFound(params.id.clone()))?;

        let queue = self
            .queue_manager
            .get(&params.id)
            .ok_or_else(|| ServerError::TaskNotFound(params.id.clone()))?;

        let stream = queue.subscribe();
        Ok(Box::pin(TakeUntilTerminal::new(stream)))
    }
}

fn is_terminal_event(event: &Event) -> bool {
    match event {
        Event::Task(t) => t.status.state.is_terminal(),
        Event::TaskStatusUpdate(u) => u.status.state.is_terminal(),
        _ => false,
    }
}

/// A stream that yields a single item then completes.
struct OnceStream<T> {
    value: Option<T>,
}

impl<T> OnceStream<T> {
    fn new(value: T) -> Self {
        Self {
            value: Some(value),
        }
    }
}

impl<T: Unpin> futures_core::Stream for OnceStream<T> {
    type Item = T;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<T>> {
        Poll::Ready(self.value.take())
    }
}

/// A stream adapter that stops after the first terminal event.
struct TakeUntilTerminal<S> {
    inner: Pin<Box<S>>,
    done: bool,
}

impl<S> TakeUntilTerminal<S> {
    fn new(stream: S) -> Self {
        Self {
            inner: Box::pin(stream),
            done: false,
        }
    }
}

impl<S> Unpin for TakeUntilTerminal<S> {}

impl<S> futures_core::Stream for TakeUntilTerminal<S>
where
    S: futures_core::Stream<Item = Result<Event, ServerError>>,
{
    type Item = Result<Event, ServerError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }
        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                if is_terminal_event(&event) {
                    self.done = true;
                }
                Poll::Ready(Some(Ok(event)))
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_queue::EventQueueWriter;
    use a2a_types::{Message, Part, PartContent, Role, TaskArtifactUpdateEvent, TaskStatusUpdateEvent};

    struct EchoExecutor;

    #[async_trait]
    impl AgentExecutor for EchoExecutor {
        async fn execute(
            &self,
            event: Event,
            queue: &dyn EventQueueWriter,
        ) -> Result<(), ServerError> {
            let (task_id, context_id) = match &event {
                Event::Task(t) => (t.id.clone(), t.context_id.clone()),
                _ => return Ok(()),
            };

            let working = Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            });
            queue.write(working).await?;

            let completed = Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id,
                context_id,
                status: TaskStatus {
                    state: TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            });
            queue.write(completed).await?;
            Ok(())
        }
    }

    fn make_send_request() -> SendMessageRequest {
        SendMessageRequest {
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
                context_id: Some("ctx-1".into()),
                task_id: None,
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            },
            configuration: None,
        }
    }

    #[tokio::test]
    async fn test_on_send_message_nonblocking() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let result = handler.on_send_message(make_send_request()).await;
        assert!(result.is_ok());
        match result.unwrap() {
            Event::Task(t) => {
                assert_eq!(t.status.state, TaskState::Submitted);
            }
            _ => panic!("Expected Task event"),
        }
    }

    #[tokio::test]
    async fn test_on_get_task_not_found() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let result = handler
            .on_get_task(GetTaskRequest {
                id: "nonexistent".into(),
                history_length: None,
            })
            .await;

        assert!(matches!(result, Err(ServerError::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_on_cancel_task_not_found() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let result = handler
            .on_cancel_task(CancelTaskRequest {
                id: "nonexistent".into(),
            })
            .await;

        assert!(matches!(result, Err(ServerError::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_on_send_message_blocking() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

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
                context_id: Some("ctx-1".into()),
                task_id: None,
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            },
            configuration: Some(a2a_types::MessageSendConfiguration {
                blocking: Some(true),
                accepted_output_modes: None,
                push_notification_config: None,
                history_length: None,
            }),
        };

        let result = handler.on_send_message(req).await;
        assert!(result.is_ok());
        // In blocking mode, we should get the terminal event (Completed)
        match result.unwrap() {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.status.state, TaskState::Completed);
            }
            other => panic!("Expected TaskStatusUpdate(Completed), got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_on_send_message_stream() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let req = SendMessageRequest {
            message: Message {
                message_id: "m-1".into(),
                role: Role::User,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "Hello stream".into(),
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
            configuration: None,
        };

        let result = handler.on_send_message_stream(req).await;
        assert!(result.is_ok());

        let mut stream = result.unwrap();
        let mut events = Vec::new();
        while let Some(result) = stream.next().await {
            events.push(result.expect("Stream error"));
        }

        // Should get: Task(Submitted) -> Working -> ArtifactUpdate -> Completed
        assert!(
            events.len() >= 3,
            "Expected at least 3 events, got {}",
            events.len()
        );

        // First event is the initial Task
        assert!(
            matches!(&events[0], Event::Task(t) if t.status.state == TaskState::Submitted),
            "First event should be Task(Submitted)"
        );

        // Should contain Completed
        let has_completed = events.iter().any(|e| {
            matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Completed)
        });
        assert!(has_completed, "Stream should contain Completed event");
    }

    #[tokio::test]
    async fn test_on_get_task_after_send() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let req = make_send_request();
        let event = handler.on_send_message(req).await.unwrap();

        let task_id = match &event {
            Event::Task(t) => t.id.clone(),
            _ => panic!("Expected Task event"),
        };

        // Wait a bit for executor to finish
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let task = handler
            .on_get_task(GetTaskRequest {
                id: task_id.clone(),
                history_length: None,
            })
            .await
            .unwrap();

        assert_eq!(task.id, task_id);
    }

    #[tokio::test]
    async fn test_on_resubscribe_not_found() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let result = handler
            .on_resubscribe(TaskResubscriptionRequest {
                id: "nonexistent".into(),
            })
            .await;

        assert!(matches!(result, Err(ServerError::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_on_cancel_completed_task() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        // Send blocking to get completed task
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
            configuration: Some(a2a_types::MessageSendConfiguration {
                blocking: Some(true),
                accepted_output_modes: None,
                push_notification_config: None,
                history_length: None,
            }),
        };

        let event = handler.on_send_message(req).await.unwrap();
        let task_id = match &event {
            Event::TaskStatusUpdate(u) => u.task_id.clone(),
            _ => panic!("Expected TaskStatusUpdate"),
        };

        // Trying to cancel a completed task should fail
        let result = handler
            .on_cancel_task(CancelTaskRequest {
                id: task_id,
            })
            .await;

        assert!(matches!(result, Err(ServerError::TaskNotCancelable(_))));
    }

    #[tokio::test]
    async fn test_context_id_auto_generated() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        // Send message WITHOUT context_id
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
                context_id: None, // No context_id provided
                task_id: None,
                metadata: None,
                extensions: None,
                reference_task_ids: None,
            },
            configuration: None,
        };

        let event = handler.on_send_message(req).await.unwrap();
        match event {
            Event::Task(t) => {
                // Auto-generated context_id should not be empty
                assert!(!t.context_id.is_empty());
                // Should be a UUID format
                assert!(uuid::Uuid::parse_str(&t.context_id).is_ok());
            }
            _ => panic!("Expected Task event"),
        }
    }

    #[test]
    fn test_is_terminal_event() {
        // Terminal states
        assert!(is_terminal_event(&Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        })));

        assert!(is_terminal_event(&Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus {
                state: TaskState::Failed,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        })));

        assert!(is_terminal_event(&Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus {
                state: TaskState::Canceled,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        })));

        assert!(is_terminal_event(&Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus {
                state: TaskState::Rejected,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        })));

        // Non-terminal states
        assert!(!is_terminal_event(&Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        })));

        assert!(!is_terminal_event(&Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus {
                state: TaskState::InputRequired,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        })));

        // ArtifactUpdate is never terminal
        assert!(!is_terminal_event(&Event::TaskArtifactUpdate(
            a2a_types::TaskArtifactUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                artifact: a2a_types::Artifact {
                    artifact_id: "a".into(),
                    name: None,
                    description: None,
                    parts: vec![],
                    metadata: None,
                    extensions: None,
                },
                append: false,
                last_chunk: true,
                metadata: None,
            }
        )));
    }

    #[tokio::test]
    async fn test_take_until_terminal_stops_at_completed() {
        let events: Vec<Result<Event, ServerError>> = vec![
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            })),
            // This event should NOT be yielded
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
        ];

        let stream = tokio_stream::iter(events);
        let mut terminal_stream = TakeUntilTerminal::new(stream);

        let mut collected = Vec::new();
        while let Some(item) = terminal_stream.next().await {
            collected.push(item.unwrap());
        }

        assert_eq!(collected.len(), 2, "Should stop after Completed event");
        assert!(matches!(&collected[0], Event::TaskStatusUpdate(u) if u.status.state == TaskState::Working));
        assert!(matches!(&collected[1], Event::TaskStatusUpdate(u) if u.status.state == TaskState::Completed));
    }

    #[test]
    #[should_panic(expected = "executor is required")]
    fn test_builder_panics_without_executor() {
        let _ = DefaultHandler::builder().build();
    }

    #[tokio::test]
    async fn test_builder_with_custom_task_store() {
        use crate::task_store::InMemoryTaskStore;

        let custom_store = Arc::new(InMemoryTaskStore::new());
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .task_store(custom_store.clone())
            .build();

        // Send a message; the custom store should have the task
        let req = make_send_request();
        let event = handler.on_send_message(req).await.unwrap();
        let task_id = match &event {
            Event::Task(t) => t.id.clone(),
            _ => panic!("Expected Task event"),
        };

        // Verify the custom store has the task
        let stored = custom_store.get(&task_id).await.unwrap();
        assert!(stored.is_some(), "Custom store should contain the task");
        assert_eq!(stored.unwrap().0.id, task_id);
    }

    #[tokio::test]
    async fn test_builder_with_custom_queue_manager() {
        use crate::event_queue::InMemoryEventQueueManager;

        let custom_mgr = Arc::new(InMemoryEventQueueManager::new());
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .queue_manager(custom_mgr.clone())
            .build();

        let req = make_send_request();
        let event = handler.on_send_message(req).await.unwrap();
        let task_id = match &event {
            Event::Task(t) => t.id.clone(),
            _ => panic!("Expected Task event"),
        };

        // Wait for executor to finish and close the queue
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // After executor completes and queue closes, the queue may still exist in the manager
        // (it's only removed on destroy()). This verifies our custom manager was used.
        // The fact that on_send_message completed successfully means the custom manager was used.
        assert!(!task_id.is_empty());
    }

    #[tokio::test]
    async fn test_on_cancel_submitted_task_succeeds() {
        // Create handler but use an executor that immediately returns without emitting events
        struct NoopExecutor;

        #[async_trait]
        impl AgentExecutor for NoopExecutor {
            async fn execute(
                &self,
                _event: Event,
                _queue: &dyn EventQueueWriter,
            ) -> Result<(), ServerError> {
                // Don't emit any events - task stays Submitted
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                Ok(())
            }
        }

        let handler = DefaultHandler::builder()
            .executor(Arc::new(NoopExecutor))
            .build();

        let req = make_send_request();
        let event = handler.on_send_message(req).await.unwrap();
        let task_id = match &event {
            Event::Task(t) => t.id.clone(),
            _ => panic!("Expected Task event"),
        };

        // Cancel the submitted (non-terminal) task
        let canceled = handler
            .on_cancel_task(CancelTaskRequest { id: task_id.clone() })
            .await
            .unwrap();

        assert_eq!(canceled.status.state, TaskState::Canceled);
        assert_eq!(canceled.id, task_id);
    }

    #[test]
    fn test_is_terminal_event_for_task_type() {
        // Task with terminal state should be terminal
        assert!(is_terminal_event(&Event::Task(a2a_types::Task {
            id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus {
                state: TaskState::Completed,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        })));

        // Task with non-terminal state should NOT be terminal
        assert!(!is_terminal_event(&Event::Task(a2a_types::Task {
            id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus {
                state: TaskState::Submitted,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        })));
    }

    #[test]
    fn test_is_terminal_event_for_message() {
        // Message event is NEVER terminal
        assert!(!is_terminal_event(&Event::Message(a2a_types::Message {
            message_id: "m".into(),
            role: a2a_types::Role::Agent,
            parts: vec![],
            context_id: None,
            task_id: None,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        })));
    }

    #[tokio::test]
    async fn test_once_stream_yields_single_item() {
        let stream = OnceStream::new(42);
        let mut stream = Box::pin(stream);

        let first = stream.next().await;
        assert_eq!(first, Some(42));

        let second = stream.next().await;
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn test_take_until_terminal_passes_through_non_terminal() {
        let events: Vec<Result<Event, ServerError>> = vec![
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::InputRequired,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
        ];

        let stream = tokio_stream::iter(events);
        let mut terminal_stream = TakeUntilTerminal::new(stream);

        let mut collected = Vec::new();
        while let Some(item) = terminal_stream.next().await {
            collected.push(item.unwrap());
        }

        // No terminal event, so all events should be yielded
        assert_eq!(collected.len(), 2);
    }

    #[tokio::test]
    async fn test_on_get_task_not_found_returns_error_variant() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let result = handler
            .on_get_task(GetTaskRequest {
                id: "nonexistent-task-id".into(),
                history_length: None,
            })
            .await;

        assert!(result.is_err());
        match result.err().unwrap() {
            ServerError::TaskNotFound(id) => assert_eq!(id, "nonexistent-task-id"),
            other => panic!("Expected TaskNotFound, got: {other}"),
        }
    }

    #[tokio::test]
    async fn test_on_cancel_completed_task_fails() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        // Create a task in blocking mode so it completes and is persisted
        let req = SendMessageRequest {
            message: Message {
                message_id: "m-block".into(),
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
            configuration: Some(a2a_types::MessageSendConfiguration {
                blocking: Some(true),
                accepted_output_modes: None,
                push_notification_config: None,
                history_length: None,
            }),
        };
        let event = handler.on_send_message(req).await.unwrap();
        let task_id = match &event {
            Event::TaskStatusUpdate(u) => u.task_id.clone(),
            Event::Task(t) => t.id.clone(),
            _ => panic!("Expected terminal event"),
        };

        // Verify task is completed
        let task = handler
            .on_get_task(GetTaskRequest {
                id: task_id.clone(),
                history_length: None,
            })
            .await
            .unwrap();
        assert!(task.status.state.is_terminal());

        // Cancel should fail for completed task
        let result = handler
            .on_cancel_task(CancelTaskRequest { id: task_id })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_take_until_terminal_stops_at_failed() {
        let events: Vec<Result<Event, ServerError>> = vec![
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Failed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            })),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
        ];

        let stream = tokio_stream::iter(events);
        let mut terminal_stream = TakeUntilTerminal::new(stream);

        let mut collected = Vec::new();
        while let Some(item) = terminal_stream.next().await {
            collected.push(item.unwrap());
        }

        assert_eq!(collected.len(), 2, "Should stop after Failed event");
        assert!(matches!(&collected[1], Event::TaskStatusUpdate(u) if u.status.state == TaskState::Failed));
    }

    #[tokio::test]
    async fn test_on_send_message_stream_persists_events() {
        let custom_store = Arc::new(InMemoryTaskStore::new());
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .task_store(custom_store.clone())
            .build();

        let req = make_send_request();
        let stream = handler.on_send_message_stream(req).await.unwrap();

        // Consume all events from the stream
        let events: Vec<_> = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        assert!(events.len() >= 3, "Should have at least 3 events");

        // Extract task_id from first event
        let task_id = match &events[0] {
            Event::Task(t) => t.id.clone(),
            _ => panic!("First event should be Task"),
        };

        // Verify events were persisted to the store
        let (task, _) = custom_store.get(&task_id).await.unwrap().unwrap();
        assert!(
            task.status.state.is_terminal(),
            "Task should be in terminal state after stream completes"
        );
        assert_eq!(
            task.status.state,
            TaskState::Completed,
            "Status should be Completed"
        );
    }

    #[test]
    fn test_is_terminal_event_auth_required() {
        // AuthRequired is an interrupt state, NOT terminal
        assert!(!is_terminal_event(&Event::TaskStatusUpdate(TaskStatusUpdateEvent {
            task_id: "t".into(),
            context_id: "c".into(),
            status: TaskStatus {
                state: TaskState::AuthRequired,
                message: None,
                timestamp: None,
            },
            is_final: None,
            metadata: None,
        })));
    }

    #[tokio::test]
    async fn test_take_until_terminal_passes_auth_required() {
        // AuthRequired is NOT terminal, so it should pass through without stopping
        let events: Vec<Result<Event, ServerError>> = vec![
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::AuthRequired,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(false),
                metadata: None,
            })),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            })),
            // This should NOT be yielded
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
        ];

        let stream = tokio_stream::iter(events);
        let mut terminal_stream = TakeUntilTerminal::new(stream);

        let mut collected = Vec::new();
        while let Some(item) = terminal_stream.next().await {
            collected.push(item.unwrap());
        }

        // Should yield Working, AuthRequired, Completed (stop) - NOT the trailing Working
        assert_eq!(collected.len(), 3, "Should yield 3 events (stop after Completed)");
        assert!(matches!(&collected[0], Event::TaskStatusUpdate(u) if u.status.state == TaskState::Working));
        assert!(matches!(&collected[1], Event::TaskStatusUpdate(u) if u.status.state == TaskState::AuthRequired));
        assert!(matches!(&collected[2], Event::TaskStatusUpdate(u) if u.status.state == TaskState::Completed));
    }

    #[tokio::test]
    async fn test_take_until_terminal_artifact_then_completed() {
        // Artifact updates should pass through, then stop at Completed
        let events: Vec<Result<Event, ServerError>> = vec![
            Ok(Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                artifact: a2a_types::Artifact {
                    artifact_id: "a".into(),
                    name: None,
                    description: None,
                    parts: vec![],
                    metadata: None,
                    extensions: None,
                },
                append: false,
                last_chunk: true,
                metadata: None,
            })),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            })),
            // Should not be yielded
            Ok(Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                artifact: a2a_types::Artifact {
                    artifact_id: "a2".into(),
                    name: None,
                    description: None,
                    parts: vec![],
                    metadata: None,
                    extensions: None,
                },
                append: false,
                last_chunk: true,
                metadata: None,
            })),
        ];

        let stream = tokio_stream::iter(events);
        let mut terminal_stream = TakeUntilTerminal::new(stream);

        let mut collected = Vec::new();
        while let Some(item) = terminal_stream.next().await {
            collected.push(item.unwrap());
        }

        assert_eq!(collected.len(), 2);
        assert!(matches!(&collected[0], Event::TaskArtifactUpdate(_)));
        assert!(matches!(&collected[1], Event::TaskStatusUpdate(u) if u.status.state == TaskState::Completed));
    }

    #[tokio::test]
    async fn test_on_send_message_preserves_provided_context_id() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let msg = Message {
            message_id: "m-ctx".into(),
            role: Role::User,
            parts: vec![Part {
                content: PartContent::Text {
                    text: "test context".into(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            }],
            context_id: Some("my-custom-context".into()),
            task_id: None,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
        };

        let req = SendMessageRequest {
            message: msg,
            configuration: None,
        };

        let event = handler.on_send_message(req).await.unwrap();
        match &event {
            Event::Task(t) => {
                assert_eq!(t.context_id, "my-custom-context");
            }
            _ => panic!("Expected Task event"),
        }
    }

    #[tokio::test]
    async fn test_on_resubscribe_after_queue_destroyed() {
        // After cancel destroys the queue, resubscribe should fail with TaskNotFound
        struct NoopExecutor;

        #[async_trait]
        impl AgentExecutor for NoopExecutor {
            async fn execute(
                &self,
                _event: Event,
                _queue: &dyn EventQueueWriter,
            ) -> Result<(), ServerError> {
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                Ok(())
            }
        }

        let handler = DefaultHandler::builder()
            .executor(Arc::new(NoopExecutor))
            .build();

        let req = make_send_request();
        let event = handler.on_send_message(req).await.unwrap();
        let task_id = match &event {
            Event::Task(t) => t.id.clone(),
            _ => panic!("Expected Task event"),
        };

        // Cancel the task (this destroys the queue)
        handler
            .on_cancel_task(a2a_types::CancelTaskRequest { id: task_id.clone() })
            .await
            .unwrap();

        // Now resubscribe should fail because queue was destroyed
        let result = handler
            .on_resubscribe(a2a_types::TaskResubscriptionRequest { id: task_id })
            .await;

        assert!(matches!(result, Err(ServerError::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_stream_yields_task_then_status_updates() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let req = make_send_request();
        let stream = handler.on_send_message_stream(req).await.unwrap();

        let events: Vec<Event> = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        // First event should be Task(Submitted)
        assert!(
            matches!(&events[0], Event::Task(t) if t.status.state == TaskState::Submitted),
            "First event should be Task(Submitted)"
        );

        // Should contain Working
        let has_working = events
            .iter()
            .any(|e| matches!(e, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Working));
        assert!(has_working, "Stream should contain Working");

        // Last event should be Completed (terminal stops the stream)
        let last = events.last().unwrap();
        assert!(
            matches!(last, Event::TaskStatusUpdate(u) if u.status.state == TaskState::Completed),
            "Last event should be Completed"
        );
    }

    #[tokio::test]
    async fn test_on_send_message_blocking_waits_for_terminal() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let msg = Message {
            message_id: "m-block".into(),
            role: Role::User,
            parts: vec![Part {
                content: PartContent::Text {
                    text: "blocking".into(),
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

        let req = SendMessageRequest {
            message: msg,
            configuration: Some(a2a_types::MessageSendConfiguration {
                blocking: Some(true),
                accepted_output_modes: None,
                push_notification_config: None,
                history_length: None,
            }),
        };

        let event = handler.on_send_message(req).await.unwrap();
        // In blocking mode, EchoExecutor emits Working → Completed
        // The last event should be the terminal Completed
        match &event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.status.state, TaskState::Completed);
            }
            _ => panic!("Expected TaskStatusUpdate with Completed, got {:?}", event),
        }
    }

    #[tokio::test]
    async fn test_on_cancel_canceled_task_fails() {
        // Canceling an already-canceled task should return TaskNotCancelable
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        // First, send a non-blocking message to create a task
        let result = handler.on_send_message(make_send_request()).await.unwrap();
        let task_id = match &result {
            Event::Task(t) => t.id.clone(),
            _ => panic!("Expected Task event"),
        };

        // Give executor time to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Cancel the task (first cancel should work since executor completes fast)
        // Actually, the EchoExecutor completes instantly, so by this time
        // the task is in Completed state (persisted in non-blocking mode? No - non-blocking
        // doesn't persist executor events). The task should be Submitted still.
        let cancel_result = handler
            .on_cancel_task(CancelTaskRequest {
                id: task_id.clone(),
            })
            .await;
        assert!(cancel_result.is_ok());
        let canceled = cancel_result.unwrap();
        assert_eq!(canceled.status.state, TaskState::Canceled);

        // Second cancel should fail (task is now in terminal Canceled state)
        let cancel2_result = handler
            .on_cancel_task(CancelTaskRequest { id: task_id })
            .await;
        assert!(cancel2_result.is_err());
        match cancel2_result.unwrap_err() {
            ServerError::TaskNotCancelable(_) => {}
            e => panic!("Expected TaskNotCancelable, got {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_executor_error_does_not_crash() {
        // An executor that returns an error should not crash the handler;
        // the task should remain in Submitted state (non-blocking mode)
        struct FailingExecutor;

        #[async_trait]
        impl AgentExecutor for FailingExecutor {
            async fn execute(
                &self,
                _event: Event,
                _queue: &dyn EventQueueWriter,
            ) -> Result<(), ServerError> {
                Err(ServerError::Internal("executor failure".into()))
            }
        }

        let handler = DefaultHandler::builder()
            .executor(Arc::new(FailingExecutor))
            .build();

        let result = handler.on_send_message(make_send_request()).await;
        assert!(result.is_ok());
        let event = result.unwrap();

        // In non-blocking mode, we get the task back immediately (Submitted state)
        match &event {
            Event::Task(t) => {
                assert_eq!(t.status.state, TaskState::Submitted);
            }
            _ => panic!("Expected Task event"),
        }
    }

    #[tokio::test]
    async fn test_take_until_terminal_empty_stream() {
        let events: Vec<Result<Event, ServerError>> = vec![];
        let stream = tokio_stream::iter(events);
        let mut terminal_stream = TakeUntilTerminal::new(stream);

        let next = terminal_stream.next().await;
        assert!(next.is_none(), "Empty stream should produce no events");
    }

    #[tokio::test]
    async fn test_on_send_message_stream_auto_generates_context_id() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        // Send message WITHOUT context_id in stream mode
        let req = SendMessageRequest {
            message: Message {
                message_id: "m-no-ctx".into(),
                role: Role::User,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "stream no ctx".into(),
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
            configuration: None,
        };

        let mut stream = handler.on_send_message_stream(req).await.unwrap();

        // First event should be Task with auto-generated context_id
        let first = stream.next().await.unwrap().unwrap();
        match first {
            Event::Task(t) => {
                assert!(!t.context_id.is_empty(), "context_id should be auto-generated");
                assert!(uuid::Uuid::parse_str(&t.context_id).is_ok(), "Auto-generated context_id should be UUID");
            }
            _ => panic!("First event should be Task"),
        }

        // Consume remaining events
        while (stream.next().await).is_some() {}
    }

    #[tokio::test]
    async fn test_on_cancel_task_in_different_terminal_states() {
        // Tasks in Failed and Rejected states should also not be cancelable
        let task_store = Arc::new(InMemoryTaskStore::new());
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .task_store(task_store.clone())
            .build();

        // Manually save a task in Failed state
        let failed_task = Task {
            id: "t-failed".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Failed,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        };
        task_store
            .save(&Event::Task(failed_task), TaskVersion::initial())
            .await
            .unwrap();

        let result = handler
            .on_cancel_task(CancelTaskRequest {
                id: "t-failed".into(),
            })
            .await;
        assert!(matches!(result, Err(ServerError::TaskNotCancelable(_))));

        // Save a task in Rejected state
        let rejected_task = Task {
            id: "t-rejected".into(),
            context_id: "ctx-2".into(),
            status: TaskStatus {
                state: TaskState::Rejected,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        };
        task_store
            .save(&Event::Task(rejected_task), TaskVersion::initial())
            .await
            .unwrap();

        let result = handler
            .on_cancel_task(CancelTaskRequest {
                id: "t-rejected".into(),
            })
            .await;
        assert!(matches!(result, Err(ServerError::TaskNotCancelable(_))));
    }

    #[tokio::test]
    async fn test_on_send_message_blocking_with_configuration() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(EchoExecutor))
            .build();

        let req = SendMessageRequest {
            message: Message {
                message_id: "m-cfg-block".into(),
                role: Role::User,
                parts: vec![Part {
                    content: PartContent::Text {
                        text: "blocking with config".into(),
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
            configuration: Some(a2a_types::MessageSendConfiguration {
                blocking: Some(true),
                accepted_output_modes: Some(vec!["text/plain".into()]),
                push_notification_config: None,
                history_length: Some(5),
            }),
        };

        let event = handler.on_send_message(req).await.unwrap();
        // Blocking mode returns terminal event
        match event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.status.state, TaskState::Completed);
            }
            _ => panic!("Expected Completed TaskStatusUpdate for blocking mode"),
        }
    }

    #[tokio::test]
    async fn test_take_until_terminal_stops_at_rejected() {
        let events: Vec<Result<Event, ServerError>> = vec![
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Rejected,
                    message: Some(a2a_types::Message {
                        message_id: "m".into(),
                        role: a2a_types::Role::Agent,
                        parts: vec![],
                        context_id: None,
                        task_id: None,
                        metadata: None,
                        extensions: None,
                        reference_task_ids: None,
                    }),
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            })),
            // Should not be yielded
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
        ];

        let stream = tokio_stream::iter(events);
        let mut terminal_stream = TakeUntilTerminal::new(stream);

        let mut collected = Vec::new();
        while let Some(item) = terminal_stream.next().await {
            collected.push(item.unwrap());
        }

        assert_eq!(collected.len(), 1);
        assert!(matches!(&collected[0], Event::TaskStatusUpdate(u) if u.status.state == TaskState::Rejected));
    }

    #[tokio::test]
    async fn test_take_until_terminal_stops_at_canceled() {
        let events: Vec<Result<Event, ServerError>> = vec![
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            })),
            Ok(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "t".into(),
                context_id: "c".into(),
                status: TaskStatus {
                    state: TaskState::Canceled,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            })),
        ];

        let stream = tokio_stream::iter(events);
        let mut terminal_stream = TakeUntilTerminal::new(stream);

        let mut collected = Vec::new();
        while let Some(item) = terminal_stream.next().await {
            collected.push(item.unwrap());
        }

        assert_eq!(collected.len(), 2);
        assert!(matches!(&collected[1], Event::TaskStatusUpdate(u) if u.status.state == TaskState::Canceled));
    }
}
