# a2a-go SDK - SSE 재연결/복구 메커니즘 심화

> 상위 문서: [a2a-go SDK Overview](./00_overview.md)

## 개요
a2a-go의 SSE(Server-Sent Events) 스트리밍 구현, 재연결 메커니즘, 현재 갭을 분석한 문서.

## SSE 구현 아키텍처

### 전체 이벤트 흐름
```
[AgentExecutor]
      │ event 생성
      ▼
[eventqueue.Queue]     Write → 브로드캐스트
      │
      ▼
[subscription.Events]  Read (iter.Seq2)
      │
      ▼
[jsonrpcHandler]       JSON-RPC 응답으로 래핑
      │
      ▼
[SSEWriter]            SSE 프레임 전송 (id + data)
      │
      ▼
[HTTP Response]        text/event-stream
      │
      ▼
[Client: ParseDataStream]  SSE 파싱 → Event 역직렬화
```

## 서버 측: SSEWriter (`internal/sse/sse.go`)

### 헤더 설정
```
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive
X-Accel-Buffering: no    ← 프록시 버퍼링 방지
```

### 이벤트 전송 형식
```
id: <UUID>\n
data: <JSON payload>\n
\n
```
- 각 이벤트마다 `uuid.NewString()`으로 고유 ID 생성
- 데이터는 JSON 직렬화된 `StreamResponse`

### Keep-Alive
```
: keep-alive\n
\n
```
- 설정 가능한 간격 (`WithKeepAlive()` 옵션)
- 게이트웨이/프록시 유휴 타임아웃 방지
- SSE 주석 형식 (`:` 접두어)

### 클라이언트 파서 (`ParseDataStream`)
- `bufio.Scanner` 기반 라인별 파싱
- 최대 토큰 사이즈: 10MB (대형 페이로드 지원)
- `data: value` 및 `data:value` 두 형식 모두 지원
- **주요 발견**: `id:` 필드를 완전히 무시함 (파싱하지 않음)

## 스트리밍 엔드포인트

### `message/stream` (메시지 스트리밍)
```
Client ──POST /message:stream──► Server
Client ◄── SSE: Task (SUBMITTED)
Client ◄── SSE: StatusUpdate (WORKING)
Client ◄── SSE: ArtifactUpdate (chunk)
Client ◄── SSE: StatusUpdate (COMPLETED)
```
- `a2asrv/jsonrpc.go`의 `handleStreamingRequest()` 처리
- 별도 고루틴에서 이벤트 처리
- Keep-alive 틱커 옵션 적용
- 연결 종료 조건: 클라이언트 컨텍스트 취소, 패닉, 쓰기 실패, 시퀀스 완료

### `tasks/resubscribe` (태스크 재구독)
```
Client ──POST /tasks/resubscribe──► Server
Client ◄── SSE: (이전 이벤트부터 스트리밍 재개)
```
- 동일한 `handleStreamingRequest()` 핸들러 사용
- 내부적으로 `OnResubscribeToTask` 호출

## 이벤트 큐 시스템

### Queue 인터페이스 (`a2asrv/eventqueue/`)
```go
type Queue interface {
    Write(event a2a.Event) error
    Read(ctx context.Context) iter.Seq2[a2a.Event, error]
    Close() error
}
```

### InMemory 구현
- **Pub/Sub 브로커 패턴**: 고루틴 기반 메시지 브로드캐스트
- 이벤트를 모든 등록된 큐에 전달 (발신자 제외)
- 버퍼 채널 (기본 32, 설정 가능)
- **영속성 없음** — 메모리에만 존재, 구독 활성 동안만 유지
- 큐 파괴 시 모든 이벤트 소멸

### Manager 패턴
```go
type Manager interface {
    GetOrCreate(taskID string) Queue  // 기존 큐 반환 또는 새로 생성
    Get(taskID string) Queue          // 기존 큐만 반환
    Destroy(taskID string)            // 모든 구독 닫기
}
```

## 구독 타입 (`internal/taskexec/subscription.go`)

### localSubscription (단일 프로세스)
- 이벤트 큐에서 직접 읽기
- 스냅샷 기능 없음 — 현재 시점부터만 수신
- 실행 완료 후 큐 즉시 파괴

### remoteSubscription (분산/클러스터)
- 구독 시 태스크 스냅샷 로드
- 스냅샷 이벤트 먼저 재생 → 이후 라이브 이벤트
- `TaskVersion` 비교로 오래된 이벤트 필터링 (중복 방지)
- **로컬보다 복구 능력 우수**

## 재구독 메커니즘

### Local Manager (`local_manager.go:114-126`)
```
1. 활성 실행 맵에서 조회
2. Manager.Get()으로 기존 큐 가져오기
3. 같은 큐에 연결된 새 구독 반환

제한: 실행 완료 시 큐 파괴됨 → 재구독 실패
```

