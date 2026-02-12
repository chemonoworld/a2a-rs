# A2A (Agent2Agent) Protocol - Technical Engineering Analysis Report

## 1. 프로젝트 개요

A2A(Agent2Agent)는 **서로 다른 프레임워크, 벤더, 조직에서 만든 AI 에이전트들이 상호 통신하고 협업할 수 있게 해주는 오픈 프로토콜**이다. Google이 기여하고 Linux Foundation이 관리하며, Apache License 2.0으로 공개되어 있다.

핵심 철학은 에이전트를 **불투명(opaque) 엔티티**로 취급하는 것이다. 에이전트 간 통신 시 내부 메모리, 도구, 로직을 노출하지 않고, 선언된 능력(capabilities)과 교환된 컨텍스트만으로 협업한다.

### MCP와의 차이점

| 구분 | A2A | MCP (Model Context Protocol) |
|:-----|:----|:----|
| 목적 | 에이전트 간 협업 (Agent-to-Agent) | 모델과 도구/리소스 연결 (Agent-to-Tool) |
| 상호작용 | 멀티턴, 상태 기반, 협상 가능 | 단방향, 무상태, 함수 호출 스타일 |
| 대상 | 자율적 문제 해결 주체 | 정해진 기능을 수행하는 도구 |

---

## 2. 레포지토리 구조 분석

```
A2A/
├── specification/          # 프로토콜의 정규 정의 (Source of Truth)
│   ├── a2a.proto          # Protocol Buffers 3 정의 (~800 lines)
│   ├── buf.yaml           # Buf 빌드 설정 (lint, breaking change 감지)
│   ├── buf.gen.yaml       # 코드 생성 설정 (Python, Go, Java, TypeScript)
│   ├── buf.lock           # 의존성 잠금 파일
│   └── json/README.md     # JSON Schema 안내
├── docs/                   # MkDocs Material 기반 문서 사이트 소스
│   ├── specification.md   # 프로토콜 스펙 (약 39K 토큰의 대규모 문서)
│   ├── definitions.md     # Proto/JSON Schema 참조
│   ├── topics/            # 개념 문서 (what-is-a2a, key-concepts 등)
│   ├── tutorials/         # Python SDK 튜토리얼 (8단계)
│   ├── sdk/               # SDK 참조 문서
│   └── assets/            # 로고, 다이어그램 이미지
├── adrs/                   # Architecture Decision Records
│   └── adr-001-protojson-serialization.md
├── scripts/               # 빌드/변환/린팅 자동화 스크립트
│   ├── proto_to_json_schema.sh  # Proto → JSON Schema 변환
│   ├── build_docs.sh      # 문서 빌드
│   ├── lint.sh / format.sh # 코드 품질
│   └── build_llms_full.sh # LLM용 전체 문서 빌드
├── .github/               # CI/CD 파이프라인
│   ├── workflows/         # 9개 GitHub Actions 워크플로우
│   └── linters/           # Super Linter 설정
├── mkdocs.yml             # 문서 사이트 설정
└── README.md              # 프로젝트 진입점
```

### 중요 포인트
- 이 레포는 **실행 코드가 아닌 프로토콜 스펙 레포**이다
- 실제 SDK 구현체는 별도 레포에 존재: `a2a-python`, `a2a-go`, `a2a-js`, `a2a-java`, `a2a-dotnet`
- 샘플 코드도 별도 레포: `a2a-samples`
- 이 레포의 산출물은 **프로토콜 정의 (`a2a.proto`, JSON Schema)**와 **문서 사이트 (`a2a-protocol.org`)**

---

## 3. 프로토콜 아키텍처 심층 분석

### 3.1 3계층 스펙 구조

```
Layer 1: Canonical Data Model (Protocol Buffers)
    ↓ 데이터 구조 정의
Layer 2: Abstract Operations (RPC 시맨틱)
    ↓ 연산 정의
Layer 3: Protocol Bindings (JSON-RPC / gRPC / HTTP+JSON)
    → 실제 전송 매핑
```

