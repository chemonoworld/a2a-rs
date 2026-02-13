# a2a-go SDK - GitHub 이슈/PR 현황

> 상위 문서: [a2a-go SDK Overview](./00_overview.md)

## 개요
a2a-go 레포지토리(a2aproject/a2a-go)의 오픈 이슈, PR, 최근 커밋, 프로젝트 방향성을 정리한 문서.

## 프로젝트 활동 요약
- **오픈 이슈**: 28개
- **오픈 PR**: 8개
- **최근 릴리스**: v0.3.7 (2026-02-04)
- **활동도**: 매우 활발 (수일 간격 커밋)
- **릴리스 관리**: Release Please 자동화

## v1.0.0 마일스톤 (11개 이슈)

| 이슈 | 제목 | 유형 |
|:-----|:-----|:-----|
| #199 | Protocol version negotiation for a2aclient.Factory | Feature |
| #198 | Capability verifier requestHandler option | Feature |
| #197 | Allow HistoryLength to be unspecified | Feature |
| #196 | Emit Task snapshot as first event on resubscription | Feature |
| #195 | Add agent emitted event validations | Feature |
| #193 | Update proto converters | Task |
| #192 | Update core type discriminator handling | Task |

### 주요 Breaking API 변경 PR
- **PR #204**: AgentExecutor API 분리 — `eventqueue.Queue`를 Reader/Writer로 분리, `eventqueue.Message` 도입
- **PR #201**: a2aclient API 개선 — `CallMeta` → `ServiceParams` 리네이밍, Transport API 통합

## 주요 오픈 이슈

### 버그
| 이슈 | 제목 | 상태 |
|:-----|:-----|:-----|
| #200 | Context propagation in cluster mode | 오픈 — Auth 정보와 요청 메타데이터가 workqueue/분산 실행에서 유실 |
| #124 | Context canceled error with multiple streaming messages | 오픈 — 지연 스트리밍 패턴 문제, PR #191로 해결 중 |

### 기능/개선
| 이슈 | 제목 | 상태 |
|:-----|:-----|:-----|
| #199 | 프로토콜 버전 협상 | v1.0 마일스톤 |
| #198 | 기능 검증기 핸들러 옵션 | v1.0 마일스톤 |
| #196 | 재구독 시 Task 스냅샷 방출 | v1.0 마일스톤 |
| #129 | Method extensions | PR #138 진행중 |

## 오픈 PR

| PR | 제목 | 상태 | 내용 |
|:---|:-----|:-----|:-----|
| #204 | Breaking AgentExecutor API changes | WIP | AgentExecutor ↔ eventqueue 분리 |
| #201 | Breaking a2aclient API improvements | WIP | CallMeta → ServiceParams |
| #191 | Streaming example | 리뷰 대기 | 올바른 ArtifactUpdate 패턴 데모 |
| #190 | Input-required state handling example | 리뷰 대기 | 멀티턴 상호작용 예제 |
| #189 | Release 0.3.7 | 머지 대기 | 자동 릴리스 |
| #182 | Factor out DB logic from cluster example | WIP | 클러스터 모드 리팩토링 |
| #138 | Method extensions prototyping | WIP | 커스텀 RPC 메서드 확장 |

## 최근 주요 커밋 (시간순)

| 커밋 | 내용 | 관련 PR |
|:-----|:-----|:--------|
| `6657a6d` | SSE: `data:` 접두어 (공백 없이) 지원 | #188 |
| `b55fbfd` | 내부 에러: TCK 실패 처리 | #186 |
| `d3af3bf` | 커스텀 패닉 핸들러 | #185 |
| `535bb1f` | 퍼블릭 AgentCard 서빙 시 Origin 반영 | #183 |
| 이전 | Keep-alive 옵션, WorkQueue 구현, DB 클러스터 | #171, #132, #133 |

## 활발한 기여자

| 이름 | 역할 | 주요 작업 |
|:-----|:-----|:---------|
| **yarolegovich** | 메인 메인테이너 | API 설계, breaking changes 리드 |
| **lbobinski** | 기여자 | 스트리밍/input-required 예제 |
| **gaborfeher** | 기여자 | 클러스터 모드 DB 리팩토링 |
| **a2a-bot** | 자동화 | Release Please 릴리스 |

## 컨트리뷰션 패턴

1. **브랜치 전략**: 기능 브랜치 (`yarolegovich/break-agentexec`)
2. **타겟 브랜치**: main (일반), release/spec-v1 (breaking changes)
3. **커밋 컨벤션**: Conventional Commits 필수 (CI 검증)
4. **릴리스**: Release Please 자동 관리
5. **예제 중심 문서화**: `examples/` 디렉토리에 패턴 데모

## 프로젝트 방향성 분석

### 단기 (현재 진행)
- AgentExecutor/Client API breaking changes 정리
- 스트리밍 예제/패턴 문서화
- 클러스터 모드 안정화

### 중기 (v1.0 마일스톤)
- 프로토콜 버전 협상
- 재구독 시 Task 스냅샷
- 이벤트 검증 강화
- 코어 타입 디스크리미네이터 업데이트

### 장기
- spec-v1 브랜치의 완전한 v1.0 프로토콜 지원
- Method Extensions 표준화
- 클러스터 모드 프로덕션 레벨 성숙

## SSE/스트리밍 관련 현황

- **#188 (머지됨)**: `data:` 접두어 공백 없이도 파싱 가능하도록 수정
- **#191 (오픈)**: 스트리밍 예제 — `TaskArtifactUpdateEvent`의 `Append: true` 올바른 사용법
- **#124 (오픈)**: 시간 지연이 있는 다중 스트리밍 메시지 문제
- **#196 (오픈)**: 재구독 시 첫 이벤트로 Task 스냅샷 방출 (v1.0)

## 관련 항목
- [v1.0 갭 분석](./03_v1-gap-analysis.md)
- [SSE 재연결 메커니즘](./04_sse-reconnection.md)

## 참고 자료
- [a2a-go Issues](https://github.com/a2aproject/a2a-go/issues)
- [a2a-go Pull Requests](https://github.com/a2aproject/a2a-go/pulls)
- [a2a-go CHANGELOG](https://github.com/a2aproject/a2a-go/blob/main/CHANGELOG.md)

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
