# a2a-rs Implementation Plan - Overview

## 개요
- **목적**: A2A Protocol v1.0의 Rust SDK 구현 (a2a-go 포팅)
- **아키텍처**: [.docs/architecture/260212_a2a-rs/](../../architecture/260212_a2a-rs/00_overview.md)
- **목표 버전**: v0.1.0

## 서브태스크 목록

### Phase 1: Foundation (순차 - 먼저 완료해야 함)
1. [01_workspace-setup](./01_workspace-setup.md) - Cargo workspace + Cargo.toml 구성
2. [02_a2a-types](./02_a2a-types.md) - 코어 도메인 타입 크레이트 전체 구현

### Phase 2: SDK 구현 (병렬 가능 - a2a-types 완료 후)
3. [03_a2a-server](./03_a2a-server.md) - 서버 SDK (axum, JSON-RPC, SSE, traits)
4. [04_a2a-client](./04_a2a-client.md) - 클라이언트 SDK (reqwest, SSE parser, transport)

### Phase 3: Integration (Phase 2 완료 후)
5. [05_examples-and-tests](./05_examples-and-tests.md) - Hello world 예제 + 통합 테스트

## 의존성 그래프

```
Phase 1: [01_workspace] → [02_a2a-types]
                                │
                    ┌───────────┴───────────┐
                    ▼                       ▼
Phase 2:    [03_a2a-server]         [04_a2a-client]
                    │                       │
                    └───────────┬───────────┘
                                ▼
Phase 3:            [05_examples-and-tests]
```

## 팀 구성 (Swarm)

| 역할 | 담당 | Phase |
|:-----|:-----|:------|
| **coordinator** (리더) | workspace setup + a2a-types + 통합 | 1, 3 |
| **server-impl** | a2a-server crate 전체 | 2 |
| **client-impl** | a2a-client crate 전체 | 2 |

## 전체 목표
- [ ] Cargo workspace 구성 (3 crates + examples)
- [ ] a2a-types: A2A v1.0 전체 타입 + serde + JSON-RPC types
- [ ] a2a-server: axum JSON-RPC handler + SSE + EventQueue + TaskStore
- [ ] a2a-client: reqwest transport + SSE parser + AgentCard resolver
- [ ] Hello world example (server + client 통신)
- [ ] `cargo check` + `cargo clippy` 통과
- [ ] 기본 단위 테스트

## 완료 기준
1. `cargo build --workspace` 성공
2. `cargo test --workspace` 성공
3. `cargo clippy --workspace -- -D warnings` 경고 없음
4. examples/helloworld가 서버-클라이언트 통신 가능

---
*작성일: 2026-02-12*
