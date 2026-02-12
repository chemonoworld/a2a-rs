# a2a-rs Architecture - 클라이언트 아키텍처

> 상위 문서: [Overview](./00_overview.md)

## 개요
reqwest 기반 A2A 클라이언트 SDK. AgentCard에서 자동으로 클라이언트를 생성하거나, 직접 URL을 지정하여 A2A 서버와 통신한다.

## 클라이언트 흐름

```
┌────────────────────────────────────┐
│          A2AClient (high-level)    │
│                                    │
│  send_message()                    │
│  send_message_stream() → Stream    │
│  get_task()                        │
│  cancel_task()                     │
│  resubscribe() → Stream            │
└───────────────┬────────────────────┘
                │
                ▼
┌────────────────────────────────────┐
│      Transport (trait)             │
│                                    │
│  ┌──────────────────────────────┐  │
│  │ JsonRpcTransport (impl)     │  │
│  │ - reqwest::Client           │  │
│  │ - POST + JSON body          │  │
│  │ - SSE response → Stream     │  │
│  └──────────────────────────────┘  │
└───────────────┬────────────────────┘
                │
                ▼
┌────────────────────────────────────┐
│      SSE Parser                    │
│                                    │
│  - reqwest response body stream    │
│  - 라인 파싱 (data: prefix)        │
│  - JSON 역직렬화 → Event           │
└────────────────────────────────────┘
```

## 핵심 타입/Trait 정의

### Transport Trait

```rust
pub type EventStream = Pin<Box<dyn Stream<Item = Result<Event, ClientError>> + Send>>;

/// 전송 계층 추상화
/// 기본 구현: JsonRpcTransport (reqwest)
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    async fn send_message(
        &self,
        request: SendMessageRequest,
    ) -> Result<Event, ClientError>;

    async fn send_message_stream(
        &self,
        request: SendMessageRequest,
    ) -> Result<EventStream, ClientError>;

    async fn get_task(
        &self,
        request: GetTaskRequest,
    ) -> Result<Task, ClientError>;

    async fn cancel_task(
        &self,
        request: CancelTaskRequest,
    ) -> Result<Task, ClientError>;

    async fn resubscribe(
        &self,
        request: TaskResubscriptionRequest,
    ) -> Result<EventStream, ClientError>;
}
```

### A2AClient (High-level API)

```rust
/// 고수준 A2A 클라이언트
/// Transport를 래핑하고 편의 메서드 제공
pub struct A2AClient {
    transport: Box<dyn Transport>,
}

impl A2AClient {
    /// AgentCard에서 자동 생성
    pub async fn from_agent_card(card: &AgentCard) -> Result<Self, ClientError> {
        let interface = card.supported_interfaces
            .iter()
            .find(|i| i.protocol_binding == ProtocolBinding::JsonRpc)
            .ok_or(ClientError::NoSupportedInterface)?;

        let transport = JsonRpcTransport::new(&interface.url)?;
        Ok(Self { transport: Box::new(transport) })
    }

    /// URL 직접 지정
    pub fn new(url: &str) -> Result<Self, ClientError> {
        let transport = JsonRpcTransport::new(url)?;
        Ok(Self { transport: Box::new(transport) })
    }

    pub async fn send_message(
        &self,
        message: Message,
    ) -> Result<Event, ClientError> {
        let request = SendMessageRequest {
            message,
            configuration: None,
        };
        self.transport.send_message(request).await
    }

    pub async fn send_message_stream(
        &self,
        message: Message,
    ) -> Result<EventStream, ClientError> {
        let request = SendMessageRequest {
            message,
            configuration: None,
        };
        self.transport.send_message_stream(request).await
    }

    pub async fn get_task(
        &self,
        task_id: &str,
    ) -> Result<Task, ClientError> {
        self.transport.get_task(GetTaskRequest {
            id: task_id.to_string(),
            history_length: None,
        }).await
    }

    pub async fn cancel_task(
        &self,
        task_id: &str,
    ) -> Result<Task, ClientError> {
        self.transport.cancel_task(CancelTaskRequest {
            id: task_id.to_string(),
        }).await
    }
}
```

