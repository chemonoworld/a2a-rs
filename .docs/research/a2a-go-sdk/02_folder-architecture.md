# a2a-go SDK - 폴더 구조 및 아키텍처

> 상위 문서: [a2a-go SDK Overview](./00_overview.md)

## 개요
a2a-go 프로젝트의 디렉토리 구조, 패키지 역할, 핵심 인터페이스, 설계 패턴을 정리한 문서.

## 전체 디렉토리 트리

```
a2a-go/
├── a2a/                        # 코어 도메인 타입 (transport-agnostic)
│   ├── agent.go                #   AgentCard, AgentSkill, SecurityScheme
│   ├── auth.go                 #   인증 타입 (OAuth2, mTLS, APIKey 등)
│   ├── core.go                 #   Task, Message, Event, Part, Artifact
│   ├── errors.go               #   A2A 에러 정의 (ParseError, InvalidRequest 등)
│   ├── push.go                 #   PushConfig, PushAuthInfo
│   ├── task_version.go         #   TaskVersion (낙관적 동시성)
│   └── doc.go                  #   패키지 문서
│
├── a2aclient/                  # 클라이언트 SDK
│   ├── client.go               #   Client 구현 (SendMessage, Stream, GetTask 등)
│   ├── factory.go              #   Client 팩토리 (AgentCard → Client 생성)
│   ├── transport.go            #   Transport 인터페이스 (추상 전송 계층)
│   ├── jsonrpc.go              #   JSON-RPC Transport 구현
│   ├── grpc.go                 #   gRPC Transport 구현
│   ├── auth.go                 #   클라이언트 인증 처리
│   ├── middleware.go           #   CallInterceptor (Before/After 훅)
│   └── agentcard/              #   AgentCard 리졸버
│       └── resolver.go         #     Well-known URI에서 AgentCard 가져오기
│
├── a2asrv/                     # 서버 SDK
│   ├── handler.go              #   RequestHandler 인터페이스 + 기본 구현
│   ├── agentexec.go            #   AgentExecutor 인터페이스 (에이전트 로직)
│   ├── jsonrpc.go              #   JSON-RPC HTTP 핸들러 (SSE 스트리밍 포함)
│   ├── agentcard.go            #   AgentCard 서빙 (well-known + CORS)
│   ├── middleware.go           #   서버 미들웨어/인터셉터
│   ├── intercepted_handler.go  #   인터셉터 래핑 핸들러
│   ├── reqctx.go               #   RequestContext (요청 컨텍스트)
│   ├── reqmeta.go              #   RequestMetadata (요청 메타데이터)
│   ├── auth.go                 #   서버 인증 처리
│   ├── extensions.go           #   확장 설정/활성화
│   ├── tasks.go                #   TaskStore 인터페이스
│   ├── eventqueue/             #   이벤트 큐
│   │   ├── manager.go          #     Manager 인터페이스
│   │   ├── manager_in_memory_impl.go  # InMemory 구현
│   │   ├── queue.go            #     Queue 인터페이스
│   │   ├── queue_in_memory_impl.go    # InMemory 구현
│   │   ├── heartbeater.go      #     하트비트 관리
│   │   ├── pullqueue.go        #     풀 큐
│   │   ├── pushqueue.go        #     푸시 큐
│   │   └── retry.go            #     재시도 로직
│   ├── push/                   #   푸시 알림
│   │   ├── sender.go           #     PushSender 인터페이스 + 구현
│   │   └── store.go            #     PushConfigStore 인터페이스
│   ├── workqueue/              #   작업 큐 (분산 모드)
│   │   ├── queue.go            #     WorkQueue 인터페이스
│   │   ├── heartbeater.go      #     하트비트
│   │   ├── pullqueue.go        #     풀 큐
│   │   ├── pushqueue.go        #     푸시 큐
│   │   └── retry.go            #     재시도
│   └── limiter/                #   동시성 제어
│       └── limiter.go          #     Limiter 인터페이스 + 구현
│
├── a2agrpc/                    # gRPC Transport
│   ├── handler.go              #   gRPC 핸들러 (A2AService 구현)
│   └── doc.go
│
├── a2aext/                     # 확장 (OpenTelemetry)
│   ├── activator.go            #   Span 활성화
│   ├── propagator.go           #   컨텍스트 전파 (분산 트레이싱)
│   └── utils.go
│
├── a2apb/                      # Protocol Buffers (자동 생성)
│   ├── a2a.pb.go               #   메시지 정의 (~140KB)
│   ├── a2a_grpc.pb.go          #   gRPC 서비스 정의 (~26KB)
│   └── pbconv/                 #   Proto ↔ Go 타입 변환 유틸
│
├── internal/                   # 내부 구현 (외부 임포트 불가)
│   ├── taskexec/               #   태스크 실행 엔진
│   │   ├── api.go              #     공개 API/팩토리
│   │   ├── local_manager.go    #     로컬 매니저 (단일 프로세스)
│   │   ├── distributed_manager.go  # 분산 매니저 (클러스터)
│   │   ├── execution_handler.go    # 실행 핸들러
│   │   ├── promise.go          #     비동기 태스크 프로미스
│   │   ├── subscription.go     #     이벤트 구독 관리
│   │   ├── limiter.go          #     동시성 리미터
│   │   └── work_queue_handler.go   # 작업 큐 핸들러
│   ├── taskstore/              #   태스크 저장소
│   │   ├── store.go            #     InMemory TaskStore 구현
│   │   └── validator.go        #     태스크 상태 검증
│   ├── taskupdate/             #   태스크 업데이트
│   │   ├── final.go            #     터미널 상태 추적
│   │   └── manager.go          #     업데이트 오케스트레이션
│   ├── eventpipe/              #   이벤트 스트리밍
│   │   └── local.go            #     로컬 이벤트 파이프
│   ├── sse/                    #   Server-Sent Events
│   │   └── sse.go              #     SSEWriter, ParseDataStream
│   ├── jsonrpc/                #   JSON-RPC 2.0
│   │   └── jsonrpc.go          #     요청/응답/에러 직렬화
│   ├── grpcutil/               #   gRPC 유틸리티
│   │   └── errors.go           #     에러 변환
│   ├── testutil/               #   테스트 유틸리티
│   │   ├── queue.go            #     Mock EventQueue
│   │   ├── task_store.go       #     Mock TaskStore
│   │   ├── workqueue.go        #     Mock WorkQueue
│   │   ├── push_sender.go      #     Mock PushSender
│   │   └── testexecutor/       #     Mock AgentExecutor
│   └── utils/
│       └── utils.go
│
├── log/                        # 로깅
│   └── logger.go               #   Logger 인터페이스
│
├── examples/                   # 예제
│   ├── helloworld/             #   기본 예제
│   │   ├── server/grpc/        #     gRPC 서버
│   │   ├── server/jsonrpc/     #     JSON-RPC 서버
│   │   └── client/             #     클라이언트
│   └── clustermode/            #   분산 모드 예제
│       ├── server/             #     MySQL + 외부 큐 서버
│       │   └── deploy/         #       docker-compose 배포
│       └── client/
│
├── e2e/tck/                    # E2E 테스트 (TCK)
├── docs/ai/                    # 아키텍처 문서
└── .github/workflows/          # CI/CD
```