- **Layer 1**: `a2a.proto`에 정의된 메시지 타입들. 프로토콜에 무관한 순수 데이터 모델
- **Layer 2**: `SendMessage`, `GetTask` 등 추상 오퍼레이션. 바인딩 독립적
- **Layer 3**: 3가지 프로토콜 바인딩으로 동일한 시맨틱을 구현

| 바인딩 | 전송 | 직렬화 | 스트리밍 |
|:-------|:-----|:-------|:---------|
| JSON-RPC 2.0 | HTTP(S) | ProtoJSON | SSE (Server-Sent Events) |
| gRPC | HTTP/2 | Protocol Buffers binary | gRPC Server Streaming |
| HTTP+JSON (REST) | HTTP(S) | ProtoJSON | SSE |

### 3.2 핵심 서비스 인터페이스 (`A2AService`)

`a2a.proto`에 정의된 11개 RPC 메서드:

```protobuf
service A2AService {
  // 메시지 송수신
  rpc SendMessage(SendMessageRequest) returns (SendMessageResponse);
  rpc SendStreamingMessage(SendMessageRequest) returns (stream StreamResponse);

  // 태스크 관리
  rpc GetTask(GetTaskRequest) returns (Task);
  rpc ListTasks(ListTasksRequest) returns (ListTasksResponse);
  rpc CancelTask(CancelTaskRequest) returns (Task);

  // 스트리밍 구독
  rpc SubscribeToTask(SubscribeToTaskRequest) returns (stream StreamResponse);

  // 푸시 알림 설정
  rpc CreateTaskPushNotificationConfig(...) returns (TaskPushNotificationConfig);
  rpc GetTaskPushNotificationConfig(...) returns (TaskPushNotificationConfig);
  rpc ListTaskPushNotificationConfig(...) returns (...);
  rpc DeleteTaskPushNotificationConfig(...) returns (Empty);

  // 에이전트 디스커버리
  rpc GetExtendedAgentCard(...) returns (AgentCard);
}
```

모든 RPC에 `google.api.http` 어노테이션이 있어 REST 매핑이 자동 생성된다. 또한 `/{tenant}/` 패턴의 `additional_bindings`로 **멀티테넌시를 네이티브 지원**한다.

---

## 4. 핵심 데이터 모델 분석

### 4.1 Task (태스크) - 핵심 작업 단위

```
Task {
  id: string (UUID, 서버 생성)
  context_id: string (UUID, 서버 생성, 관련 상호작용 그룹)
  status: TaskStatus {
    state: TaskState (enum)
    message: Message (상태 메시지)
    timestamp: Timestamp
  }
  artifacts: [Artifact] (결과물)
  history: [Message] (상호작용 히스토리)
  metadata: Struct (커스텀 메타데이터)
}
```

### 4.2 Task 상태 머신 (State Machine)

```
                    ┌──────────────┐
                    │ UNSPECIFIED  │
                    └──────┬───────┘
                           │ (메시지 수신)
                    ┌──────▼───────┐
                    │  SUBMITTED   │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
              ┌────►│   WORKING    │◄────┐
              │     └──┬───┬───┬───┘     │
              │        │   │   │         │
   (추가 입력 제공) │   │   │    (인증 완료)
              │        │   │   │         │
    ┌─────────▼─┐  │   │   │  ┌─────────▼──────┐
    │INPUT_      │  │   │   │  │AUTH_            │
    │REQUIRED    │  │   │   │  │REQUIRED         │
    └────────────┘  │   │   │  └─────────────────┘
                    │   │   │       (인터럽트 상태)
         ┌──────────┘   │   └──────────┐
         ▼              ▼              ▼
   ┌───────────┐  ┌──────────┐  ┌───────────┐
   │ COMPLETED │  │  FAILED  │  │ CANCELED  │
   └───────────┘  └──────────┘  └───────────┘
         ▲                             ▲
         │         ┌──────────┐        │
         └─────────│ REJECTED │────────┘
                   └──────────┘
                   (터미널 상태들)
```

