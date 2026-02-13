# a2a-go SDK - v0.3 → v1.0 갭 분석

> 상위 문서: [a2a-go SDK Overview](./00_overview.md)

## 개요
현재 SDK(v0.3.6, 프로토콜 0.3.0)와 A2A Protocol v1.0 스펙 사이의 갭을 분석한 문서. `origin/release/spec-v1` 브랜치의 진행 상황 포함.

## 현재 상태
- **SDK 버전**: 0.3.6
- **프로토콜 버전**: `a2a/core.go:30` → `const Version ProtocolVersion = "0.3.0"`
- **v1.0 작업 브랜치**: `origin/release/spec-v1`

## 갭 요약 테이블

| # | 갭 | 심각도 | 현재 상태 | 작업 상태 |
|:--|:---|:-------|:---------|:---------|
| 1 | Part 타입 통합 | CRITICAL | TextPart/FilePart/DataPart 분리 | spec-v1 진행중 |
| 2 | Enum 케이싱 | HIGH | kebab-case | spec-v1 진행중 |
| 3 | Kind 필드 제거 | CRITICAL | 명시적 `kind` 문자열 | spec-v1 진행중 |
| 4 | AgentCard 재구조화 | HIGH | url + protocolVersion | 미착수 |
| 5 | 페이지네이션 필드명 | LOW | snake_case | spec-v1 완료 |
| 6 | OAuth 플로우 | MEDIUM | Implicit/Password 존재 | 미착수 |
| 7 | mTLS | LOW | 구조체만 존재 | 부분 완료 |
| 8 | AgentCard JWS 서명 | MEDIUM | 타입 정의만 존재 | 미착수 |
| 9 | 확장 메커니즘 헤더 | MEDIUM | 메타데이터 기반만 | 부분 완료 |
| 10 | 멀티테넌시 | MEDIUM | 필드만 존재 | 미착수 |
| 11 | URL 경로 접두어 | LOW | /v0.3/* | 미착수 |

## 상세 갭 분석

### GAP 1: Part 타입 통합 (CRITICAL)

**현재 (v0.3)**: 3개 분리된 타입
```go
// a2a/core.go
type Part interface { isPart(); Meta() map[string]any }
type TextPart struct { Text string; Metadata map[string]any }
type DataPart struct { Data map[string]any; Metadata map[string]any }
type FilePart struct { File FilePartContent; Metadata map[string]any }
```

**v1.0**: 단일 Part + oneof
```protobuf
message Part {
  oneof content { string text; bytes raw; string url; Value data; }
  Struct metadata; string filename; string media_type;
}
```

- 코드베이스 내 TextPart/FilePart/DataPart 사용: 55+ 곳
- `raw` (바이너리) 타입 새로 추가 필요
- `filename`, `media_type` 필드 Part 레벨로 이동 필요

### GAP 2: Enum 케이싱 (HIGH)

**현재**: kebab-case
```go
TaskStateAuthRequired = "auth-required"
TaskStateInputRequired = "input-required"
```

**v1.0**: SCREAMING_SNAKE_CASE (ProtoJSON 규칙)
```
TASK_STATE_AUTH_REQUIRED
TASK_STATE_INPUT_REQUIRED
```

- `a2a/core.go`, `a2a/auth.go`의 모든 enum 상수 변경 필요
- JSON 직렬화/역직렬화 전부 영향

### GAP 3: Kind 필드 제거 (CRITICAL)

**현재**: 명시적 `kind` 디스크리미네이터
```go
// Event 역직렬화
type typedEvent struct { Kind string `json:"kind"` }
switch te.Kind {
  case "message": case "task": case "status-update": case "artifact-update":
}

// Part 역직렬화
type typedPart struct { Kind string `json:"kind"` }
switch tp.Kind {
  case "text": case "data": case "file":
}
```

**v1.0**: 멤버 존재 여부로 판별 (oneof 스타일)
```go
if _, ok := raw["text"]; ok { /* TextPart */ }
else if _, ok := raw["data"]; ok { /* DataPart */ }
```

- JSON 마샬링/언마샬링 로직 전면 재작성 필요

### GAP 4: AgentCard 재구조화 (HIGH)

**현재**: 최상위 URL + ProtocolVersion
```go
type AgentCard struct {
  URL string
  ProtocolVersion string
  PreferredTransport TransportProtocol
  AdditionalInterfaces []AgentInterface  // 보조 역할
}
```

**v1.0**: `SupportedInterfaces[]`가 주 구조
```go
type AgentCard struct {
  SupportedInterfaces []AgentInterface {
    URL string
    ProtocolBinding string  // "JSONRPC" | "GRPC" | "HTTP+JSON"
    ProtocolVersion string
    Tenant string
  }
  Signatures []AgentCardSignature  // JWS 서명
}
```

- 최상위 `URL`, `ProtocolVersion` 폐기
- 클라이언트 리졸버 전체 수정 필요

### GAP 5: 페이지네이션 필드명 (LOW) — spec-v1 완료

**변경 내용**: snake_case → camelCase
- `page_size` → `pageSize`
- `page_token` → `pageToken`
- `total_size` → `totalSize`
- `next_page_token` → `nextPageToken`
- `context_id` → `contextId`
- `history_length` → `historyLength`

### GAP 6: OAuth 플로우 (MEDIUM)

**현재**: 폐기된 플로우 포함
```go
type OAuthFlows struct {
  AuthorizationCode *AuthorizationCodeOAuthFlow
  ClientCredentials *ClientCredentialsOAuthFlow
  Implicit *ImplicitOAuthFlow     // v1.0에서 제거
  Password *PasswordOAuthFlow     // v1.0에서 제거
}
```

**v1.0**: 현대화
- Implicit, Password 제거 (보안 위험)
- Device Code (RFC 8628) 추가 (CLI/IoT용)
- Authorization Code에 PKCE 필수 옵션 추가

### GAP 7-11: 나머지 갭

| 갭 | 현재 | 필요 작업 |
|:---|:-----|:---------|
| mTLS | `MutualTLSSecurityScheme` 구조체 존재 | Transport 계층 실제 적용 |
| JWS 서명 | `AgentCardSignature` 타입 존재 | RFC 7515 + RFC 8785 검증 로직 |
| 확장 헤더 | 메타데이터 기반 | `A2A-Extensions` HTTP 헤더 파싱/설정 |
| 멀티테넌시 | 필드만 존재 | `/{tenant}/` URL 경로 매핑 |
| URL 경로 | `/v0.3/*` | 버전 접두어 제거 |

## spec-v1 브랜치 진행 현황

### 완료된 작업
- 페이지네이션 필드명 camelCase 전환
- REST Transport 신규 구현 (`a2aclient/rest.go`, 316줄)
- Proto 변환 유틸리티 (`a2apb/pbconv/`)
- Auth 개선 (클라이언트/서버)
- AgentExecutor 인터페이스 개선
- CallInterceptor early return 지원

### 미완료 작업
- Part 타입 통합
- Enum SCREAMING_SNAKE_CASE 전환
- Kind 필드 제거
- AgentCard 재구조화 + JWS 서명
- OAuth Device Code 플로우
- 멀티테넌시 URL 매핑
- 확장 HTTP 헤더

## Critical Blockers (v1.0 릴리스 차단)

1. **Part 타입 통합** — 모든 메시지 처리에 영향
2. **Enum 케이싱** — 모든 JSON 직렬화에 영향
3. **Kind 필드 제거** — 언마샬 로직 전면 재작성
4. **AgentCard 재구조화** — 하위 호환 불가

## 권장 마이그레이션 경로

```
Phase 1: 타입 시스템 (가장 파급력 큼)
├── Part 통합
├── Enum 케이싱
└── Kind 필드 제거

Phase 2: AgentCard + 보안
├── AgentCard → SupportedInterfaces
├── JWS 서명 검증
├── Device Code OAuth
└── mTLS Transport 적용

Phase 3: 전송 계층
├── 확장 HTTP 헤더
├── 멀티테넌시 URL
└── URL 경로 접두어 제거

Phase 4: 릴리스
├── 마이그레이션 가이드
├── v0.3.x 디프리케이션
└── v1.0.0 릴리스
```

## 관련 항목
- [SSE 재연결 메커니즘](./04_sse-reconnection.md)
- [GitHub 이슈/PR 현황](./05_github-status.md)

## 참고 자료
- [A2A Protocol Specification v1.0](https://a2a-protocol.org/latest/specification/)
- [a2a-go release/spec-v1 브랜치](https://github.com/a2aproject/a2a-go/tree/release/spec-v1)
- [ADR-001: ProtoJSON Serialization](https://github.com/a2aproject/A2A/blob/main/adrs/adr-001-protojson-serialization.md)

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
