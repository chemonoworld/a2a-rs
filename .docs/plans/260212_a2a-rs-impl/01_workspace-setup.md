# Workspace Setup

## 개요
- **상위 태스크**: [Overview](./00_overview.md)
- **목적**: Cargo workspace 루트 + 3개 크레이트 디렉토리 + examples 스켈레톤 생성
- **담당**: coordinator

## 목표
- [ ] Workspace root Cargo.toml
- [ ] crates/a2a-types/Cargo.toml + src/lib.rs
- [ ] crates/a2a-server/Cargo.toml + src/lib.rs
- [ ] crates/a2a-client/Cargo.toml + src/lib.rs
- [ ] examples/helloworld/ 스켈레톤
- [ ] `cargo check --workspace` 통과

## 구현 계획

### 1. 디렉토리 구조 생성
```
columbus/
├── Cargo.toml              # workspace root
├── crates/
│   ├── a2a-types/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── a2a-server/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── a2a-client/
│       ├── Cargo.toml
│       └── src/lib.rs
└── examples/
    └── helloworld/
        ├── server.rs
        └── client.rs
```

### 2. Workspace Cargo.toml
```toml
[workspace]
resolver = "2"
members = [
    "crates/a2a-types",
    "crates/a2a-server",
    "crates/a2a-client",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
rust-version = "1.75"

[workspace.dependencies]
# Internal
a2a-types = { path = "crates/a2a-types" }
a2a-server = { path = "crates/a2a-server" }
a2a-client = { path = "crates/a2a-client" }

# Shared
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
thiserror = "2"
tracing = "0.1"
uuid = { version = "1", features = ["v4", "serde"] }
bytes = "1"
futures-core = "0.3"
tokio-stream = "0.1"

# Server
axum = { version = "0.8" }

# Client
reqwest = { version = "0.12", features = ["json", "stream"] }

# Dev
tokio-test = "0.4"
```

### 3. 각 Crate Cargo.toml

**a2a-types:**
```toml
[package]
name = "a2a-types"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
thiserror.workspace = true
```

**a2a-server:**
```toml
[package]
name = "a2a-server"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
a2a-types.workspace = true
axum.workspace = true
tokio.workspace = true
tokio-stream.workspace = true
futures-core.workspace = true
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
tracing.workspace = true
thiserror.workspace = true
bytes.workspace = true
```

**a2a-client:**
```toml
[package]
name = "a2a-client"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
a2a-types.workspace = true
reqwest.workspace = true
tokio.workspace = true
tokio-stream.workspace = true
futures-core.workspace = true
serde.workspace = true
serde_json.workspace = true
tracing.workspace = true
thiserror.workspace = true
bytes.workspace = true
```

### 4. 초기 lib.rs (각 크레이트)
빈 lib.rs로 시작. `cargo check` 통과만 목표.

## 관련 파일
- `.docs/architecture/260212_a2a-rs/01_components.md`

---
*작성일: 2026-02-12*
