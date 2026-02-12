use std::sync::Arc;

use a2a_types::AgentCard;
use axum::routing::{get, post};
use axum::Router;

use crate::agent_card_serve::serve_agent_card;
use crate::agent_executor::AgentExecutor;
use crate::error::ServerError;
use crate::handler::{DefaultHandler, RequestHandler};
use crate::jsonrpc_handler::jsonrpc_handler;
use crate::middleware::CallInterceptor;

/// Shared application state for axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub handler: Arc<dyn RequestHandler>,
    pub agent_card: AgentCard,
    pub interceptor: Option<Arc<dyn CallInterceptor>>,
}

/// Create an axum `Router` with A2A protocol endpoints.
pub fn create_router(handler: Arc<dyn RequestHandler>, agent_card: AgentCard) -> Router {
    create_router_with_interceptor(handler, agent_card, None)
}

/// Create an axum `Router` with A2A protocol endpoints and an optional interceptor.
pub fn create_router_with_interceptor(
    handler: Arc<dyn RequestHandler>,
    agent_card: AgentCard,
    interceptor: Option<Arc<dyn CallInterceptor>>,
) -> Router {
    let state = AppState {
        handler,
        agent_card,
        interceptor,
    };

    Router::new()
        .route("/", post(jsonrpc_handler))
        .route("/.well-known/agent-card.json", get(serve_agent_card))
        .with_state(state)
}

/// Convenience function: create a server from an `AgentExecutor` and start listening.
pub async fn serve(
    executor: impl AgentExecutor,
    agent_card: AgentCard,
    addr: &str,
) -> Result<(), ServerError> {
    let handler = DefaultHandler::builder()
        .executor(Arc::new(executor))
        .build();
    let router = create_router(Arc::new(handler), agent_card);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .await
        .map_err(ServerError::Io)?;
    Ok(())
}
