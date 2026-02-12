# a2a-rs Implementation Plan

## Overview
a2a-go SDK를 Rust로 포팅하는 프로젝트. A2A Protocol v1.0 스펙을 직접 타겟팅한다.

## Tech Stack

| Component | Choice | Reason |
|:----------|:-------|:-------|
| Async Runtime | tokio | 사실상 표준 |
| HTTP Framework | axum | tower 기반, 현대적 |
| HTTP Client | reqwest | tokio 네이티브 |
| Serialization | serde + serde_json | 사실상 표준 |
| UUID | uuid | 가장 널리 사용 |
| Logging | tracing | 구조화 로깅 |
| Error Handling | thiserror | 라이브러리용 에러 타입 |
| gRPC | tonic | 추후 추가 (Phase 2) |

## Crate Structure (Cargo Workspace)

```
louisville/
├── Cargo.toml                 # Workspace root
├── crates/
│   ├── a2a-types/             # Core domain types
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── task.rs        # Task, TaskState, TaskStatus
│   │       ├── message.rs     # Message, Role
│   │       ├── event.rs       # Event enum (sealed)
│   │       ├── part.rs        # Part (v1.0 unified)
│   │       ├── artifact.rs    # Artifact
│   │       ├── agent_card.rs  # AgentCard, AgentSkill, AgentInterface
│   │       ├── auth.rs        # SecurityScheme, OAuth2 등
│   │       ├── error.rs       # A2A error codes
│   │       └── push.rs        # PushNotificationConfig
│   │
│   ├── a2a-server/            # Server SDK
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── handler.rs     # RequestHandler trait + default impl
│   │       ├── agent_executor.rs  # AgentExecutor trait
│   │       ├── jsonrpc/
│   │       │   ├── mod.rs
│   │       │   ├── handler.rs # axum JSON-RPC handler
│   │       │   ├── types.rs   # JSON-RPC request/response types
│   │       │   └── router.rs  # axum Router setup
│   │       ├── sse/
│   │       │   ├── mod.rs
│   │       │   └── writer.rs  # SSE stream writer
│   │       ├── event_queue/
│   │       │   ├── mod.rs
│   │       │   ├── queue.rs   # Queue trait + in-memory impl
│   │       │   └── manager.rs # Manager trait + in-memory impl
│   │       ├── task_store/
│   │       │   ├── mod.rs
│   │       │   └── memory.rs  # In-memory TaskStore
│   │       ├── agent_card_server.rs  # AgentCard serving
│   │       └── middleware.rs  # CallInterceptor
│   │
│   ├── a2a-client/            # Client SDK
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs      # A2AClient (high-level)
│   │       ├── transport.rs   # Transport trait
│   │       ├── jsonrpc.rs     # JSON-RPC transport impl
│   │       ├── sse.rs         # SSE stream parser
│   │       ├── agent_card_resolver.rs  # AgentCard resolver
│   │       └── error.rs       # Client errors
│   │
│   └── a2a-macros/            # (Optional) Derive macros
│
└── examples/
    └── helloworld/
        ├── server.rs
        └── client.rs
```

## Core Types Mapping (Go → Rust)

### Event (Sealed enum)
```rust
// Go: Event interface with isEvent() marker
// Rust: enum (natural fit)
pub enum Event {
    Message(Message),
    Task(Task),
    TaskStatusUpdate(TaskStatusUpdateEvent),
    TaskArtifactUpdate(TaskArtifactUpdateEvent),
}
```

### Part (v1.0 unified, oneof)
```rust
// Go: Part interface with TextPart/FilePart/DataPart
// Rust v1.0: single Part with content enum
pub struct Part {
    pub content: PartContent,
    pub metadata: Option<serde_json::Value>,
    pub filename: Option<String>,
    pub media_type: Option<String>,
}

pub enum PartContent {
    Text(String),
    Raw(Vec<u8>),         // base64 in JSON
    Url(String),
    Data(serde_json::Value),
}
```

