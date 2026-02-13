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

#[cfg(test)]
mod tests {
    use super::*;
    use a2a_types::{AgentCard, AgentInterface, Event, ProtocolBinding};
    use async_trait::async_trait;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn make_card() -> AgentCard {
        AgentCard {
            name: "RouterTest".into(),
            description: None,
            supported_interfaces: vec![AgentInterface {
                url: "http://localhost:3000".into(),
                protocol_binding: ProtocolBinding::JsonRpc,
                protocol_version: "1.0".into(),
                tenant: None,
            }],
            capabilities: None,
            security_schemes: None,
            skills: None,
            default_input_modes: None,
            default_output_modes: None,
        }
    }

    struct NoopExecutor;

    #[async_trait]
    impl AgentExecutor for NoopExecutor {
        async fn execute(
            &self,
            _event: Event,
            _queue: &dyn crate::event_queue::EventQueueWriter,
        ) -> Result<(), ServerError> {
            Ok(())
        }
    }

    fn make_handler() -> Arc<dyn RequestHandler> {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(NoopExecutor))
            .build();
        Arc::new(handler)
    }

    #[test]
    fn test_create_router_returns_router() {
        let router = create_router(make_handler(), make_card());
        let _ = router; // Should compile and create without panic
    }

    #[test]
    fn test_create_router_with_interceptor_none() {
        let router = create_router_with_interceptor(make_handler(), make_card(), None);
        let _ = router;
    }

    #[test]
    fn test_create_router_with_interceptor_some() {
        use crate::middleware::CallInterceptor;

        struct TestInterceptor;

        #[async_trait]
        impl CallInterceptor for TestInterceptor {
            async fn before(
                &self,
                _method: &str,
                _params: &serde_json::Value,
            ) -> Result<(), ServerError> {
                Ok(())
            }

            async fn after(
                &self,
                _method: &str,
                _result: &Result<serde_json::Value, ServerError>,
            ) -> Result<(), ServerError> {
                Ok(())
            }
        }

        let interceptor: Option<Arc<dyn CallInterceptor>> =
            Some(Arc::new(TestInterceptor));
        let router =
            create_router_with_interceptor(make_handler(), make_card(), interceptor);
        let _ = router;
    }

    #[tokio::test]
    async fn test_router_post_root_reaches_jsonrpc_handler() {
        let router = create_router(make_handler(), make_card());

        let body = r#"{"jsonrpc":"2.0","id":1,"method":"unknown/method","params":{}}"#;
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        // MethodNotFound for unknown method
        assert_eq!(parsed["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn test_router_get_agent_card_endpoint() {
        let router = create_router(make_handler(), make_card());

        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let card: AgentCard = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(card.name, "RouterTest");
    }

    #[tokio::test]
    async fn test_router_unknown_path_returns_404() {
        let router = create_router(make_handler(), make_card());

        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/nonexistent")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn test_router_get_to_post_endpoint_returns_405() {
        let router = create_router(make_handler(), make_card());

        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 405);
    }

    #[tokio::test]
    async fn test_app_state_clone() {
        let state = AppState {
            handler: make_handler(),
            agent_card: make_card(),
            interceptor: None,
        };

        let cloned = state.clone();
        assert_eq!(cloned.agent_card.name, "RouterTest");
        assert!(cloned.interceptor.is_none());
    }

    #[tokio::test]
    async fn test_app_state_with_interceptor_clone() {
        struct SimpleInterceptor;

        #[async_trait]
        impl CallInterceptor for SimpleInterceptor {
            async fn before(
                &self,
                _method: &str,
                _params: &serde_json::Value,
            ) -> Result<(), ServerError> {
                Ok(())
            }

            async fn after(
                &self,
                _method: &str,
                _result: &Result<serde_json::Value, ServerError>,
            ) -> Result<(), ServerError> {
                Ok(())
            }
        }

        let state = AppState {
            handler: make_handler(),
            agent_card: make_card(),
            interceptor: Some(Arc::new(SimpleInterceptor)),
        };

        let cloned = state.clone();
        assert!(cloned.interceptor.is_some());
    }
}