## 패키지 아키텍처 다이어그램

```
                    ┌─────────────────────────┐
                    │      Application        │
                    │   (examples/, e2e/)      │
                    └─────┬──────────┬────────┘
                          │          │
              ┌───────────▼─┐    ┌──▼───────────┐
              │  a2aclient  │    │    a2asrv     │
              │  (Client)   │    │   (Server)    │
              └──┬──────┬───┘    └──┬──────┬─────┘
                 │      │          │      │
          ┌──────▼─┐  ┌─▼──────┐  │   ┌──▼──────┐
          │jsonrpc  │  │ grpc   │  │   │a2agrpc  │
          │transport│  │transport│ │   │(gRPC    │
          └────┬────┘  └───┬────┘  │   │handler) │
               │           │       │   └────┬────┘
          ┌────▼───────────▼───────▼────────▼────┐
          │            a2a (Core Domain)          │
          │   Task, Message, Event, AgentCard     │
          └──────────────┬───────────────────────┘
                         │
          ┌──────────────▼───────────────────────┐
          │         internal/                     │
          │  taskexec, taskstore, eventpipe,      │
          │  sse, jsonrpc, grpcutil               │
          └──────────────┬───────────────────────┘
                         │
          ┌──────────────▼───────────────────────┐
          │  a2apb (Generated Protocol Buffers)   │
          │  a2aext (OpenTelemetry Extensions)    │
          └──────────────────────────────────────┘
```

## 핵심 인터페이스

### AgentExecutor (에이전트 개발자가 구현)
```go
// a2asrv/agentexec.go
type AgentExecutor interface {
    Execute(ctx context.Context, event a2a.Event, queue eventqueue.Queue) error
}
```
- 에이전트의 비즈니스 로직을 구현하는 핵심 인터페이스
- `event`: 수신된 메시지/태스크 이벤트
- `queue`: 응답 이벤트를 작성하는 큐 (스트리밍 지원)

