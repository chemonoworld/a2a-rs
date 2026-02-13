# a2a-types Crate Implementation

## 개요
- **상위 태스크**: [Overview](./00_overview.md)
- **목적**: A2A Protocol v1.0의 모든 데이터 타입 정의. transport-agnostic, serde 기반 직렬화.
- **담당**: coordinator (Phase 1 - 선행 필수)

## 목표
- [ ] task.rs - Task, TaskState, TaskStatus, TaskVersion
- [ ] message.rs - Message, Role
- [ ] part.rs - Part, PartContent (v1.0 통합 구조)
- [ ] artifact.rs - Artifact
- [ ] event.rs - Event enum (4 variants)
- [ ] agent_card.rs - AgentCard, AgentInterface, AgentSkill, AgentCapabilities
- [ ] auth.rs - SecurityScheme, OAuth2 등
- [ ] error.rs - A2AError, A2AErrorCode
- [ ] push.rs - TaskPushNotificationConfig
- [ ] jsonrpc.rs - JSON-RPC 2.0 request/response types, SendMessageRequest 등
- [ ] lib.rs - 모든 모듈 pub mod + re-export
- [ ] serde 직렬화/역직렬화 단위 테스트

## 파일별 구현 상세

### task.rs
```
타입:
- TaskState enum (9 variants, SCREAMING_SNAKE_CASE serde rename)
  - impl: is_terminal(), is_interrupt()
- TaskStatus { state, message?, timestamp? }
- Task { id, context_id, status, artifacts?, history?, metadata? }
- TaskVersion(i64)

serde: #[serde(rename_all = "camelCase")]
```

### message.rs
```
타입:
- Role enum (User, Agent) - ROLE_USER, ROLE_AGENT
- Message { message_id, role, parts, context_id?, task_id?, metadata?, extensions?, reference_task_ids? }

serde: camelCase
```

### part.rs
```
타입:
- PartContent enum (Text, Raw, Url, Data) - untagged serde
  - Raw: Vec<u8> with base64 serialization
- Part { content (flatten), metadata?, filename?, media_type? }

주의: PartContent의 untagged 역직렬화 순서 중요
- Text { text: String } 먼저
- Data { data: Value } 마지막 (가장 포괄적)

base64 직렬화: serde custom (serialize_with / deserialize_with)
  또는 별도 base64 모듈
```

### artifact.rs
```
타입:
- Artifact { artifact_id, name?, description?, parts, metadata?, extensions? }

serde: camelCase
```

### event.rs
```
타입:
- TaskStatusUpdateEvent { task_id, context_id, status, is_final?, metadata? }
- TaskArtifactUpdateEvent { task_id, context_id, artifact, append, last_chunk, metadata? }
- Event enum { Task, Message, TaskStatusUpdate, TaskArtifactUpdate }
  - #[non_exhaustive]

serde: 커스텀 Deserialize 구현 필요 (untagged로 시작, 문제 시 커스텀 visitor)
판별 로직:
  - "status" + "id" + "contextId" (no "role") → Task
  - "role" + "parts" → Message
  - "taskId" + "status" (no "artifact") → TaskStatusUpdate
  - "taskId" + "artifact" → TaskArtifactUpdate
```

### agent_card.rs
```
타입:
- ProtocolBinding enum (JsonRpc, Grpc, HttpJson)
- AgentInterface { url, protocol_binding, protocol_version, tenant? }
- AgentCapabilities { streaming, push_notifications, extended_agent_card, extensions? }
- AgentExtension { uri, description?, required? }
- AgentSkill { id, name, description?, tags?, examples?, input_modes?, output_modes? }
- AgentCard { name, description?, supported_interfaces, capabilities?, security_schemes?, skills?, default_input_modes?, default_output_modes? }

serde: camelCase
```

### auth.rs
```
타입:
- SecurityScheme enum (ApiKey, HttpAuth, OAuth2, Oidc, Mtls)
- ApiKeySecurityScheme { name, in_location }
- HttpAuthSecurityScheme { scheme, bearer_format? }
- OAuth2SecurityScheme { flows }
- OAuthFlows { authorization_code?, client_credentials?, device_code? }
- AuthorizationCodeOAuthFlow { authorization_url, token_url, scopes?, pkce_required? }
- ClientCredentialsOAuthFlow { token_url, scopes? }
- DeviceCodeOAuthFlow { device_authorization_url, token_url, scopes? }
- OpenIdConnectSecurityScheme { openid_connect_url }
- MutualTlsSecurityScheme { description? }

serde: camelCase
```

### error.rs
```
타입:
- A2AErrorCode enum (i32 값 매핑)
  - ParseError(-32700), InvalidRequest(-32600), MethodNotFound(-32601)
  - InvalidParams(-32602), InternalError(-32603)
  - TaskNotFound(-32001), TaskNotCancelable(-32002)
  - PushNotSupported(-32003), UnsupportedOperation(-32004)
  - IncompatibleContentTypes(-32005), ExtensionsNotSupported(-32006)
  - AgentCardNotFound(-32007)
  - Unauthenticated(-31401), Unauthorized(-31403)
- A2AError { code: i32, message: String, data?: Value }
  - impl From<A2AErrorCode>
  - 각 에러 코드에 대한 기본 메시지 제공
```

### push.rs
```
타입:
- TaskPushNotificationConfig { url, token?, authentication? }
- PushNotificationAuthInfo { schemes? }

serde: camelCase
```

### jsonrpc.rs
```
타입:
- JsonRpcId enum (Number(i64), String(String), Null) - untagged
- JsonRpcRequest { jsonrpc, method, params?, id }
- JsonRpcResponse { jsonrpc, result?, error?, id }
- JsonRpcError { code, message, data? }

요청 파라미터:
- SendMessageRequest { message, configuration? }
- MessageSendConfiguration { blocking?, accepted_output_modes?, push_notification_config?, history_length? }
- GetTaskRequest { id, history_length? }
- CancelTaskRequest { id }
- TaskResubscriptionRequest { id }

응답:
- SendMessageResponse (= Event: Task | Message)
- StreamResponse (= Event: 4 variants)
```

### lib.rs
```rust
pub mod task;
pub mod message;
pub mod part;
pub mod artifact;
pub mod event;
pub mod agent_card;
pub mod auth;
pub mod error;
pub mod push;
pub mod jsonrpc;

// Convenience re-exports
pub use task::*;
pub use message::*;
pub use part::*;
pub use artifact::*;
pub use event::*;
pub use agent_card::*;
pub use error::*;
pub use jsonrpc::*;
```

## 테스트 계획
각 모듈에 `#[cfg(test)] mod tests` 포함:
1. **직렬화 왕복**: Rust 타입 → JSON → Rust 타입
2. **ProtoJSON 호환**: 실제 A2A 프로토콜 JSON 샘플과 역직렬화 검증
3. **TaskState 헬퍼**: is_terminal(), is_interrupt() 검증
4. **Event 판별**: 각 variant의 JSON 역직렬화 정확성
5. **Part 판별**: text, raw, url, data 각각의 JSON 역직렬화

## 고려사항
- Event의 untagged 역직렬화가 가장 까다로움. 순서 의존적이므로 커스텀 Deserialize가 필요할 수 있음
- Part의 base64 직렬화는 `serde` 커스텀 함수로 처리
- 모든 Optional 필드는 `#[serde(skip_serializing_if = "Option::is_none")]`

## 관련 파일
- `.docs/architecture/260212_a2a-rs/02_data-model.md`
- `.docs/research/a2a-protocol-analysis.md` (섹션 4: 핵심 데이터 모델)

---
*작성일: 2026-02-12*
