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
///
/// The broadcast sender is wrapped in a `RwLock<Option<...>>` so that
/// `close()` can drop the sender, causing all subscriber streams to end.
pub struct InMemoryEventQueue {
    sender: RwLock<Option<broadcast::Sender<Event>>>,
}

const DEFAULT_BROADCAST_CAPACITY: usize = 32;

impl InMemoryEventQueue {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(DEFAULT_BROADCAST_CAPACITY);
        Self {
            sender: RwLock::new(Some(sender)),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender: RwLock::new(Some(sender)),
        }
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
        let sender = self.sender.read().expect("RwLock poisoned");
        if let Some(ref sender) = *sender {
            let _ = sender.send(event);
        }
        Ok(())
    }

    async fn close(&self) -> Result<(), ServerError> {
        let mut sender = self.sender.write().expect("RwLock poisoned");
        *sender = None; // Drop the sender; all receivers will see stream end
        Ok(())
    }
}

impl EventQueueReader for InMemoryEventQueue {
    fn subscribe(&self) -> EventStream {
        let sender = self.sender.read().expect("RwLock poisoned");
        match sender.as_ref() {
            Some(sender) => {
                let rx = sender.subscribe();
                let stream =
                    BroadcastStream::new(rx).filter_map(|result| match result {
                        Ok(event) => Some(Ok(event)),
                        Err(
                            tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(
                                _,
                            ),
                        ) => None,
                    });
                Box::pin(stream)
            }
            None => {
                // Queue already closed, return empty stream
                Box::pin(tokio_stream::empty())
            }
        }
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
    async fn test_inmemory_event_queue_close() {
        let queue = InMemoryEventQueue::new();
        let mut stream = queue.subscribe();

        queue.write(make_task_event("t-1")).await.unwrap();
        queue.close().await.unwrap();

        // Should get the event written before close
        let event = stream.next().await.unwrap().unwrap();
        match event {
            Event::Task(t) => assert_eq!(t.id, "t-1"),
            _ => panic!("Expected Task event"),
        }

        // After close, stream should end (None)
        let next = stream.next().await;
        assert!(next.is_none(), "Stream should end after close");
    }

    #[tokio::test]
    async fn test_inmemory_event_queue_write_after_close() {
        let queue = InMemoryEventQueue::new();
        queue.close().await.unwrap();

        // Writing after close should not panic (just silently drops)
        let result = queue.write(make_task_event("t-1")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_inmemory_event_queue_subscribe_after_close() {
        let queue = InMemoryEventQueue::new();
        queue.close().await.unwrap();

        // Subscribing after close returns empty stream
        let mut stream = queue.subscribe();
        let next = stream.next().await;
        assert!(next.is_none(), "Subscribe after close should return empty stream");
    }

    #[tokio::test]
    async fn test_write_with_no_subscribers() {
        // Writing without any subscribers should succeed silently
        let queue = InMemoryEventQueue::new();
        let result = queue.write(make_task_event("t-1")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_subscriber_dropped_write_still_works() {
        let queue = InMemoryEventQueue::new();
        {
            let _stream = queue.subscribe();
            // stream is dropped here
        }

        // Write after subscriber dropped should succeed
        let result = queue.write(make_task_event("t-1")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_custom_capacity() {
        let queue = InMemoryEventQueue::with_capacity(4);
        let mut stream = queue.subscribe();

        queue.write(make_task_event("t-1")).await.unwrap();
        queue.write(make_task_event("t-2")).await.unwrap();

        let e1 = stream.next().await.unwrap().unwrap();
        let e2 = stream.next().await.unwrap().unwrap();
        match (e1, e2) {
            (Event::Task(t1), Event::Task(t2)) => {
                assert_eq!(t1.id, "t-1");
                assert_eq!(t2.id, "t-2");
            }
            _ => panic!("Expected Task events"),
        }
    }

    #[tokio::test]
    async fn test_queue_manager_multiple_tasks() {
        let mgr = InMemoryEventQueueManager::new();

        let q1 = mgr.get_or_create("task-a");
        let q2 = mgr.get_or_create("task-b");

        // Different task IDs get different queues
        assert!(!Arc::ptr_eq(
            &(q1 as Arc<dyn EventQueue>),
            &(q2 as Arc<dyn EventQueue>),
        ));

        // Both should be retrievable
        assert!(mgr.get("task-a").is_some());
        assert!(mgr.get("task-b").is_some());

        // Destroy one, other remains
        mgr.destroy("task-a");
        assert!(mgr.get("task-a").is_none());
        assert!(mgr.get("task-b").is_some());
    }

    #[tokio::test]
    async fn test_close_idempotent() {
        // Closing an already-closed queue should succeed without panic
        let queue = InMemoryEventQueue::new();
        queue.close().await.unwrap();
        queue.close().await.unwrap();

        // Writing after double-close should also be fine
        let result = queue.write(make_task_event("t-1")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_queue_manager_destroy_nonexistent() {
        // Destroying a nonexistent queue should not panic
        let mgr = InMemoryEventQueueManager::new();
        mgr.destroy("nonexistent-task");
        assert!(mgr.get("nonexistent-task").is_none());
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

    #[tokio::test]
    async fn test_get_or_create_after_destroy_creates_new_queue() {
        let mgr = InMemoryEventQueueManager::new();

        let q1 = mgr.get_or_create("task-1");
        q1.write(make_task_event("t-1")).await.unwrap();
        q1.close().await.unwrap();

        mgr.destroy("task-1");
        assert!(mgr.get("task-1").is_none());

        // get_or_create should create a fresh queue
        let q2 = mgr.get_or_create("task-1");
        assert!(mgr.get("task-1").is_some());

        // The new queue should be functional (not closed)
        let mut stream = q2.subscribe();
        q2.write(make_task_event("t-new")).await.unwrap();
        q2.close().await.unwrap();

        let event = stream.next().await.unwrap().unwrap();
        match event {
            Event::Task(t) => assert_eq!(t.id, "t-new"),
            _ => panic!("Expected Task event"),
        }
    }

    #[test]
    fn test_inmemory_event_queue_default() {
        let q1 = InMemoryEventQueue::new();
        let q2 = InMemoryEventQueue::default();
        // Both should be created successfully (smoke test for Default impl)
        let _ = q1;
        let _ = q2;
    }

    #[test]
    fn test_queue_manager_default() {
        let m1 = InMemoryEventQueueManager::new();
        let m2 = InMemoryEventQueueManager::default();
        // Both should be created successfully (smoke test for Default impl)
        assert!(m1.get("nonexistent").is_none());
        assert!(m2.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_queue_manager_concurrent_get_or_create() {
        use std::sync::Arc;

        let mgr = Arc::new(InMemoryEventQueueManager::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let m = mgr.clone();
            handles.push(tokio::spawn(async move {
                m.get_or_create("same-task")
            }));
        }

        let mut queues: Vec<Arc<dyn EventQueue>> = Vec::new();
        for handle in handles {
            queues.push(handle.await.unwrap());
        }

        // All should return the same queue (Arc pointer equality)
        for q in &queues[1..] {
            assert!(Arc::ptr_eq(&queues[0], q), "All concurrent calls should return same queue");
        }
    }

    #[tokio::test]
    async fn test_event_queue_multiple_writes_after_close() {
        let queue = InMemoryEventQueue::new();
        queue.close().await.unwrap();

        // Multiple writes after close should all succeed silently
        for i in 0..5 {
            let result = queue.write(make_task_event(&format!("t-{i}"))).await;
            assert!(result.is_ok(), "Write {i} after close should succeed");
        }

        // Subscribe after all those writes returns empty stream
        let mut stream = queue.subscribe();
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_queue_manager_independent_queues() {
        let mgr = InMemoryEventQueueManager::new();

        let q1 = mgr.get_or_create("task-a");
        let q2 = mgr.get_or_create("task-b");

        // Write to q1 should not affect q2
        let mut stream_b = q2.subscribe();
        q1.write(make_task_event("t-a")).await.unwrap();
        q1.close().await.unwrap();

        // q2's stream should not receive q1's events
        q2.write(make_task_event("t-b")).await.unwrap();
        q2.close().await.unwrap();

        let event = stream_b.next().await.unwrap().unwrap();
        match event {
            Event::Task(t) => assert_eq!(t.id, "t-b"),
            _ => panic!("Expected task-b event"),
        }

        // Destroying q1 should not affect q2
        mgr.destroy("task-a");
        assert!(mgr.get("task-a").is_none());
        assert!(mgr.get("task-b").is_some());
    }
}
