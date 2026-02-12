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
    use a2a_types::{Message, Part, PartContent, Role, TaskStatusUpdateEvent};

    struct EchoExecutor;

    #[async_trait]
    impl AgentExecutor for EchoExecutor {
        async fn execute(
            &self,
            _event: Event,
            queue: &dyn EventQueueWriter,
        ) -> Result<(), ServerError> {
            let working = Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: "ignored".into(),
                context_id: "ignored".into(),
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
                task_id: "ignored".into(),
                context_id: "ignored".into(),
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
}