**터미널 상태**: COMPLETED, FAILED, CANCELED, REJECTED - 한번 도달하면 되돌릴 수 없음 (Task Immutability)
**인터럽트 상태**: INPUT_REQUIRED, AUTH_REQUIRED - 추가 정보 필요

### 4.3 Message (메시지)

```
Message {
  message_id: string (UUID, 생성자가 만듦)
  context_id: string (선택, 컨텍스트 연결)
  task_id: string (선택, 태스크 연결)
  role: Role (ROLE_USER | ROLE_AGENT)
  parts: [Part] (콘텐츠, 1개 이상 필수)
  metadata: Struct
  extensions: [string] (확장 URI들)
  reference_task_ids: [string] (참조하는 이전 태스크들)
}
```

### 4.4 Part (파트) - 통합 콘텐츠 컨테이너

v1.0에서 완전히 재설계된 핵심 타입. `TextPart`, `FilePart`, `DataPart`가 **단일 `Part` 메시지로 통합**:

```protobuf
message Part {
  oneof content {
    string text = 1;      // 텍스트
    bytes raw = 2;         // 바이너리 (Base64 in JSON)
    string url = 3;        // 파일 URL
    Value data = 4;        // 구조화된 JSON 데이터
  }
  Struct metadata = 5;
  string filename = 6;
  string media_type = 7;  // MIME 타입
}
```

**설계 의도**: `oneof`를 사용하여 정확히 하나의 콘텐츠 타입만 포함하도록 강제. `kind` 디스크리미네이터 대신 JSON 멤버 존재 여부로 타입을 판별 (`"text" in part` 패턴).

### 4.5 Agent Card (에이전트 카드) - 디지털 명함

에이전트의 자기 서술 매니페스트. 클라이언트가 에이전트를 발견하고 상호작용 방법을 파악하는 데 사용:

```
AgentCard {
  name, description: 에이전트 식별 정보
  supported_interfaces: [AgentInterface] {
    url: 서비스 엔드포인트
    protocol_binding: "JSONRPC" | "GRPC" | "HTTP+JSON"
    protocol_version: "1.0"
    tenant: 기본 테넌트
  }
  capabilities: AgentCapabilities {
    streaming: bool
    push_notifications: bool
    extended_agent_card: bool
    extensions: [AgentExtension]
  }
  security_schemes: map<string, SecurityScheme>
  skills: [AgentSkill] {
    id, name, description
    tags, examples
    input_modes, output_modes  // MIME 타입
  }
  signatures: [AgentCardSignature]  // JWS 서명
  default_input_modes, default_output_modes
}
```

### 4.6 Artifact (아티팩트) - 태스크 결과물

```
Artifact {
  artifact_id: string (태스크 내 유니크)
  name: string (사람이 읽을 수 있는 이름)
  description: string
  parts: [Part] (1개 이상 필수)
  metadata: Struct
  extensions: [string]
}
```

---

## 5. 통신 메커니즘 상세

### 5.1 동기 요청/응답 (Request/Response)

```
Client                          A2A Server
  │                                │
  │─── POST /message:send ────────►│
  │    (SendMessageRequest)        │
  │                                │ 처리...
  │◄── SendMessageResponse ───────│
  │    (Task 또는 Message)          │
```

`SendMessageResponse`는 `oneof` 패턴으로 **Task 또는 Message**를 반환:
- **Message**: 즉각 응답 가능한 간단한 요청
- **Task**: 장기 실행이 필요한 복잡한 작업

`blocking: true` 설정 시 서버가 태스크 완료 또는 인터럽트까지 대기 후 응답.

### 5.2 스트리밍 (SSE - Server-Sent Events)

```
Client                          A2A Server
  │                                │
  │─── POST /message:stream ──────►│
  │                                │
  │◄── HTTP 200 (text/event-stream)│
  │◄── StreamResponse(Task)  ──────│  SUBMITTED
  │◄── StreamResponse(StatusUpdate)│  WORKING
  │◄── StreamResponse(ArtifactUpd) │  아티팩트 청크 1
  │◄── StreamResponse(ArtifactUpd) │  아티팩트 청크 2 (append=true)
  │◄── StreamResponse(ArtifactUpd) │  아티팩트 최종 (last_chunk=true)
  │◄── StreamResponse(StatusUpdate)│  COMPLETED
  │    (스트림 종료)                 │
```

