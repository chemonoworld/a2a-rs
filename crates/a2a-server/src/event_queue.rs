use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use a2a_types::Event;
use async_trait::async_trait;
use futures_core::Stream;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::error::ServerError;

/// Type alias for a stream of events.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event, ServerError>> + Send>>;

/// Writes events into the queue (used by AgentExecutor).
#[async_trait]
pub trait EventQueueWriter: Send + Sync {
    async fn write(&self, event: Event) -> Result<(), ServerError>;
    async fn close(&self) -> Result<(), ServerError>;
}

/// Reads events from the queue via subscription.
pub trait EventQueueReader: Send + Sync {
    fn subscribe(&self) -> EventStream;
}

/// Combined event queue interface.
pub trait EventQueue: EventQueueWriter + EventQueueReader {}

/// Manages event queues keyed by task ID.
pub trait EventQueueManager: Send + Sync + 'static {
    fn get_or_create(&self, task_id: &str) -> Arc<dyn EventQueue>;
    fn get(&self, task_id: &str) -> Option<Arc<dyn EventQueue>>;
    fn destroy(&self, task_id: &str);
}

/// In-memory event queue backed by `tokio::sync::broadcast`.
pub struct InMemoryEventQueue {
    sender: broadcast::Sender<Event>,
}

const DEFAULT_BROADCAST_CAPACITY: usize = 32;

impl InMemoryEventQueue {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(DEFAULT_BROADCAST_CAPACITY);
        Self { sender }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }
}

impl Default for InMemoryEventQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventQueueWriter for InMemoryEventQueue {
    async fn write(&self, event: Event) -> Result<(), ServerError> {
        let _ = self.sender.send(event);
        Ok(())
    }

    async fn close(&self) -> Result<(), ServerError> {
        Ok(())
    }
}

impl EventQueueReader for InMemoryEventQueue {
    fn subscribe(&self) -> EventStream {
        let rx = self.sender.subscribe();
        let stream = BroadcastStream::new(rx).filter_map(|result| match result {
            Ok(event) => Some(Ok(event)),
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(_)) => None,
        });
        Box::pin(stream)
    }
}

impl EventQueue for InMemoryEventQueue {}

/// In-memory event queue manager using `std::sync::RwLock` for fast synchronous access.
pub struct InMemoryEventQueueManager {
    queues: RwLock<HashMap<String, Arc<InMemoryEventQueue>>>,
}

impl InMemoryEventQueueManager {
    pub fn new() -> Self {
        Self {
            queues: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryEventQueueManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EventQueueManager for InMemoryEventQueueManager {
    fn get_or_create(&self, task_id: &str) -> Arc<dyn EventQueue> {
        // Fast path: read lock
        {
            let queues = self.queues.read().expect("RwLock poisoned");
            if let Some(queue) = queues.get(task_id) {
                return queue.clone();
            }
        }
        // Slow path: write lock
        let mut queues = self.queues.write().expect("RwLock poisoned");
        let queue = queues
            .entry(task_id.to_string())
            .or_insert_with(|| Arc::new(InMemoryEventQueue::new()));
        queue.clone()
    }

    fn get(&self, task_id: &str) -> Option<Arc<dyn EventQueue>> {
        let queues = self.queues.read().expect("RwLock poisoned");
        queues
            .get(task_id)
            .map(|q| q.clone() as Arc<dyn EventQueue>)
    }

    fn destroy(&self, task_id: &str) {
        let mut queues = self.queues.write().expect("RwLock poisoned");
        queues.remove(task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2a_types::{Task, TaskState, TaskStatus};
    use tokio_stream::StreamExt;

    fn make_task_event(id: &str) -> Event {
        Event::Task(Task {
            id: id.into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Working,
                message: None,
                timestamp: None,
            },
            artifacts: None,
            history: None,
            metadata: None,
        })
    }

    #[tokio::test]
    async fn test_inmemory_event_queue_write_subscribe() {
        let queue = InMemoryEventQueue::new();
        let mut stream = queue.subscribe();

        queue.write(make_task_event("t-1")).await.unwrap();
        queue.write(make_task_event("t-2")).await.unwrap();

        let event1 = stream.next().await.unwrap().unwrap();
        match event1 {
            Event::Task(t) => assert_eq!(t.id, "t-1"),
            _ => panic!("Expected Task event"),
        }

        let event2 = stream.next().await.unwrap().unwrap();
        match event2 {
            Event::Task(t) => assert_eq!(t.id, "t-2"),
            _ => panic!("Expected Task event"),
        }
    }

    #[tokio::test]
    async fn test_inmemory_event_queue_multiple_subscribers() {
        let queue = InMemoryEventQueue::new();
        let mut stream1 = queue.subscribe();
        let mut stream2 = queue.subscribe();

        queue.write(make_task_event("t-1")).await.unwrap();

        let e1 = stream1.next().await.unwrap().unwrap();
        let e2 = stream2.next().await.unwrap().unwrap();

        match (e1, e2) {
            (Event::Task(t1), Event::Task(t2)) => {
                assert_eq!(t1.id, "t-1");
                assert_eq!(t2.id, "t-1");
            }
            _ => panic!("Expected Task events"),
        }
    }

    #[tokio::test]
    async fn test_inmemory_queue_manager() {
        let mgr = InMemoryEventQueueManager::new();

        let q1 = mgr.get_or_create("task-1");
        let q2 = mgr.get_or_create("task-1");

        // Same queue returned for same task ID (ptr_eq on the underlying data)
        assert!(Arc::ptr_eq(
            &(q1.clone() as Arc<dyn EventQueue>),
            &(q2.clone() as Arc<dyn EventQueue>),
        ));

        assert!(mgr.get("task-1").is_some());
        assert!(mgr.get("task-2").is_none());

        mgr.destroy("task-1");
        assert!(mgr.get("task-1").is_none());
    }
}
