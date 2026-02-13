# a2a-go SDK - Overview

## 개요
- **정의**: A2A(Agent2Agent) 프로토콜의 공식 Go SDK. 에이전트 간 통신/협업을 위한 서버·클라이언트 라이브러리
- **모듈**: `github.com/a2aproject/a2a-go`
- **Go 버전**: 1.24.4
- **현재 버전**: 0.3.6
- **라이선스**: Apache License 2.0
- **관리**: Linux Foundation 산하 오픈소스 프로젝트, Google이 기여

## 핵심 개념
- **Opaque Agent 패턴**: 에이전트 내부(메모리, 도구, 로직)를 노출하지 않고 선언된 능력과 교환된 컨텍스트만으로 협업
- **Transport-agnostic 설계**: 코어 도메인 타입과 전송 계층을 분리하여 JSON-RPC, gRPC 동시 지원
- **Local/Distributed 모드**: 단일 프로세스 실행과 분산 실행(MySQL + 외부 큐) 모두 지원

## 문서 구성
1. [01_tech-stack](./01_tech-stack.md) - 기술 스택 (의존성, 도구 체인, CI/CD)
2. [02_folder-architecture](./02_folder-architecture.md) - 폴더 구조 및 아키텍처 (패키지 구조, 설계 패턴, 핵심 인터페이스)
3. [03_v1-gap-analysis](./03_v1-gap-analysis.md) - v0.3 → v1.0 갭 분석 (11개 갭, 마이그레이션 로드맵)
4. [04_sse-reconnection](./04_sse-reconnection.md) - SSE 재연결/복구 메커니즘 심화 (구현 상세, 3개 주요 갭)
5. [05_github-status](./05_github-status.md) - GitHub 이슈/PR 현황 (28개 이슈, v1.0 마일스톤, 프로젝트 방향)

## 핵심 요약

### 아키텍처 계층
```
┌─────────────────────────────────────┐
│         Transport Layer             │
│   (JSON-RPC / gRPC / SSE)          │
├─────────────────────────────────────┤
│       Server/Client SDK             │
│   (a2asrv / a2aclient)             │
├─────────────────────────────────────┤
│       Core Domain Types             │
│   (a2a - Task, Message, Event)      │
├─────────────────────────────────────┤
│       Internal Infrastructure       │
│   (taskexec, taskstore, eventpipe)  │
└─────────────────────────────────────┘
```

### 지원 전송 프로토콜
| 바인딩 | 전송 | 직렬화 | 스트리밍 |
|:-------|:-----|:-------|:---------|
| JSON-RPC 2.0 | HTTP(S) | ProtoJSON | SSE |
| gRPC | HTTP/2 | Protocol Buffers binary | gRPC Server Streaming |

### 실행 모드
| 모드 | Task Store | Event Queue | Work Queue | 용도 |
|:-----|:-----------|:------------|:-----------|:-----|
| Local | In-memory | In-memory | 없음 | 개발/단일 인스턴스 |
| Distributed | MySQL | 외부 큐 | 외부 큐 | 프로덕션/클러스터 |

## 참고 자료
- [a2a-go GitHub](https://github.com/a2aproject/a2a-go)
- [A2A Protocol 공식 사이트](https://a2a-protocol.org/latest/)
- [A2A Protocol SDK 문서](https://a2a-protocol.org/latest/sdk/)
- [a2a-go 소개 (Medium)](https://medium.com/@guoqizhou123123/introducing-a2a-go-the-go-implementation-of-a2a-protocol-605131cb837c)

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
