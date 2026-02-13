# a2a-server Crate Implementation

## 개요
- **상위 태스크**: [Overview](./00_overview.md)
- **목적**: axum 기반 A2A 서버 SDK. AgentExecutor trait 구현만으로 완전한 A2A 서버 실행 가능.
- **담당**: server-impl (Phase 2 - a2a-types 완료 후)
- **의존**: a2a-types crate

## 목표
- [ ] error.rs - ServerError 타입
- [ ] agent_executor.rs - AgentExecutor trait 정의
- [ ] event_queue.rs - EventQueue traits + InMemoryEventQueue
- [ ] task_store.rs - TaskStore trait + InMemoryTaskStore
- [ ] handler.rs - RequestHandler trait + DefaultHandler
- [ ] sse_writer.rs - SSE 스트림 생성 (axum Sse 호환)
- [ ] jsonrpc_handler.rs - JSON-RPC 메서드 라우팅 (axum handler)
- [ ] agent_card_serve.rs - AgentCard well-known 엔드포인트
- [ ] middleware.rs - CallInterceptor trait
- [ ] router.rs - axum Router 조립 (create_router)
- [ ] lib.rs - 모듈 선언 + public API re-export
- [ ] 단위 테스트

## 파일별 구현 상세

### error.rs
```rust
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("A2A error: {0}")]
    A2A(A2AError),

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Task not cancelable: {0}")]
    TaskNotCancelable(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Queue closed")]
    QueueClosed,
}

impl ServerError → A2AError 변환 (JSON-RPC 에러 코드 매핑)
```

### agent_executor.rs
```rust
/// 개발자가 구현하는 에이전트 비즈니스 로직
#[async_trait::async_trait]
pub trait AgentExecutor: Send + Sync + 'static {
    async fn execute(
        &self,
        event: Event,
        queue: &dyn EventQueueWriter,
    ) -> Result<(), ServerError>;
}
```
- 간단한 trait, 의존성 없음
- `EventQueueWriter`로 응답 이벤트를 작성

### event_queue.rs
```
Traits:
- EventQueueWriter { write(Event), close() }
- EventQueueReader { subscribe() -> EventStream }
- EventQueue: EventQueueWriter + EventQueueReader
- EventQueueManager { get_or_create(task_id) -> Arc<dyn EventQueue>, get(task_id), destroy(task_id) }

InMemory 구현:
- InMemoryEventQueue
  - tokio::sync::broadcast::Sender<Event> 내부
  - subscribe() → broadcast::Receiver를 Stream으로 래핑
  - write() → sender.send()
  - close() → drop sender 또는 닫힘 마커 이벤트

- InMemoryEventQueueManager
  - RwLock<HashMap<String, Arc<InMemoryEventQueue>>>
  - get_or_create: entry API 사용
  - destroy: remove + 큐 close

EventStream 타입 별칭:
  pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event, ServerError>> + Send>>;
```

### task_store.rs
```
Trait:
- TaskStore { save(Event, TaskVersion), get(task_id), list(context_id?, page_size?, page_token?) }

InMemoryTaskStore:
- RwLock<HashMap<String, (Task, TaskVersion)>>
- save: Event에서 Task 상태 추출/업데이트, 버전 증가, 충돌 검사
- get: HashMap lookup
- list: context_id 필터링, 페이지네이션 (cursor = 마지막 task_id)

save 로직 상세:
  Event::Task(task) → 전체 교체
  Event::Message(msg) → Task에 history 추가
  Event::TaskStatusUpdate(update) → Task.status 업데이트
  Event::TaskArtifactUpdate(update) → Task.artifacts에 추가/수정
```

### handler.rs
```
Trait: RequestHandler
  on_send_message(SendMessageRequest) -> Result<Event>
  on_send_message_stream(SendMessageRequest) -> Result<EventStream>
  on_get_task(GetTaskRequest) -> Result<Task>
  on_cancel_task(CancelTaskRequest) -> Result<Task>
  on_resubscribe(TaskResubscriptionRequest) -> Result<EventStream>

DefaultHandler:
  - executor: Arc<dyn AgentExecutor>
  - task_store: Arc<dyn TaskStore>
  - queue_manager: Arc<dyn EventQueueManager>

  on_send_message:
    1. context_id = message.context_id 또는 Uuid::new_v4()
    2. task_id = Uuid::new_v4()
    3. task = Task { id, context_id, status: Submitted }
    4. task_store.save(Event::Task(task))
    5. queue = queue_manager.get_or_create(task_id)
    6. tokio::spawn(executor.execute(event, queue))
    7. configuration.blocking이면 완료까지 대기
    8. 최종 Event 반환

  on_send_message_stream:
    1-6. 동기와 동일
    7. EventStream 반환:
       - 초기: Event::Task(task) 방출
       - queue.subscribe()로 라이브 이벤트
       - 터미널 상태 도달 시 종료

  on_get_task: task_store.get(id)
  on_cancel_task: task_store에서 조회 + 상태 검증 + Canceled로 업데이트
  on_resubscribe: queue_manager.get(id)로 기존 큐 구독

Builder 패턴:
  DefaultHandler::builder()
    .executor(executor)
    .task_store(store)  // 기본: InMemoryTaskStore
    .queue_manager(mgr) // 기본: InMemoryEventQueueManager
    .build()
```