### TaskState (Enum with ProtoJSON serialization)
```rust
#[derive(Serialize, Deserialize)]
pub enum TaskState {
    #[serde(rename = "TASK_STATE_SUBMITTED")]
    Submitted,
    #[serde(rename = "TASK_STATE_WORKING")]
    Working,
    #[serde(rename = "TASK_STATE_COMPLETED")]
    Completed,
    // ... terminal + interrupt states
}
```

### Traits (Go interfaces → Rust traits)
```rust
// AgentExecutor
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute(&self, event: Event, queue: &dyn EventQueueWriter) -> Result<()>;
}

// RequestHandler
#[async_trait]
pub trait RequestHandler: Send + Sync {
    async fn on_send_message(&self, params: MessageSendParams) -> Result<Event>;
    fn on_send_message_stream(&self, params: MessageSendParams) -> EventStream;
    async fn on_get_task(&self, params: TaskQueryParams) -> Result<Task>;
    async fn on_cancel_task(&self, params: TaskIdParams) -> Result<Task>;
    // ...
}

// TaskStore
#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn save(&self, event: &Event, version: TaskVersion) -> Result<()>;
    async fn get(&self, task_id: &str) -> Result<(Task, TaskVersion)>;
    async fn list(&self, params: &ListParams) -> Result<(Vec<Task>, Option<String>)>;
}

// Transport (Client)
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send_message(&self, req: SendMessageRequest) -> Result<Event>;
    fn send_message_stream(&self, req: SendMessageRequest) -> EventStream;
    async fn get_task(&self, req: GetTaskRequest) -> Result<Task>;
    async fn cancel_task(&self, req: CancelTaskRequest) -> Result<Task>;
    // ...
}
```

## Implementation Phases

### Phase 1: Foundation (coordinator)
- [x] Plan document
- [ ] Cargo workspace setup
- [ ] a2a-types crate: all core domain types with serde

### Phase 2: Parallel Implementation (team)
- [ ] **server-agent**: a2a-server crate
  - Handler trait + default impl
  - AgentExecutor trait
  - JSON-RPC handler (axum)
  - SSE streaming
  - Event queue (in-memory)
  - Task store (in-memory)
  - AgentCard serving
- [ ] **client-agent**: a2a-client crate
  - Transport trait
  - JSON-RPC transport (reqwest)
  - SSE parser
  - AgentCard resolver
  - High-level Client

### Phase 3: Integration
- [ ] Hello world example (server + client)
- [ ] Integration tests
- [ ] cargo check / clippy

## Scope (v0.1.0)

### In Scope
- Core types (v1.0 spec)
- Server SDK (JSON-RPC + SSE)
- Client SDK (JSON-RPC + SSE)
- In-memory task store & event queue
- AgentCard serving & resolving
- Hello world example

### Out of Scope (future)
- gRPC transport (tonic)
- Distributed/cluster mode
- Push notifications (webhook)
- OpenTelemetry extensions
- mTLS transport
- OAuth implementation details
- Agent Card JWS signatures
- Multi-tenancy URL routing

## JSON-RPC Method Mapping

| Method | Endpoint | Handler |
|:-------|:---------|:--------|
| message/send | POST / | on_send_message |
| message/stream | POST / (SSE) | on_send_message_stream |
| tasks/get | POST / | on_get_task |
| tasks/cancel | POST / | on_cancel_task |
| tasks/resubscribe | POST / (SSE) | on_resubscribe |

## Error Code Mapping

| Code | Name | Description |
|:-----|:-----|:------------|
| -32700 | ParseError | Invalid JSON |
| -32600 | InvalidRequest | Invalid JSON-RPC |
| -32601 | MethodNotFound | Unknown method |
| -32602 | InvalidParams | Invalid parameters |
| -32603 | InternalError | Server error |
| -32001 | TaskNotFound | Task not found |
| -32002 | TaskNotCancelable | Task in terminal state |

---
*Created: 2026-02-12*
