# a2a-rs Architecture - Overview

## 개요
- **목적**: A2A(Agent2Agent) Protocol v1.0의 공식 수준 Rust SDK 구현
- **범위**: Core types, Server SDK (JSON-RPC + SSE), Client SDK (JSON-RPC + SSE), In-memory stores
- **포팅 원본**: [a2a-go SDK](https://github.com/a2aproject/a2a-go) v0.3.6 + v1.0 스펙 직접 타겟팅
- **프로젝트명**: `a2a-rs` (crate name: `a2a-types`, `a2a-server`, `a2a-client`)

## 문서 구성
1. [컴포넌트 설계](./01_components.md) - Cargo workspace, 크레이트 구조, 의존성
2. [데이터 모델](./02_data-model.md) - Go → Rust 핵심 타입 매핑, serde 전략
3. [서버 아키텍처](./03_server-architecture.md) - axum 기반 서버, JSON-RPC, SSE, trait 설계
4. [클라이언트 아키텍처](./04_client-architecture.md) - reqwest 기반 클라이언트, SSE 파서, transport 추상화

## 아키텍처 원칙

1. **Rust-native 설계**: Go의 패턴을 그대로 옮기지 않고, Rust의 강점(enum, ownership, trait)을 적극 활용
2. **v1.0 스펙 직접 타겟팅**: Go SDK의 v0.3 코드가 아닌 A2A Protocol v1.0 스펙을 Source of Truth으로
3. **Result<T, E> 패턴**: panic 금지, 모든 에러를 명시적으로 전파
4. **Zero-cost abstractions**: trait object는 필요한 곳에서만, 가능하면 제네릭 + static dispatch
5. **최소 의존성**: 필요 최소한의 외부 크레이트만 사용
6. **Sealed enum**: Go의 sealed interface → Rust의 `#[non_exhaustive]` enum으로 자연스럽게 매핑

## 핵심 결정 사항

| 결정 | 근거 | 대안 | 상세 문서 |
|:-----|:-----|:-----|:----------|
| Cargo workspace (3 crates) | 관심사 분리, 독립 컴파일, 선택적 의존 | 단일 크레이트 + feature flags | [01_components](./01_components.md) |
| axum (서버) | tower 에코시스템, 현대적 API, axum::extract | actix-web, warp | [03_server](./03_server-architecture.md) |
| reqwest (클라이언트) | tokio 네이티브, streaming 지원 | hyper 직접 사용 | [04_client](./04_client-architecture.md) |
| serde + `#[serde(tag)]` | ProtoJSON 호환 직렬화, 커스텀 Visitor 최소화 | prost (proto 직접 생성) | [02_data-model](./02_data-model.md) |
| `Pin<Box<dyn Stream>>` (스트리밍) | async 호환, 표준 패턴 | channel-based | [03_server](./03_server-architecture.md) |
| In-memory only (v0.1) | 복잡도 최소화, MVP 우선 | SQL 백엔드 동시 구현 | [01_components](./01_components.md) |

## 기술 스택 요약

| 영역 | 기술 | 버전 |
|:-----|:-----|:-----|
| Language | Rust | 2021 edition |
| Async Runtime | tokio | 1.x (full features) |
| HTTP Server | axum | 0.8.x |
| HTTP Client | reqwest | 0.12.x |
| Serialization | serde + serde_json | 1.x |
| UUID | uuid | 1.x (v4, serde) |
| Logging | tracing | 0.1.x |
| Error | thiserror | 2.x |
| Streaming | tokio-stream + futures-core | latest |
| Bytes | bytes | 1.x |

## 시스템 컨텍스트

```
┌──────────────────────────────────────────────────────────────┐
│                      User Application                        │
│  (에이전트 개발자가 작성하는 비즈니스 로직)                      │
└───────┬──────────────────────────────────┬───────────────────┘
        │ impl AgentExecutor               │ use A2AClient
        ▼                                  ▼
┌────────────────────┐          ┌────────────────────┐
│   a2a-server       │          │   a2a-client       │
│                    │          │                    │
│ ┌────────────────┐ │          │ ┌────────────────┐ │
│ │ RequestHandler │ │  HTTP    │ │ Transport      │ │
│ │ (default impl) │◄├──────────┤►│ (JSON-RPC)     │ │
│ ├────────────────┤ │  SSE     │ ├────────────────┤ │
│ │ JSON-RPC Layer │ │          │ │ SSE Parser     │ │
│ ├────────────────┤ │          │ ├────────────────┤ │
│ │ SSE Writer     │ │          │ │ AgentCard      │ │
│ ├────────────────┤ │          │ │ Resolver       │ │
│ │ EventQueue     │ │          │ └────────────────┘ │
│ │ TaskStore      │ │          │                    │
│ └────────────────┘ │          │                    │
└────────┬───────────┘          └────────┬───────────┘
         │                               │
         └───────────┬───────────────────┘
                     ▼
          ┌────────────────────┐
          │   a2a-types        │
          │                    │
          │ Task, Message,     │
          │ Event, Part,       │
          │ AgentCard,         │
          │ Error codes        │
          └────────────────────┘
```

## 참조 리서치
- `.docs/research/a2a-protocol-analysis.md` - A2A Protocol v1.0 전체 분석
- `.docs/research/a2a-go-sdk/00_overview.md` ~ `05_github-status.md` - Go SDK 분석
- `.docs/plans/a2a-rs-implementation-plan.md` - 초기 구현 계획

## Scope (v0.1.0)

### In Scope
- Core domain types (A2A Protocol v1.0 스펙 준수)
- Server SDK: JSON-RPC + SSE streaming (axum)
- Client SDK: JSON-RPC + SSE parsing (reqwest)
- In-memory TaskStore, EventQueue
- AgentCard serving (well-known URI) + resolving
- Hello world example (server + client)

### Out of Scope (future versions)
- gRPC transport (tonic) → v0.2
- Distributed/cluster mode → v0.3
- Push notifications (webhook) → v0.2
- OpenTelemetry extensions → v0.3
- mTLS, OAuth implementation → v0.2
- Agent Card JWS signatures → v0.3
- Multi-tenancy URL routing → v0.3

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
