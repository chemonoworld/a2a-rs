# a2a-go SDK - 기술 스택

> 상위 문서: [a2a-go SDK Overview](./00_overview.md)

## 개요
a2a-go 프로젝트에서 사용된 언어, 의존성, 도구 체인, CI/CD 파이프라인을 정리한 문서.

## 언어 및 런타임

| 항목 | 값 |
|:-----|:---|
| 언어 | Go |
| Go 버전 | 1.24.4 |
| 모듈 시스템 | Go Modules (`go.mod`) |
| 최소 Go 기능 | `iter.Seq2` (Go 1.24+ range-over-func) |

## 직접 의존성 (Direct Dependencies)

| 패키지 | 버전 | 용도 |
|:-------|:-----|:-----|
| `google.golang.org/grpc` | v1.73.0 | gRPC 전송 계층 |
| `google.golang.org/protobuf` | v1.36.6 | Protocol Buffers 직렬화 |
| `google.golang.org/genproto/googleapis/api` | v0.0.0-20250715 | Google API 어노테이션 (HTTP 매핑) |
| `github.com/google/uuid` | v1.6.0 | UUID 생성 (Task ID, Message ID 등) |
| `github.com/go-sql-driver/mysql` | v1.9.3 | MySQL 드라이버 (분산 모드 Task Store) |
| `github.com/google/go-cmp` | v0.7.0 | 테스트 비교 유틸리티 |
| `golang.org/x/sync` | v0.15.0 | 동시성 프리미티브 (`errgroup` 등) |

## 간접 의존성 (Indirect Dependencies)

| 패키지 | 용도 |
|:-------|:-----|
| `filippo.io/edwards25519` | 암호화 (MySQL 인증) |
| `golang.org/x/net` | 네트워크 유틸리티 |
| `golang.org/x/sys` | 시스템 콜 |
| `golang.org/x/text` | 텍스트 처리 |
| `google.golang.org/genproto/googleapis/rpc` | gRPC 상태 코드 |

> 의존성이 매우 가볍다 (직접 7개, 간접 5개). 표준 라이브러리를 최대한 활용하는 설계.

## Protocol Buffers 도구 체인

### 코드 생성 (buf.gen.yaml)
```
a2aproject/A2A (공식 스펙 레포)
    │
    │  specification/grpc/a2a.proto
    │
    ▼
buf generate
    │
    ├── protoc-gen-go       → a2apb/a2a.pb.go      (~140KB, 메시지 정의)
    └── protoc-gen-go-grpc  → a2apb/a2a_grpc.pb.go  (~26KB, gRPC 서비스)
```

### 필요 도구
```bash
go install github.com/bufbuild/buf/cmd/buf@latest
go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest
```

### Proto 소스
- 레포: `https://github.com/a2aproject/A2A.git`
- 참조 커밋: `e7cf203ecdd7003c0f1740bb712248d2b5252bf1`
- 경로: `specification/grpc/`

## 린팅 및 코드 품질

### golangci-lint (v2.3.1)
`.golangci.yml` 설정:
- **포맷터**: `goimports` (import 정렬)
- **린터**: `goheader` (Apache 2.0 라이선스 헤더 검증)
- **정적 분석**: `staticcheck` (전체 활성화, 일부 스타일 규칙 제외)
  - 제외: ST1000(패키지 주석), ST1003(네이밍), ST1016(리시버 네이밍), ST1020-1022(주석 스타일)
  - 제외: QF1001(드모르간 법칙), QF1008(임베디드 필드)

## 테스트

### 테스트 실행 명령
```bash
go test -race -mod=readonly -v -count=1 -shuffle=on ./...
```

| 플래그 | 목적 |
|:-------|:-----|
| `-race` | 데이터 레이스 감지 |
| `-mod=readonly` | 의존성 변경 검증 |
| `-v` | 상세 출력 |
| `-count=1` | 캐싱 없이 매번 실행 |
| `-shuffle=on` | 테스트 순서 랜덤화 |

### 테스트 규모
- **총 테스트 파일**: ~39개 `*_test.go`
- **테스트 유틸**: `internal/testutil/` (Mock 구현체 - queue, task store, work queue, push sender, executor)
- **E2E 테스트**: `e2e/tck/` (Technology Compatibility Kit)
  - Python 오케스트레이터 (`orchestrate_tck.py`)
  - Shell 러너 (`run_tck.sh`)
  - Go SUT 구현체 (`sut.go`, `sut_agent_executor.go`)

### 테스트 컨벤션 (TESTING.md)
- `google/go-cmp`로 비교 (reflect.DeepEqual 대신)
- `t.Parallel()` 사용 권장
- 기존 테스트를 레퍼런스로 활용

## CI/CD 파이프라인 (GitHub Actions)

| 워크플로우 | 트리거 | 목적 |
|:-----------|:-------|:-----|
| `go.yaml` | push to main, PR | Go 테스트 + golangci-lint |
| `nightly.yaml` | 스케줄 | 야간 자동 테스트 |
| `release-please.yaml` | push to main | 자동 릴리스 관리 |
| `validate-pr-title.yaml` | PR | Conventional Commit 타이틀 검증 |

### CI 환경 설정
- `.github/actions/setup/action.yml`: 일관된 CI 환경 구성 커스텀 액션

## 릴리스 관리

- **도구**: [release-please](https://github.com/google-github-actions/release-please-action)
- **설정**: `.release-please-manifest.json`
- **버전 히스토리**: `CHANGELOG.md`
- **컨벤션**: [Conventional Commits](https://www.conventionalcommits.org/) 필수

## 관련 항목
- [폴더 구조 및 아키텍처](./02_folder-architecture.md)

## 참고 자료
- [a2a-go go.mod](https://github.com/a2aproject/a2a-go/blob/main/go.mod)
- [buf.build - Protocol Buffers 도구](https://buf.build/)
- [golangci-lint](https://golangci-lint.run/)

---
*작성일: 2026-02-12*
*최종 수정: 2026-02-12*
