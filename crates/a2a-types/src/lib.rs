pub mod agent_card;
pub mod artifact;
pub mod auth;
pub mod error;
pub mod event;
pub mod jsonrpc;
pub mod message;
pub mod part;
pub mod push;
pub mod task;

// Convenience re-exports
pub use agent_card::{
    AgentCapabilities, AgentCard, AgentExtension, AgentInterface, AgentSkill, ProtocolBinding,
};
pub use artifact::Artifact;
pub use error::{A2AError, A2AErrorCode};
pub use event::{Event, TaskArtifactUpdateEvent, TaskStatusUpdateEvent};
pub use jsonrpc::{
    CancelTaskRequest, GetTaskRequest, JsonRpcError, JsonRpcId, JsonRpcRequest, JsonRpcResponse,
    MessageSendConfiguration, SendMessageRequest, TaskResubscriptionRequest,
};
pub use message::{Message, Role};
pub use part::{Part, PartContent};
pub use push::TaskPushNotificationConfig;
pub use task::{Task, TaskState, TaskStatus, TaskVersion};
