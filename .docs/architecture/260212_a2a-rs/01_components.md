# a2a-rs Architecture - 컴포넌트 설계

> 상위 문서: [Overview](./00_overview.md)

## Cargo Workspace 구조

```
columbus/
├── Cargo.toml                    # [workspace] 루트
├── crates/
│   ├── a2a-types/                # 코어 도메인 타입 (transport-agnostic)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs            # pub mod 선언 + re-export
│   │       ├── task.rs           # Task, TaskState, TaskStatus, TaskVersion
│   │       ├── message.rs        # Message, Role
│   │       ├── event.rs          # Event enum (Message | Task | StatusUpdate | ArtifactUpdate)
│   │       ├── part.rs           # Part { content: PartContent, metadata, filename, media_type }
│   │       ├── artifact.rs       # Artifact
│   │       ├── agent_card.rs     # AgentCard, AgentSkill, AgentInterface, AgentCapabilities
│   │       ├── auth.rs           # SecurityScheme, OAuth2, APIKey, HTTPAuth, OIDC, MTLS
│   │       ├── error.rs          # A2AError enum (JSON-RPC error codes)
│   │       ├── push.rs           # TaskPushNotificationConfig
│   │       └── jsonrpc.rs        # JSON-RPC 2.0 request/response/error types
│   │
│   ├── a2a-client/               # 클라이언트 SDK
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs         # A2AClient (high-level API)
│   │       ├── transport.rs      # Transport trait 정의
│   │       ├── jsonrpc_transport.rs  # JSON-RPC over HTTP transport impl
│   │       ├── sse.rs            # SSE stream parser (ParseDataStream 상당)
│   │       ├── agent_card_resolver.rs  # Well-known URI resolver
│   │       └── error.rs          # ClientError
│   │
│   └── a2a-server/               # 서버 SDK
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── handler.rs        # RequestHandler trait + DefaultHandler
│           ├── agent_executor.rs # AgentExecutor trait
│           ├── jsonrpc_handler.rs # axum JSON-RPC request router/handler
│           ├── sse_writer.rs     # SSE frame writer (axum Sse compatible)
│           ├── event_queue.rs    # EventQueue trait + InMemoryEventQueue
│           ├── task_store.rs     # TaskStore trait + InMemoryTaskStore
│           ├── agent_card_serve.rs  # AgentCard well-known endpoint serving
│           ├── middleware.rs     # CallInterceptor trait
│           ├── router.rs         # axum Router 구성 (create_router)
│           └── error.rs          # ServerError
│
└── examples/
    └── helloworld/
        ├── server.rs             # 간단한 에코 에이전트 서버
        └── client.rs             # 클라이언트 메시지 전송 예제
```

## 크레이트 의존성 그래프

```
a2a-types (0 internal deps)
    ▲           ▲
    │           │
a2a-server    a2a-client
    ▲           ▲
    │           │
    └─ examples ┘
```

### a2a-types 의존성
```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "2"
```

### a2a-client 의존성
```toml
[dependencies]
a2a-types = { path = "../a2a-types" }
reqwest = { version = "0.12", features = ["json", "stream"] }
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
futures-core = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
thiserror = "2"
bytes = "1"
```

### a2a-server 의존성
```toml
[dependencies]
a2a-types = { path = "../a2a-types" }
axum = { version = "0.8", features = ["json"] }
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
futures-core = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"
thiserror = "2"
bytes = "1"
```

## 컴포넌트 책임

### a2a-types
- **책임**: A2A Protocol v1.0의 모든 데이터 타입 정의, JSON 직렬화/역직렬화
- **원칙**: transport-agnostic, async 런타임 독립, 외부 의존성 최소화
- **핵심 결정**:
  - `serde` derive로 ProtoJSON 호환 직렬화 (SCREAMING_SNAKE_CASE enum)
  - Part는 v1.0 통합 구조 (단일 Part + PartContent enum)
  - Event는 `#[non_exhaustive]` enum으로 sealed 패턴 구현
  - JSON-RPC 2.0 기본 타입도 여기에 포함 (server/client 공용)

### a2a-server
- **책임**: A2A 서버 프레임워크 제공 (axum 기반)
- **원칙**: 개발자는 `AgentExecutor` trait만 구현하면 서버 실행 가능
- **핵심 결정**:
  - `RequestHandler` trait으로 요청 처리 추상화 + DefaultHandler 제공
  - axum Router를 내부에서 구성하여 JSON-RPC 엔드포인트 자동 매핑
  - SSE 스트리밍은 axum의 `Sse` response type 활용
  - EventQueue, TaskStore는 trait으로 추상화 + in-memory 기본 구현

### a2a-client
- **책임**: A2A 서버에 연결하여 메시지 전송, 태스크 관리
- **원칙**: `Transport` trait으로 전송 계층 추상화, 기본 JSON-RPC 구현 제공
- **핵심 결정**:
  - reqwest로 HTTP 요청, SSE 스트림 파싱은 직접 구현
  - `A2AClient` 고수준 API가 Transport를 래핑
  - AgentCard resolver는 well-known URI에서 자동 fetch

## Go → Rust 패턴 매핑

| Go 패턴 | Rust 매핑 | 적용 위치 |
|:--------|:---------|:---------|
| `interface{}` (sealed) | `enum` + `#[non_exhaustive]` | Event, Part, SecurityScheme |
| Functional Options | Builder pattern + `Default` | Handler, Client 생성 |
| `context.Context` | `&self` + cancellation via `tokio::select!` | 모든 async 메서드 |
| `iter.Seq2[T, error]` | `Pin<Box<dyn Stream<Item = Result<T>>>>` | 스트리밍 응답 |
| `error` interface | `thiserror` enum | 모든 에러 타입 |
| `sync.Mutex` | `tokio::sync::RwLock` / `DashMap` | TaskStore, EventQueue |
| goroutine | `tokio::spawn` | 비동기 태스크 실행 |
| channel | `tokio::sync::mpsc` / `broadcast` | EventQueue 내부 |

## 관련 항목
- [데이터 모델](./02_data-model.md)
- [서버 아키텍처](./03_server-architecture.md)
- [클라이언트 아키텍처](./04_client-architecture.md)

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
