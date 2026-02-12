# a2a-rs Architecture - 서버 아키텍처

> 상위 문서: [Overview](./00_overview.md)

## 개요
axum 기반 A2A 서버 SDK. 개발자는 `AgentExecutor` trait만 구현하면 완전한 A2A 서버를 실행할 수 있다.

## 요청 처리 흐름

```
HTTP Request (POST /)
    │
    ▼
┌────────────────────────────┐
│  axum Router               │
│  - POST /   (JSON-RPC)     │
│  - GET /.well-known/       │
│    agent-card.json          │
└────────────┬───────────────┘
             │
             ▼
┌────────────────────────────┐
│  jsonrpc_handler           │
│  - JSON 파싱               │
│  - method 라우팅            │
│  - message/send            │
│  - message/stream → SSE    │
│  - tasks/get               │
│  - tasks/cancel            │
│  - tasks/resubscribe → SSE │
└────────────┬───────────────┘
             │
             ▼
┌────────────────────────────┐
│  RequestHandler (trait)    │
│  - DefaultHandler (impl)   │
│  - 태스크 생성/조회/취소     │
│  - 이벤트 큐 관리            │
│  - 스트리밍 오케스트레이션    │
└────────────┬───────────────┘
             │
             ▼
┌────────────────────────────┐
│  AgentExecutor (trait)     │
│  - 사용자 비즈니스 로직     │
│  - event 수신 → 처리       │
│  - queue에 응답 이벤트 작성 │
└────────────┬───────────────┘
             │
             ▼
┌────────────────────────────┐
│  EventQueue (trait)        │
│  - InMemoryEventQueue      │
│  - broadcast 패턴          │
│  - tokio::sync::broadcast  │
└────────────┬───────────────┘
             │
             ▼
┌────────────────────────────┐
│  TaskStore (trait)         │
│  - InMemoryTaskStore       │
│  - 태스크 CRUD + 버전 관리  │
└────────────────────────────┘
```

## 핵심 Trait 정의

### AgentExecutor (사용자 구현)

```rust
/// 에이전트 비즈니스 로직의 진입점
/// Go의 AgentExecutor interface에 해당
#[async_trait]
pub trait AgentExecutor: Send + Sync + 'static {
    /// 이벤트(메시지)를 받아 처리하고, 응답을 queue에 작성
    async fn execute(
        &self,
        event: Event,
        queue: &dyn EventQueueWriter,
    ) -> Result<(), ServerError>;
}
```

### RequestHandler

```rust
/// 서버 요청 처리기 trait
/// DefaultHandler가 기본 구현을 제공
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event, ServerError>> + Send>>;

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
```

### TaskStore

```rust
#[async_trait]
pub trait TaskStore: Send + Sync + 'static {
    async fn save(
        &self,
        event: &Event,
        version: TaskVersion,
    ) -> Result<(), ServerError>;

    async fn get(
        &self,
        task_id: &str,
    ) -> Result<Option<(Task, TaskVersion)>, ServerError>;

    async fn list(
        &self,
        context_id: Option<&str>,
        page_size: Option<i32>,
        page_token: Option<&str>,
    ) -> Result<(Vec<Task>, Option<String>), ServerError>;
}
```

### EventQueue

```rust
/// 이벤트 쓰기 (AgentExecutor가 사용)
#[async_trait]
pub trait EventQueueWriter: Send + Sync {
    async fn write(&self, event: Event) -> Result<(), ServerError>;
    async fn close(&self) -> Result<(), ServerError>;
}

/// 이벤트 읽기 (RequestHandler가 사용)
pub trait EventQueueReader: Send + Sync {
    fn subscribe(&self) -> EventStream;
}

/// 이벤트 큐 매니저 (태스크 ID → 큐 매핑)
pub trait EventQueueManager: Send + Sync + 'static {
    fn get_or_create(&self, task_id: &str) -> Arc<dyn EventQueue>;
    fn get(&self, task_id: &str) -> Option<Arc<dyn EventQueue>>;
    fn destroy(&self, task_id: &str);
}

/// 통합 인터페이스 (Reader + Writer)
pub trait EventQueue: EventQueueWriter + EventQueueReader {}
```

## DefaultHandler 동작

### SendMessage (동기)

