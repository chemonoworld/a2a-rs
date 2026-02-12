# Examples & Integration Tests

## 개요
- **상위 태스크**: [Overview](./00_overview.md)
- **목적**: Hello world 예제로 server + client 통신 검증, 통합 테스트
- **담당**: coordinator (Phase 3 - Phase 2 완료 후)
- **의존**: a2a-server, a2a-client

## 목표
- [ ] examples/helloworld/server.rs - 에코 에이전트 서버
- [ ] examples/helloworld/client.rs - 메시지 전송 클라이언트
- [ ] 통합 테스트 - 서버/클라이언트 in-process 통신
- [ ] cargo check + clippy + test 전체 통과

## examples/helloworld/server.rs
```
에코 에이전트: 받은 메시지를 그대로 반환

구현:
struct EchoExecutor;

#[async_trait]
impl AgentExecutor for EchoExecutor {
    async fn execute(&self, event: Event, queue: &dyn EventQueueWriter) -> Result<()> {
        match event {
            Event::Message(msg) => {
                // 상태 업데이트: Working
                queue.write(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                    status: TaskStatus { state: TaskState::Working, .. },
                    ..
                })).await?;

                // 아티팩트: 받은 텍스트를 에코
                let text = extract_text(&msg);
                queue.write(Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                    artifact: Artifact {
                        parts: vec![Part { content: PartContent::Text { text: format!("Echo: {}", text) }, .. }],
                        ..
                    },
                    last_chunk: true,
                    ..
                })).await?;

                // 상태 업데이트: Completed
                queue.write(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                    status: TaskStatus { state: TaskState::Completed, .. },
                    is_final: Some(true),
                    ..
                })).await?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let agent_card = AgentCard {
        name: "Echo Agent".into(),
        supported_interfaces: vec![AgentInterface {
            url: "http://localhost:3000".into(),
            protocol_binding: ProtocolBinding::JsonRpc,
            protocol_version: "1.0".into(),
            ..
        }],
        skills: Some(vec![AgentSkill {
            id: "echo".into(),
            name: "Echo".into(),
            description: Some("Echoes back your message".into()),
            ..
        }]),
        ..
    };

    a2a_server::serve(EchoExecutor, agent_card, "0.0.0.0:3000").await.unwrap();
}
```

## examples/helloworld/client.rs
```
클라이언트: 서버에 메시지 전송 + 스트리밍 수신

#[tokio::main]
async fn main() {
    // 1. AgentCard 확인
    let resolver = AgentCardResolver::new();
    let card = resolver.resolve("http://localhost:3000").await.unwrap();
    println!("Agent: {}", card.name);

    // 2. 클라이언트 생성
    let client = A2AClient::from_agent_card(&card).await.unwrap();

    // 3. 동기 메시지 전송
    let message = Message {
        message_id: uuid::Uuid::new_v4().to_string(),
        role: Role::User,
        parts: vec![Part {
            content: PartContent::Text { text: "Hello, A2A!".into() },
            ..Default::default()
        }],
        ..Default::default()
    };
    let event = client.send_message(message).await.unwrap();
    println!("Response: {:?}", event);

    // 4. 스트리밍 메시지 전송
    let message2 = Message { ... };
    let mut stream = client.send_message_stream(message2).await.unwrap();
    while let Some(result) = stream.next().await {
        match result {
            Ok(event) => println!("Stream event: {:?}", event),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}
```

## Cargo.toml (examples)
```toml
# Workspace root Cargo.toml에 추가
[[example]]
name = "helloworld-server"
path = "examples/helloworld/server.rs"
required-features = []

[[example]]
name = "helloworld-client"
path = "examples/helloworld/client.rs"
required-features = []

[dev-dependencies]
a2a-types.workspace = true
a2a-server.workspace = true
a2a-client.workspace = true
tokio.workspace = true
tokio-stream.workspace = true
uuid.workspace = true
```

## 통합 테스트
```
tests/integration_test.rs:

1. test_send_message_sync
   - 서버를 랜덤 포트로 시작
   - 클라이언트로 메시지 전송
   - Task 또는 Message 응답 검증

2. test_send_message_stream
   - 서버 시작
   - 스트리밍 메시지 전송
   - StatusUpdate + ArtifactUpdate + Completed 이벤트 순서 검증

3. test_get_task
   - 메시지 전송 → task_id 획득
   - get_task로 태스크 조회
   - 상태 검증

4. test_cancel_task
   - 장기 실행 에이전트에 메시지 전송
   - cancel_task 호출
   - Canceled 상태 검증

5. test_agent_card_resolve
   - 서버의 /.well-known/agent-card.json 접근
   - AgentCard 파싱 검증

헬퍼:
  async fn start_test_server(executor: impl AgentExecutor) -> (String, JoinHandle<()>)
    - 0 포트로 바인드 → 실제 포트 획득
    - background 서버 시작
    - (url, handle) 반환
```

## 최종 검증
```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo run --example helloworld-server &
cargo run --example helloworld-client
```

## 관련 파일
- `.docs/architecture/260212_a2a-rs/00_overview.md`
- 모든 아키텍처 문서

---
*작성일: 2026-02-12*