### RequestHandler (서버 코어)
```go
// a2asrv/handler.go
type RequestHandler interface {
    OnSendMessage(ctx context.Context, params MessageSendParams) (a2a.Event, error)
    OnSendMessageStream(ctx context.Context, params MessageSendParams) iter.Seq2[a2a.Event, error]
    OnGetTask(ctx context.Context, params TaskQueryParams) (*a2a.Task, error)
    OnCancelTask(ctx context.Context, params TaskIdParams) (*a2a.Task, error)
    OnResubscribe(ctx context.Context, params TaskIdParams) iter.Seq2[a2a.Event, error]
    // Push notification config methods...
    // Extended agent card method...
}
```

### Transport (클라이언트 전송 추상화)
```go
// a2aclient/transport.go
type Transport interface {
    SendMessage(ctx context.Context, req SendMessageRequest) (a2a.Event, error)
    SendMessageStream(ctx context.Context, req SendMessageRequest) iter.Seq2[a2a.Event, error]
    GetTask(ctx context.Context, req GetTaskRequest) (*a2a.Task, error)
    CancelTask(ctx context.Context, req CancelTaskRequest) (*a2a.Task, error)
    // ...
}
```

### TaskStore (태스크 영속성)
```go
// a2asrv/tasks.go
type TaskStore interface {
    Save(ctx context.Context, event a2a.Event, version TaskVersion) error
    Get(ctx context.Context, taskID TaskID) (*a2a.Task, TaskVersion, error)
    List(ctx context.Context, params ListParams) ([]*a2a.Task, string, error)
}
```

### EventQueue (이벤트 큐)
```go
// a2asrv/eventqueue/queue.go
type Queue interface {
    Write(event a2a.Event) error
    Read(ctx context.Context) iter.Seq2[a2a.Event, error]
    Close() error
}

type Manager interface {
    GetOrCreate(taskID string) Queue
    Destroy(taskID string)
}
```

## 핵심 데이터 타입

### Event 인터페이스 (Sealed Interface)
```go
// a2a/core.go
type Event interface {
    isEvent()  // sealed - 외부 구현 불가
}

// 구현체:
// - Message        : 사용자/에이전트 메시지
// - Task           : 상태를 가진 작업 단위
// - TaskStatusUpdateEvent   : 태스크 상태 변경
// - TaskArtifactUpdateEvent : 아티팩트 업데이트 (청크 스트리밍)
```

### Part 인터페이스 (Discriminated Union)
```go
// a2a/core.go
type Part interface {
    isPart()  // sealed
}

// 구현체:
// - TextPart : 텍스트 콘텐츠
// - DataPart : 구조화된 JSON 데이터
// - FilePart : 파일 (URI 또는 Base64 바이트)
```

### TaskState (상태 머신)
```go
// a2a/core.go
type TaskState string
const (
    TaskStateSubmitted     TaskState = "submitted"
    TaskStateWorking       TaskState = "working"
    TaskStateCompleted     TaskState = "completed"      // terminal
    TaskStateFailed        TaskState = "failed"          // terminal
    TaskStateCanceled      TaskState = "canceled"        // terminal
    TaskStateRejected      TaskState = "rejected"        // terminal
    TaskStateInputRequired TaskState = "input-required"  // interrupt
    TaskStateAuthRequired  TaskState = "auth-required"   // interrupt
    TaskStateUnknown       TaskState = "unknown"
)
```

## 설계 패턴

### 1. Functional Options 패턴
모든 주요 생성자에서 사용. 유연한 설정과 기본값 제공:
```go
handler := a2asrv.NewHandler(executor,
    a2asrv.WithLogger(logger),
    a2asrv.WithTaskStore(store),
    a2asrv.WithEventQueueManager(queueMgr),
    a2asrv.WithPushNotifications(pushStore, pushSender),
    a2asrv.WithConcurrencyConfig(config),
    a2asrv.WithClusterMode(workQueue),
    a2asrv.WithCallInterceptor(interceptor),
)
```

### 2. Sealed Interface (Rust enum 유사)
비공개 마커 메서드(`isEvent()`, `isPart()`)로 외부 구현을 차단:
- `Event`: Message | Task | TaskStatusUpdateEvent | TaskArtifactUpdateEvent
- `Part`: TextPart | DataPart | FilePart
- JSON 역직렬화 시 `kind` 필드 또는 멤버 존재 여부로 판별