`StreamResponse`의 `oneof payload`로 4가지 이벤트 타입을 전달:
1. `Task` - 태스크 전체 상태
2. `Message` - 에이전트 메시지
3. `TaskStatusUpdateEvent` - 상태 변경 이벤트
4. `TaskArtifactUpdateEvent` - 아티팩트 업데이트 (`append`, `last_chunk` 플래그로 청크 재조립)

**재구독**: SSE 연결이 끊긴 경우 `SubscribeToTask`로 재연결 가능.

### 5.3 푸시 알림 (Push Notifications)

매우 장기 실행되는 태스크용. 클라이언트가 웹훅 URL을 등록하면 서버가 상태 변경 시 HTTP POST로 알림:

```
Client                  A2A Server              Client Webhook
  │                        │                        │
  │─ SendMessage ─────────►│                        │
  │  (push_notification_   │                        │
  │   config.url=webhook)  │                        │
  │◄── Task (SUBMITTED) ──│                        │
  │                        │ 처리중...               │
  │    (연결 끊김 가능)      │                        │
  │                        │── POST StreamResponse──►│
  │                        │   (상태 변경 알림)        │
  │◄─── 알림 수신 ──────────────────────────────────│
  │─── GetTask ───────────►│                        │
  │◄── Task (최신 상태) ───│                        │
```

보안 메커니즘:
- 서버 → 웹훅: JWT + JWKS 기반 비대칭 키 인증
- 웹훅: 타임스탬프/nonce 기반 재전송 공격 방지
- SSRF 방지를 위한 URL 검증 필수

---

## 6. 에이전트 디스커버리 메커니즘

### 6.1 Well-Known URI (표준 발견)

```
GET https://{agent-domain}/.well-known/agent-card.json
→ Returns AgentCard JSON
```

RFC 8615 기반. 퍼블릭 에이전트에 적합.

### 6.2 큐레이티드 레지스트리 (엔터프라이즈)

중앙 레지스트리가 Agent Card들을 관리하고 클라이언트가 스킬/태그 기준으로 검색. A2A 스펙 자체에는 레지스트리 API가 정의되어 있지 않으며 추후 표준화 예정.

### 6.3 Extended Agent Card (인증된 확장 카드)

인증된 클라이언트에게만 추가 정보를 노출:

```
GET /extendedAgentCard (인증 헤더 포함)
→ Returns 확장된 AgentCard (추가 스킬, 민감한 정보 포함)
```

---

## 7. 보안 아키텍처

### 7.1 전송 계층

- **HTTPS 필수** (프로덕션). TLS 1.2+ 권장
- 서버 인증서 검증 (MITM 방지)

### 7.2 인증 (Authentication)

Agent Card의 `security_schemes`에 선언된 5가지 스킴:

```protobuf
message SecurityScheme {
  oneof scheme {
    APIKeySecurityScheme api_key;       // API 키 (header/query/cookie)
    HTTPAuthSecurityScheme http_auth;    // HTTP Auth (Bearer, Basic 등)
    OAuth2SecurityScheme oauth2;         // OAuth 2.0
    OpenIdConnectSecurityScheme oidc;    // OpenID Connect
    MutualTlsSecurityScheme mtls;        // 상호 TLS (v1.0 신규)
  }
}
```

**핵심 원칙**: A2A 페이로드에 인증 정보를 넣지 않음. HTTP 헤더를 통해 전달 (`Authorization: Bearer <TOKEN>`).

### 7.3 OAuth 2.0 플로우

v1.0에서 현대화:
- **Authorization Code + PKCE**: 표준 플로우 (PKCE 필수 옵션 추가)
- **Client Credentials**: 서버 간 통신
- **Device Code** (RFC 8628): CLI/IoT 디바이스용 (v1.0 신규)
- ~~Implicit~~, ~~Password~~: 보안 위험으로 **제거(deprecated)**