### Distributed Manager (`distributed_manager.go:62-71`)
```
1. TaskStore에서 태스크 존재 확인
2. GetOrCreate()로 큐 생성
3. 스냅샷 + 스트리밍 remoteSubscription 반환

장점: 태스크가 존재하는 한 재구독 가능
```

## 주요 갭 분석

### GAP 1: Last-Event-ID 미사용 (MAJOR)

| 항목 | 상태 |
|:-----|:-----|
| 서버: SSE ID 생성 | ✅ UUID로 생성 |
| 클라이언트: `id:` 파싱 | ❌ 완전 무시 |
| `Last-Event-ID` 헤더 전송 | ❌ 미구현 |
| 서버: `Last-Event-ID` 처리 | ❌ 미구현 |

**문제**: SSE 스펙은 `Last-Event-ID` 헤더로 "이 이벤트 이후부터 재개"를 지원하지만, 서버가 ID를 생성하면서도 클라이언트가 이를 추적하지 않음. 재연결 시 이벤트 위치 지정 불가.

### GAP 2: 이벤트 비영속 (MAJOR)

| 항목 | 상태 |
|:-----|:-----|
| 이벤트 인메모리 저장 | ✅ 구독 활성 중 |
| 이벤트 디스크 저장 | ❌ 미구현 |
| 큐 파괴 후 이벤트 접근 | ❌ 불가 |
| 서버 재시작 후 복구 | ❌ 불가 |

**문제**: 로컬 모드에서 실행 완료 → 큐 즉시 파괴 → 이벤트 소멸. 클라이언트가 완료 이벤트를 놓치면 복구 방법 없음.

### GAP 3: 큐 조기 파괴 (MAJOR)

```go
// local_manager.go:341-346
func (m *localManager) destroyQueue(taskID string) {
    // 실행 완료 즉시 큐 파괴
    // 재구독 시도 전에 파괴될 수 있음
}
```

**문제**: 실행 완료와 큐 파괴 사이에 레이스 컨디션. 클라이언트가 연결 끊김 후 재구독하려 하면 이미 큐가 없음.

### GAP 4: 단일 사용 구독

```go
// subscription.go:47
s.consumed = true  // 한 번 이터레이션 후 재사용 불가
```
- `Manager.Resubscribe()`가 새 구독을 반환하므로 우회 가능
- 하지만 추가 왕복 필요

## 현재 재연결 역량 매트릭스

| 기능 | Local 모드 | Distributed 모드 |
|:-----|:----------|:----------------|
| 실행 중 재구독 | ✅ | ✅ |
| 완료 후 재구독 | ❌ (큐 파괴) | ✅ (스냅샷) |
| 스냅샷 복구 | ❌ | ✅ |
| 버전 기반 중복 제거 | ❌ | ✅ |
| Last-Event-ID 이력 재생 | ❌ | ❌ |
| 자동 재연결 | ❌ | ❌ |
| Keep-Alive | ✅ (설정 가능) | ✅ (설정 가능) |
| 이벤트 영속성 | ❌ | ❌ (TaskStore만 최종 상태) |
| 이벤트 순서 보장 (FIFO) | ✅ (태스크 내) | ✅ (태스크 내) |

## 연결 끊김 시 동작

```
1. 네트워크 끊김
2. SSE 스트림 닫힘 (io.EOF)
3. ParseDataStream → error 반환
4. 클라이언트 코드에 에러 전파
5. 클라이언트가 수동으로 ResubscribeToTask 호출 필요

⚠ 끊김과 재구독 사이에 발생한 이벤트는 유실 위험
```

## 클라이언트 타임아웃
- HTTP 클라이언트 기본 타임아웃: 3분 (`a2aclient/jsonrpc.go:82`)
- 지수 백오프 없음
- Transport 계층에 재시도 로직 없음

## 관련 이슈

- **Issue #124**: 지연이 있는 다중 스트리밍 메시지에서 컨텍스트 취소 에러 (버그, 해결 중)
- **Issue #196**: 재구독 시 첫 이벤트로 Task 스냅샷 방출 (v1.0 마일스톤)
- **PR #191**: 스트리밍 예제 추가 (올바른 `Append: true` 패턴)

## 관련 항목
- [v1.0 갭 분석](./03_v1-gap-analysis.md)
- [GitHub 이슈/PR 현황](./05_github-status.md)

## 참고 자료
- [SSE 스펙 (HTML Living Standard)](https://html.spec.whatwg.org/multipage/server-sent-events.html)
- [internal/sse/sse.go](../../internal/sse/sse.go) — SSEWriter/ParseDataStream 구현
- [a2asrv/jsonrpc.go](../../a2asrv/jsonrpc.go) — 스트리밍 핸들러
- [internal/taskexec/subscription.go](../../internal/taskexec/subscription.go) — 구독 관리

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
