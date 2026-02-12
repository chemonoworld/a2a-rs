# a2a-client Crate Implementation

## 개요
- **상위 태스크**: [Overview](./00_overview.md)
- **목적**: reqwest 기반 A2A 클라이언트 SDK. Transport trait 추상화 + JSON-RPC 구현 + SSE 파서.
- **담당**: client-impl (Phase 2 - a2a-types 완료 후)
- **의존**: a2a-types crate

## 목표
- [ ] error.rs - ClientError 타입
- [ ] transport.rs - Transport trait 정의
- [ ] sse.rs - SSE 스트림 파서 (reqwest bytes_stream → Event stream)
- [ ] jsonrpc_transport.rs - JSON-RPC over HTTP Transport 구현
- [ ] agent_card_resolver.rs - Well-known URI에서 AgentCard 가져오기
- [ ] client.rs - A2AClient 고수준 API
- [ ] lib.rs - 모듈 선언 + public API
- [ ] 단위 테스트

## 파일별 구현 상세

### error.rs
```rust
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("JSON-RPC error: code={}, message={}", .0.code, .0.message)]
    JsonRpc(JsonRpcError),

    #[error("Empty result in JSON-RPC response")]
    EmptyResult,

    #[error("No supported interface in AgentCard")]
    NoSupportedInterface,

    #[error("SSE parse error: {0}")]
    SseParse(String),

    #[error("Stream closed unexpectedly")]
    StreamClosed,
}
```

### transport.rs
```rust
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event, ClientError>> + Send>>;

#[async_trait::async_trait]
pub trait Transport: Send + Sync + 'static {
    async fn send_message(&self, req: SendMessageRequest) -> Result<Event, ClientError>;
    async fn send_message_stream(&self, req: SendMessageRequest) -> Result<EventStream, ClientError>;
    async fn get_task(&self, req: GetTaskRequest) -> Result<Task, ClientError>;
    async fn cancel_task(&self, req: CancelTaskRequest) -> Result<Task, ClientError>;
    async fn resubscribe(&self, req: TaskResubscriptionRequest) -> Result<EventStream, ClientError>;
}
```

### sse.rs
```
SSE 파서: reqwest의 bytes_stream → Event stream

핵심 함수:
pub fn parse_sse_stream(
    byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = Result<Event, ClientError>> + Send

내부 로직:
1. Bytes → 라인 분리 (UTF-8, \n 구분)
   - 내부 버퍼로 불완전 라인 처리
   - 최대 버퍼: 10MB

2. 라인 파싱:
   - "data: {json}" → data 수집
   - "data:{json}" → data 수집 (공백 없이도 지원)
   - ": keep-alive" 또는 ":" → 주석, 무시
   - "id: {id}" → 현재 무시 (향후 Last-Event-ID용)
   - "" (빈 줄) → 이벤트 완료, data 처리

3. Data → JSON 역직렬화:
   - JsonRpcResponse 파싱
   - error 있으면 ClientError::JsonRpc
   - result → Event 역직렬화

구현 방식: async_stream::stream! 매크로 또는 수동 Stream impl
  → async_stream 추가 의존성 최소화 위해 수동 구현 선호
  → 대안: futures::stream::unfold 사용

실제 구현:
  struct SseParser {
      buffer: String,
      data_buffer: String,
  }

  impl SseParser {
      fn feed(&mut self, chunk: &[u8]) -> Vec<Result<Event, ClientError>>
      fn process_line(&mut self, line: &str) -> Option<Result<Event, ClientError>>
  }

  → SseParser를 Stream adapter로 래핑
```

