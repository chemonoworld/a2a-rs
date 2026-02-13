use std::sync::Arc;

use a2a_server::{AgentExecutor, EventQueueWriter, ServerError};
use a2a_types::{
    AgentCapabilities, AgentCard, AgentInterface, AgentSkill, Artifact, Event, Part, PartContent,
    ProtocolBinding, TaskArtifactUpdateEvent, TaskState, TaskStatus, TaskStatusUpdateEvent,
};
use async_trait::async_trait;

/// A simple echo agent that reflects the user's message back.
struct EchoExecutor;

#[async_trait]
impl AgentExecutor for EchoExecutor {
    async fn execute(
        &self,
        event: Event,
        queue: &dyn EventQueueWriter,
    ) -> Result<(), ServerError> {
        // Extract text from the initial task event
        let (task_id, context_id) = match &event {
            Event::Task(task) => (task.id.clone(), task.context_id.clone()),
            _ => return Ok(()),
        };

        // Signal working state
        queue
            .write(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                status: TaskStatus {
                    state: TaskState::Working,
                    message: None,
                    timestamp: None,
                },
                is_final: None,
                metadata: None,
            }))
            .await?;

        // Produce an echo artifact
        queue
            .write(Event::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                task_id: task_id.clone(),
                context_id: context_id.clone(),
                artifact: Artifact {
                    artifact_id: "echo-artifact-1".into(),
                    name: Some("Echo Response".into()),
                    description: None,
                    parts: vec![Part {
                        content: PartContent::Text {
                            text: "Echo: Hello from the A2A echo agent!".into(),
                        },
                        metadata: None,
                        filename: None,
                        media_type: None,
                    }],
                    metadata: None,
                    extensions: None,
                },
                append: false,
                last_chunk: true,
                metadata: None,
            }))
            .await?;

        // Signal completed state
        queue
            .write(Event::TaskStatusUpdate(TaskStatusUpdateEvent {
                task_id,
                context_id,
                status: TaskStatus {
                    state: TaskState::Completed,
                    message: None,
                    timestamp: None,
                },
                is_final: Some(true),
                metadata: None,
            }))
            .await?;

        Ok(())
    }
}

fn build_agent_card(addr: &str) -> AgentCard {
    AgentCard {
        name: "Echo Agent".into(),
        description: Some("A simple A2A echo agent that reflects messages back".into()),
        supported_interfaces: vec![AgentInterface {
            url: format!("http://{addr}"),
            protocol_binding: ProtocolBinding::JsonRpc,
            protocol_version: "1.0".into(),
            tenant: None,
        }],
        capabilities: Some(AgentCapabilities {
            streaming: true,
            push_notifications: false,
            extended_agent_card: false,
            extensions: None,
        }),
        security_schemes: None,
        skills: Some(vec![AgentSkill {
            id: "echo".into(),
            name: "Echo".into(),
            description: Some("Echoes back your message".into()),
            tags: Some(vec!["demo".into()]),
            examples: Some(vec!["Say hello".into()]),
            input_modes: None,
            output_modes: None,
        }]),
        default_input_modes: None,
        default_output_modes: None,
    }
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:3000";
    let agent_card = build_agent_card(addr);

    println!("Starting Echo Agent on http://{addr}");
    println!("Agent card: http://{addr}/.well-known/agent-card.json");

    let handler = a2a_server::DefaultHandler::builder()
        .executor(Arc::new(EchoExecutor))
        .build();
    let router = a2a_server::create_router(Arc::new(handler), agent_card);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, router).await.unwrap();
}