### 7.4 Agent Card 서명 (v1.0 신규)

JWS (RFC 7515) + JSON Canonicalization (RFC 8785)로 Agent Card의 무결성을 암호학적으로 검증:

```json
{
  "signatures": [{
    "protected": "base64url_encoded_header",
    "signature": "base64url_encoded_signature"
  }]
}
```

---

## 8. 확장 메커니즘 (Extensions)

프로토콜의 확장성을 제공하되 코어 스펙을 깨지 않는 설계:

### 8.1 확장 유형

| 유형 | 설명 | 예시 |
|:-----|:-----|:-----|
| Data-only | Agent Card에 구조화된 메타데이터 추가 | GDPR 준수 정보 |
| Profile | 요청/응답에 추가 구조·제약 부과 | 모든 메시지가 특정 스키마를 따르도록 |
| Method (Extended Skills) | 새로운 RPC 메서드 추가 | `tasks/search` 메서드 |
| State Machine | 태스크 상태 머신에 새 상태/전이 추가 | 커스텀 서브상태 |

### 8.2 확장 활성화 흐름

```http
POST /agents/eightball HTTP/1.1
A2A-Extensions: https://example.com/ext/konami-code/v1
Content-Type: application/json
{ "jsonrpc": "2.0", "method": "SendMessage", ... }

→ 응답:
A2A-Extensions: https://example.com/ext/konami-code/v1  (활성화 확인)
```

- 클라이언트: `A2A-Extensions` HTTP 헤더로 활성화 요청
- 서버: 지원되는 확장만 활성화하고 응답 헤더에 반영
- 미지원 확장은 무시됨 (기본 비활성)

### 8.3 제약사항

확장이 할 수 **없는** 것:
- 코어 데이터 구조의 필드 추가/제거 (→ `metadata` 맵 사용)
- Enum 타입에 새 값 추가 (→ `metadata`로 시맨틱 확장)

---

## 9. 빌드 시스템 및 도구 체인

### 9.1 Protocol Buffers 도구 체인

```
a2a.proto (Source of Truth)
    │
    ├─── buf (lint + breaking change 감지)
    │    └── buf.yaml: STANDARD 규칙, FILE 레벨 breaking 감지
    │
    ├─── buf generate (코드 생성)
    │    ├── Python (protobuf + gRPC stubs)
    │    ├── Go (protobuf + gRPC stubs)
    │    ├── Java (protobuf + gRPC stubs)
    │    └── TypeScript (ts-proto)
    │
    └─── proto_to_json_schema.sh
         ├── protoc + protoc-gen-jsonschema (bufbuild)
         ├── jq 번들링
         └── → a2a.json (JSON Schema 2020-12)
```

### 9.2 문서 빌드

```
docs/ + mkdocs.yml
    │
    ├── MkDocs Material (테마)
    ├── mike (버전 관리 - v0.1.0 ~ v1.0)
    ├── pymdownx.snippets (--8<-- 로 proto 파일 인라인)
    ├── pymdownx.superfences (Mermaid 다이어그램)
    ├── macros 플러그인 (동적 콘텐츠)
    └── redirects 플러그인 (URL 마이그레이션)
```

### 9.3 CI/CD 파이프라인 (GitHub Actions)

| 워크플로우 | 목적 |
|:-----------|:-----|
| `docs.yml` | MkDocs 문서 빌드·배포 |
| `linter.yaml` | Super Linter (Markdown, Proto, ESLint, CSS 등) |
| `spelling.yaml` | 맞춤법 검사 |
| `links.yaml` | Lychee 링크 체커 |
| `conventional-commits.yml` | 커밋 메시지 컨벤션 체크 |
| `release-please.yml` | 자동 릴리스 관리 |
| `dispatch-a2a-update.yml` | SDK 레포에 변경사항 전파 |
| `check-linked-issues.yml` | PR-이슈 연결 검증 |
| `stale.yaml` | 비활성 이슈/PR 정리 |

### 9.4 직렬화 전략 (ADR-001)