### jsonrpc_transport.rs
```
JsonRpcTransport:
  - client: reqwest::Client
  - url: String
  - request_id: AtomicI64

new(url: &str) → Self
  - reqwest::Client::builder().timeout(180s).build()

내부 헬퍼:
  next_id() → i64 (atomic increment)

  send_jsonrpc<T: DeserializeOwned>(method, params) → Result<T>:
    1. JsonRpcRequest 구성 { jsonrpc: "2.0", method, params, id }
    2. POST url, body = json(request)
    3. response.json::<JsonRpcResponse>()
    4. error 있으면 ClientError::JsonRpc
    5. result → serde_json::from_value::<T>

  send_jsonrpc_stream(method, params) → Result<EventStream>:
    1. JsonRpcRequest 구성
    2. POST url, body = json(request)
    3. response.bytes_stream() → parse_sse_stream()

Transport trait 구현:
  send_message → send_jsonrpc::<Event>("message/send", req)
  send_message_stream → send_jsonrpc_stream("message/stream", req)
  get_task → send_jsonrpc::<Task>("tasks/get", req)
  cancel_task → send_jsonrpc::<Task>("tasks/cancel", req)
  resubscribe → send_jsonrpc_stream("tasks/resubscribe", req)
```

### agent_card_resolver.rs
```
AgentCardResolver:
  - client: reqwest::Client

resolve(base_url: &str) → Result<AgentCard, ClientError>:
  1. url = "{base_url}/.well-known/agent-card.json"
  2. GET url
  3. response.json::<AgentCard>()

DefaultResolver: lazy_static 또는 OnceLock으로 싱글톤
```

### client.rs
```
A2AClient:
  - transport: Box<dyn Transport>

생성자:
  from_agent_card(card: &AgentCard) → Result<Self>
    - supported_interfaces에서 JSONRPC 바인딩 찾기
    - JsonRpcTransport::new(interface.url)

  new(url: &str) → Result<Self>
    - JsonRpcTransport::new(url)

  with_transport(transport: impl Transport) → Self
    - 커스텀 Transport 주입

메서드 (Transport 위임):
  send_message(message: Message) → Result<Event>
  send_message_with_config(message: Message, config: MessageSendConfiguration) → Result<Event>
  send_message_stream(message: Message) → Result<EventStream>
  get_task(task_id: &str) → Result<Task>
  cancel_task(task_id: &str) → Result<Task>
  resubscribe(task_id: &str) → Result<EventStream>
```

### lib.rs
```rust
pub mod error;
pub mod transport;
pub mod sse;
pub mod jsonrpc_transport;
pub mod agent_card_resolver;
pub mod client;

pub use error::ClientError;
pub use transport::{Transport, EventStream};
pub use jsonrpc_transport::JsonRpcTransport;
pub use agent_card_resolver::AgentCardResolver;
pub use client::A2AClient;
```

## 테스트 계획
1. **SSE Parser**: 다양한 SSE 포맷 파싱 테스트
   - 정상 이벤트, keep-alive, 빈 줄, 불완전 청크
   - "data: " / "data:" 두 형식
   - 대용량 페이로드 (10MB 경계)
2. **JsonRpcTransport**: mockito 또는 wiremock으로 HTTP 모킹
   - 정상 응답, 에러 응답, 타임아웃
3. **A2AClient**: 통합 수준 (Phase 3에서 실제 서버와)
4. **AgentCardResolver**: mock HTTP로 well-known 응답 파싱

## 고려사항
- SSE 파서는 외부 크레이트(`eventsource-stream`) 대신 직접 구현 (의존성 최소화)
- reqwest의 `stream` feature 필수
- timeout: 스트리밍 요청은 별도 타임아웃 필요 (무한 또는 매우 긴)
  - 동기 요청: 180초
  - 스트리밍: reqwest의 timeout은 전체 응답에 적용되므로, 스트리밍 시 비활성화 필요
    → 별도 Client 인스턴스 또는 per-request timeout override

## 관련 파일
- `.docs/architecture/260212_a2a-rs/04_client-architecture.md`
- `.docs/research/a2a-go-sdk/04_sse-reconnection.md`

---
*작성일: 2026-02-12*