### 3. Iterator 패턴 (Go 1.24+ `iter.Seq2`)
스트리밍 응답에 채널 대신 range-over-func 패턴 사용:
```go
// 서버 측
func OnSendMessageStream(ctx, params) iter.Seq2[a2a.Event, error]

// 클라이언트 측
for event, err := range client.SendMessageStream(ctx, req) {
    // 이벤트 처리
}
```

### 4. 미들웨어/인터셉터 패턴
서버·클라이언트 양측에서 Before/After 훅 지원:
```go
type CallInterceptor interface {
    Before(ctx context.Context, method string, req any) (context.Context, error)
    After(ctx context.Context, method string, resp any, err error) error
}
```

### 5. Factory 패턴
AgentCard에서 클라이언트 자동 생성:
```go
card, _ := agentcard.DefaultResolver.Resolve(ctx)
client, _ := a2aclient.NewFromCard(ctx, card, options...)
```

### 6. Optimistic Concurrency (낙관적 동시성)
`TaskVersion`(int64)으로 동시 업데이트 충돌 방지:
- 이벤트 큐 읽기/쓰기 시 버전 추적
- TaskStore 저장 시 버전 검증

## 서버 요청 처리 흐름

```
HTTP Request
    │
    ▼
┌──────────────────────┐
│  jsonrpcHandler      │  JSON-RPC 메서드 라우팅
│  (a2asrv/jsonrpc.go) │  message/send, message/stream, tasks/get ...
└──────────┬───────────┘
           │
    ┌──────▼──────────────┐
    │  RequestHandler     │  비즈니스 로직 조율
    │  (a2asrv/handler.go)│  태스크 생성, 이벤트 큐, 상태 관리
    └──────────┬──────────┘
               │
    ┌──────────▼──────────┐
    │  AgentExecutor      │  에이전트 비즈니스 로직 실행
    │  (사용자 구현)       │  이벤트 수신 → 처리 → 큐에 응답 작성
    └──────────┬──────────┘
               │
    ┌──────────▼──────────┐
    │  EventQueue         │  이벤트 버퍼링 + 스트리밍
    │  (eventqueue/)      │  Write → Read (iter.Seq2)
    └──────────┬──────────┘
               │
    ┌──────────▼──────────┐
    │  TaskStore +        │  영속성 + 알림
    │  PushSender         │  상태 저장, 웹훅 전송
    └─────────────────────┘
```

## SSE 스트리밍 상세

```go
// internal/sse/sse.go

// SSEWriter: http.ResponseWriter + Flusher 래핑
type SSEWriter struct { ... }

func (w *SSEWriter) WriteHeaders()           // Content-Type: text/event-stream
func (w *SSEWriter) WriteData(id, data)      // id: {id}\ndata: {json}\n\n
func (w *SSEWriter) WriteKeepAlive()         // : keep-alive\n\n

// ParseDataStream: SSE 클라이언트 파서 (iter.Seq2 기반)
func ParseDataStream(reader) iter.Seq2[[]byte, error]
// - "data: " 및 "data:" 접두어 모두 지원
// - 10MB 최대 토큰 사이즈
```

## JSON-RPC 에러 코드 매핑

| 코드 | 의미 | A2A 매핑 |
|:-----|:-----|:---------|
| -32700 | Parse Error | ParseError |
| -32600 | Invalid Request | InvalidRequest |
| -32601 | Method Not Found | MethodNotFound |
| -32602 | Invalid Params | InvalidParams |
| -32603 | Internal Error | InternalError |
| -32001 | Task Not Found | TaskNotFound |
| -32002 | Task Not Cancelable | TaskNotCancelable |
| -32003 | Push Not Supported | PushNotSupported |
| -32004 | Push Config Not Found | UnsupportedOperation |
| -32005 | Incompatible Content Types | IncompatibleContentTypes |
| -32006 | Extensions Not Supported | ExtensionsNotSupported |
| -32007 | Agent Card Not Found | AgentCardNotFound |
| -31401 | Unauthenticated | Unauthenticated |
| -31403 | Unauthorized | Unauthorized |

## 관련 항목
- [기술 스택](./01_tech-stack.md)

## 참고 자료
- [a2a-go 소스 코드](https://github.com/a2aproject/a2a-go)
- [A2A Protocol Specification](https://a2a-protocol.org/latest/specification/)
- [docs/ai/OVERVIEW.md](../../docs/ai/OVERVIEW.md) - 프로젝트 내 아키텍처 문서

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