**ProtoJSON**을 JSON 직렬화의 정규 방식으로 채택:
- Enum: `SCREAMING_SNAKE_CASE` (`TASK_STATE_COMPLETED`)
- 필드: `camelCase` (`contextId`, `taskId`)
- 타임스탬프: ISO 8601 UTC 밀리초 정밀도

트레이드오프: 개발자에게 덜 익숙한 enum 형식이지만, protobuf 도구 체인과의 일관성 확보.

---

## 10. 요청 라이프사이클 전체 흐름

```
1. 에이전트 디스커버리
   Client ──GET /.well-known/agent-card.json──► A2A Server
   Client ◄─────── AgentCard (JSON) ───────── A2A Server

2. 인증
   Client: AgentCard.security_schemes 파싱
   Client ──── OAuth/Token 요청 ────► Auth Server
   Client ◄─── JWT/Token ──────────── Auth Server

3. 메시지 전송 (동기)
   Client ──POST /message:send + JWT──► A2A Server
   A2A Server: 메시지 처리, 태스크 생성/응답
   Client ◄── Task 또는 Message ───── A2A Server

4. 메시지 전송 (스트리밍)
   Client ──POST /message:stream + JWT──► A2A Server
   Client ◄── SSE: Task (SUBMITTED) ──── A2A Server
   Client ◄── SSE: StatusUpdate (WORKING)
   Client ◄── SSE: ArtifactUpdate (chunk)
   Client ◄── SSE: StatusUpdate (COMPLETED)
   (스트림 종료)

5. 멀티턴 상호작용
   Agent 응답: INPUT_REQUIRED
   Client: 추가 정보와 함께 같은 context_id로 재전송
   Agent: 처리 재개 → COMPLETED
```

---

## 11. 설계 패턴 및 아키텍처적 결정

### 11.1 `oneof` 패턴 활용 (Rust의 Enum과 유사)

프로토콜 전반에 `oneof`를 적극 활용하여 **타입 안전성**을 보장:

```protobuf
// Part 콘텐츠: 정확히 하나만 존재
oneof content { text, raw, url, data }

// 응답 타입: Task 또는 Message
oneof payload { task, message }

// 스트림 이벤트: 4가지 중 하나
oneof payload { task, message, status_update, artifact_update }

// 보안 스킴: 5가지 중 하나
oneof scheme { api_key, http_auth, oauth2, oidc, mtls }

// OAuth 플로우: 5가지 중 하나
oneof flow { authorization_code, client_credentials, implicit, password, device_code }
```

이는 Rust의 tagged enum과 동일한 패턴으로, JSON에서는 멤버 존재 여부로 판별한다.

### 11.2 ID 설계 전략

- **Simple UUID**: 복합 리소스 이름(`tasks/{id}`) 대신 단순 UUID 사용
- **서버 생성**: `task_id`, `context_id`는 서버가 생성
- **클라이언트 생성**: `message_id`는 클라이언트가 생성

### 11.3 Task Immutability (태스크 불변성)

터미널 상태에 도달한 태스크는 재시작 불가. 후속 작업은 같은 `context_id`에서 새 태스크 생성:
- 깔끔한 입출력 매핑
- 추적(traceability) 용이
- 구현 단순화

### 11.4 Cursor 기반 페이지네이션

v1.0에서 페이지 기반 → 커서 기반으로 전환:
```protobuf
message ListTasksRequest {
  optional int32 page_size = 3;  // 최대 100
  string page_token = 4;          // 이전 응답의 next_page_token
}
```

확장성과 일관성 보장.

### 11.5 멀티테넌시 네이티브 지원

모든 요청 메시지에 `tenant` 필드가 존재하며, REST URL에서도 `/{tenant}/` 패스 매핑 지원:
```protobuf
rpc SendMessage(SendMessageRequest) returns (SendMessageResponse) {
  option (google.api.http) = {
    post: "/message:send"
    additional_bindings: { post: "/{tenant}/message:send" }
  };
};
```

---

## 12. 에이전트 유형별 응답 패턴