```
1. message에서 context_id 추출 또는 생성
2. task_id 생성 (UUID v4)
3. Task 생성 (SUBMITTED 상태)
4. TaskStore에 저장
5. EventQueue 생성
6. tokio::spawn으로 AgentExecutor.execute() 실행
7. blocking=true면 executor 완료까지 대기
8. 최종 Task 또는 응답 이벤트 반환
```

### SendMessageStream (스트리밍)

```
1. 동기와 동일한 초기화
2. EventStream 반환:
   a. 초기 Task (SUBMITTED) 방출
   b. EventQueue.subscribe()로 라이브 이벤트 스트림
   c. 각 이벤트를 TaskStore에 저장하면서 스트리밍
   d. 터미널 상태 도달 시 스트림 종료
```

## SSE 스트리밍 구현

### SSE Writer (axum `Sse` 활용)

```rust
/// axum의 Sse response type 활용
use axum::response::sse::{Event as SseEvent, Sse};

pub fn event_stream_to_sse(
    stream: EventStream,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let sse_stream = stream.map(|result| {
        match result {
            Ok(event) => {
                let data = serde_json::to_string(&wrap_jsonrpc_response(&event))
                    .unwrap_or_default();
                let id = Uuid::new_v4().to_string();
                Ok(SseEvent::default().id(id).data(data))
            }
            Err(e) => {
                let error = serde_json::to_string(&wrap_jsonrpc_error(&e))
                    .unwrap_or_default();
                Ok(SseEvent::default().data(error))
            }
        }
    });

    Sse::new(sse_stream)
        .keep_alive(axum::response::sse::KeepAlive::default())
}
```

### SSE 이벤트 형식

```
id: <uuid>\n
data: {"jsonrpc":"2.0","result":{"status":{"state":"TASK_STATE_WORKING"}},"id":1}\n
\n
```

## JSON-RPC Handler (axum)

```rust
/// JSON-RPC 메서드 라우팅
async fn jsonrpc_handler(
    State(handler): State<Arc<dyn RequestHandler>>,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    match request.method.as_str() {
        "message/send" => handle_send_message(handler, request).await,
        "message/stream" => handle_send_message_stream(handler, request).await,
        "tasks/get" => handle_get_task(handler, request).await,
        "tasks/cancel" => handle_cancel_task(handler, request).await,
        "tasks/resubscribe" => handle_resubscribe(handler, request).await,
        _ => method_not_found_response(request.id),
    }
}
```

`message/stream`과 `tasks/resubscribe`는 SSE 응답을 반환하므로 `Sse` response type 사용.

## Router 구성

```rust
/// A2A 서버 Router 생성
pub fn create_router(
    handler: Arc<dyn RequestHandler>,
    agent_card: AgentCard,
) -> Router {
    Router::new()
        .route("/", post(jsonrpc_handler))
        .route(
            "/.well-known/agent-card.json",
            get(serve_agent_card),
        )
        .with_state(AppState { handler, agent_card })
}
```

## InMemory 구현

### InMemoryTaskStore

```rust
pub struct InMemoryTaskStore {
    tasks: RwLock<HashMap<String, (Task, TaskVersion)>>,
}
```
- `RwLock`으로 읽기 동시성 확보
- TaskVersion은 save 시마다 증가

### InMemoryEventQueue

```rust
pub struct InMemoryEventQueue {
    sender: broadcast::Sender<Event>,
}

pub struct InMemoryEventQueueManager {
    queues: RwLock<HashMap<String, Arc<InMemoryEventQueue>>>,
}
```
- `tokio::sync::broadcast` 채널로 pub/sub 구현
- subscribe()는 broadcast::Receiver를 Stream으로 래핑

## Middleware / Interceptor

```rust
/// 요청 전후 훅 (Go의 CallInterceptor에 해당)
#[async_trait]
pub trait CallInterceptor: Send + Sync + 'static {
    async fn before(
        &self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<(), ServerError>;

    async fn after(
        &self,
        method: &str,
        result: &Result<serde_json::Value, ServerError>,
    ) -> Result<(), ServerError>;
}
```

## 관련 항목
- [데이터 모델](./02_data-model.md)
- [클라이언트 아키텍처](./04_client-architecture.md)

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