### sse_writer.rs
```
axum::response::sse::Sse 활용:

fn event_stream_to_sse(stream: EventStream, request_id: JsonRpcId) -> Sse<impl Stream>

각 Event를:
  1. JsonRpcResponse { result: serialize(event), id: request_id } 로 래핑
  2. SseEvent::default().id(uuid).data(json_string) 생성
  3. keep_alive 설정 (기본 15초)
```

### jsonrpc_handler.rs
```
axum handler 함수:

async fn jsonrpc_handler(State(state): State<AppState>, body: String) -> Response
  1. JSON 파싱 → JsonRpcRequest (실패 시 ParseError)
  2. method 라우팅:
     - "message/send" → handle_send_message → Json response
     - "message/stream" → handle_stream → Sse response
     - "tasks/get" → handle_get_task → Json response
     - "tasks/cancel" → handle_cancel_task → Json response
     - "tasks/resubscribe" → handle_resubscribe → Sse response
     - _ → MethodNotFound error
  3. params 파싱 (실패 시 InvalidParams)
  4. handler 호출
  5. 결과를 JsonRpcResponse로 래핑

handle_send_message:
  params → SendMessageRequest (serde_json::from_value)
  handler.on_send_message(params) → Event
  Event → JsonRpcResponse { result: serialize(event) }

handle_stream:
  params → SendMessageRequest
  handler.on_send_message_stream(params) → EventStream
  event_stream_to_sse(stream, request_id)

AppState:
  handler: Arc<dyn RequestHandler>
  agent_card: AgentCard
```

### agent_card_serve.rs
```
async fn serve_agent_card(State(state): State<AppState>) -> Json<AgentCard>

CORS 헤더 설정:
  Access-Control-Allow-Origin: * (또는 요청 Origin 반영)
  Access-Control-Allow-Methods: GET, OPTIONS
  Access-Control-Allow-Headers: Content-Type
```

### middleware.rs
```
#[async_trait]
pub trait CallInterceptor: Send + Sync + 'static {
    async fn before(&self, method: &str, params: &serde_json::Value) -> Result<(), ServerError>;
    async fn after(&self, method: &str, result: &Result<serde_json::Value, ServerError>) -> Result<(), ServerError>;
}

jsonrpc_handler에서 interceptor 호출 (before → handler → after)
```

### router.rs
```rust
pub fn create_router(handler: Arc<dyn RequestHandler>, agent_card: AgentCard) -> Router {
    let state = AppState { handler, agent_card };

    Router::new()
        .route("/", post(jsonrpc_handler))
        .route("/.well-known/agent-card.json", get(serve_agent_card))
        .with_state(state)
}

/// 편의 함수: AgentExecutor로 바로 서버 시작
pub async fn serve(
    executor: impl AgentExecutor,
    agent_card: AgentCard,
    addr: &str,
) -> Result<(), ServerError> {
    let handler = DefaultHandler::builder()
        .executor(Arc::new(executor))
        .build();
    let router = create_router(Arc::new(handler), agent_card);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
```

### lib.rs
```rust
pub mod error;
pub mod agent_executor;
pub mod event_queue;
pub mod task_store;
pub mod handler;
pub mod sse_writer;
pub mod jsonrpc_handler;
pub mod agent_card_serve;
pub mod middleware;
pub mod router;

pub use error::ServerError;
pub use agent_executor::AgentExecutor;
pub use event_queue::*;
pub use task_store::*;
pub use handler::*;
pub use router::{create_router, serve};
```

## 테스트 계획
1. **InMemoryTaskStore**: save/get/list 단위 테스트
2. **InMemoryEventQueue**: write/subscribe 동시성 테스트
3. **DefaultHandler**: 모킹된 executor로 send_message flow
4. **JSON-RPC routing**: 각 method의 올바른 라우팅
5. **SSE**: 이벤트 직렬화 형식 검증

## 고려사항
- `async_trait` 크레이트 사용 (trait에 async fn 지원)
- `axum 0.8`의 State 패턴: `Arc` 래핑 필요
- SSE keep-alive 간격 설정 가능하도록
- EventQueue의 broadcast 채널 버퍼 크기: 기본 32 (Go와 동일)

## 관련 파일
- `.docs/architecture/260212_a2a-rs/03_server-architecture.md`
- `.docs/research/a2a-go-sdk/02_folder-architecture.md`

---
*작성일: 2026-02-12*