| 에이전트 유형 | 응답 방식 | 사용 시나리오 |
|:-------------|:---------|:-------------|
| Message-only | 항상 Message 반환 | LLM 래퍼, 간단한 도구 호출 |
| Task-generating | 항상 Task 반환 | 장기 실행, 상태 추적 필요 |
| Hybrid | Message + Task 혼합 | 협상 후 작업 수행 |

Hybrid 에이전트의 일반적 흐름:
1. 초기 메시지 교환 (Message) - 스킬 협상, 범위 확인
2. 태스크 생성 (Task) - 실제 작업 수행
3. 인터럽트 처리 (INPUT_REQUIRED) - 추가 정보 요청
4. 완료 (COMPLETED) - 아티팩트 전달

---

## 13. v0.3 → v1.0 주요 Breaking Changes 요약

| 변경 | 영향도 | 내용 |
|:-----|:-------|:-----|
| Part 타입 통합 | CRITICAL | TextPart/FilePart/DataPart → 단일 Part |
| Enum 케이싱 | HIGH | kebab-case → SCREAMING_SNAKE_CASE |
| `kind` 제거 | HIGH | 디스크리미네이터 필드 → 멤버 존재 여부 판별 |
| AgentCard 재구조화 | HIGH | url, protocolVersion → supportedInterfaces[] |
| 페이지네이션 | MEDIUM | 페이지 기반 → 커서 기반 |
| OAuth 플로우 | MEDIUM | Implicit/Password 제거, Device Code 추가 |
| URL 경로 | LOW | `/v1/` 접두어 제거 |

---

## 14. SDK 에코시스템

| SDK | 언어 | 패키지 |
|:----|:-----|:-------|
| [a2a-python](https://github.com/a2aproject/a2a-python) | Python | `pip install a2a-sdk` |
| [a2a-go](https://github.com/a2aproject/a2a-go) | Go | `go get github.com/a2aproject/a2a-go` |
| [a2a-js](https://github.com/a2aproject/a2a-js) | JavaScript/TypeScript | `npm install @a2a-js/sdk` |
| [a2a-java](https://github.com/a2aproject/a2a-java) | Java | Maven |
| [a2a-dotnet](https://github.com/a2aproject/a2a-dotnet) | .NET | `dotnet add package A2A` |

추가 도구:
- [a2a-samples](https://github.com/a2aproject/a2a-samples): 샘플 에이전트·확장
- [a2a-inspector](https://github.com/a2aproject/a2a-inspector): 에이전트 검증 도구
- [a2a-tck](https://github.com/a2aproject/a2a-tck): 프로토콜 호환성 테스트 키트

---

## 15. 결론 및 기술적 평가

### 강점
1. **Proto-first 설계**: Protocol Buffers를 Source of Truth으로 두고 JSON Schema, gRPC, REST를 자동 생성하는 전략이 일관성을 보장
2. **Opaque Agent 패턴**: 에이전트 내부를 노출하지 않는 설계로 보안과 지적 재산 보호에 유리
3. **3가지 바인딩 동시 지원**: JSON-RPC, gRPC, HTTP+JSON으로 다양한 환경 대응
4. **확장 메커니즘**: 코어 스펙을 깨지 않으면서 도메인별 기능 추가 가능
5. **엔터프라이즈 보안**: OAuth 2.0 현대화, mTLS, JWS Agent Card 서명

### 한계/주의점
1. **레지스트리 API 미정의**: 에이전트 디스커버리의 핵심 구성요소가 아직 표준화되지 않음
2. **ProtoJSON 의존성**: 개발자에게 익숙하지 않은 SCREAMING_SNAKE_CASE enum, unknown 필드 라운드트립 불가
3. **스펙 전용 레포**: 실제 구현체가 없어 스펙과 구현 사이의 갭이 존재할 수 있음
4. **v1.0 마이그레이션 비용**: Part 타입 통합, Enum 케이싱 변경 등 상당한 breaking changes

---

*Report generated: 2026-02-12*
*Source: [a2aproject/A2A](https://github.com/a2aproject/A2A) (commit 68727dc)*