### JsonRpcTransport 구현

```rust
pub struct JsonRpcTransport {
    client: reqwest::Client,
    url: String,
    request_id: AtomicI64,
}

impl JsonRpcTransport {
    pub fn new(url: &str) -> Result<Self, ClientError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))  // 3분 (Go SDK와 동일)
            .build()?;

        Ok(Self {
            client,
            url: url.to_string(),
            request_id: AtomicI64::new(1),
        })
    }

    fn next_id(&self) -> i64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// JSON-RPC 요청 전송 (동기 응답)
    async fn send_jsonrpc<T: DeserializeOwned>(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> Result<T, ClientError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: Some(serde_json::to_value(params)?),
            id: JsonRpcId::Number(self.next_id()),
        };

        let response = self.client
            .post(&self.url)
            .json(&request)
            .send()
            .await?;

        let jsonrpc_resp: JsonRpcResponse = response.json().await?;

        if let Some(error) = jsonrpc_resp.error {
            return Err(ClientError::JsonRpc(error));
        }

        let result = jsonrpc_resp.result
            .ok_or(ClientError::EmptyResult)?;
        serde_json::from_value(result).map_err(Into::into)
    }

    /// JSON-RPC 요청 전송 (SSE 스트리밍 응답)
    async fn send_jsonrpc_stream(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> Result<EventStream, ClientError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: Some(serde_json::to_value(params)?),
            id: JsonRpcId::Number(self.next_id()),
        };

        let response = self.client
            .post(&self.url)
            .json(&request)
            .send()
            .await?;

        // SSE 스트림 파싱
        let stream = parse_sse_stream(response.bytes_stream());
        Ok(Box::pin(stream))
    }
}
```

## SSE Parser

```rust
/// SSE 스트림 파싱
/// Go의 ParseDataStream에 해당
pub fn parse_sse_stream(
    body: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> impl Stream<Item = Result<Event, ClientError>> + Send {
    // 1. byte stream → line stream
    // 2. 라인 파싱:
    //    - "data: {json}" → JSON 추출
    //    - "data:{json}" → JSON 추출 (공백 없이)
    //    - ": keep-alive" → 무시 (주석)
    //    - "id: xxx" → 무시 (현재 미사용)
    //    - 빈 줄 → 이벤트 경계
    // 3. JSON → JsonRpcResponse 역직렬화
    // 4. result → Event 역직렬화
    //
    // 구현: StreamExt::filter_map 기반
    //
    // 최대 버퍼: 10MB (Go SDK와 동일)
}
```

### SSE 파싱 상세 로직

```
입력 바이트 스트림
    │
    ▼
┌─────────────────┐
│ Line Decoder    │ bytes → UTF-8 lines (\n 구분)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ SSE Parser      │
│ - "data: " →    │ 데이터 라인 수집
│ - ": " →        │ 주석 무시
│ - "id: " →      │ ID 무시 (향후 사용)
│ - "" (빈줄) →   │ 이벤트 완료
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ JSON Decode     │ data → JsonRpcResponse → Event
└────────┬────────┘
         │
         ▼
    Result<Event, ClientError>
```

## AgentCard Resolver

```rust
/// Well-known URI에서 AgentCard 가져오기
pub struct AgentCardResolver {
    client: reqwest::Client,
}

impl AgentCardResolver {
    pub async fn resolve(
        &self,
        base_url: &str,
    ) -> Result<AgentCard, ClientError> {
        let url = format!("{}/.well-known/agent-card.json", base_url.trim_end_matches('/'));
        let response = self.client.get(&url).send().await?;
        let card: AgentCard = response.json().await?;
        Ok(card)
    }
}
```

## Error Types

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

    #[error("No supported interface found in AgentCard")]
    NoSupportedInterface,

    #[error("SSE parse error: {0}")]
    SseParse(String),

    #[error("Stream closed")]
    StreamClosed,
}
```

## 관련 항목
- [데이터 모델](./02_data-model.md)
- [서버 아키텍처](./03_server-architecture.md)

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
