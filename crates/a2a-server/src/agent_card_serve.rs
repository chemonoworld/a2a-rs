use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;

use crate::router::AppState;

/// Serve the agent card at `/.well-known/agent-card.json` with CORS headers.
pub async fn serve_agent_card(State(state): State<AppState>) -> impl IntoResponse {
    let json = serde_json::to_string(&state.agent_card).unwrap_or_default();
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS"),
            (header::ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type"),
        ],
        json,
    )
}

#[cfg(test)]
mod tests {
    use crate::handler::DefaultHandler;
    use crate::router::create_router;
    use a2a_types::AgentCard;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tower::ServiceExt;

    // Minimal AgentExecutor for building a handler
    struct NoopExecutor;
    #[async_trait::async_trait]
    impl crate::agent_executor::AgentExecutor for NoopExecutor {
        async fn execute(
            &self,
            _event: a2a_types::Event,
            _queue: &dyn crate::event_queue::EventQueueWriter,
        ) -> Result<(), crate::error::ServerError> {
            Ok(())
        }
    }

    fn make_test_agent_card() -> AgentCard {
        AgentCard {
            name: "TestAgent".into(),
            description: Some("A test agent".into()),
            supported_interfaces: vec![],
            capabilities: None,
            security_schemes: None,
            skills: None,
            default_input_modes: None,
            default_output_modes: None,
        }
    }

    fn make_router() -> axum::Router {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(NoopExecutor))
            .build();
        create_router(Arc::new(handler), make_test_agent_card())
    }

    #[tokio::test]
    async fn test_agent_card_endpoint_returns_json() {
        let app = make_router();
        let req = Request::builder()
            .uri("/.well-known/agent-card.json")
            .method("GET")
            .body(axum::body::Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let content_type = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(content_type, "application/json");

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let card: AgentCard = serde_json::from_slice(&body).unwrap();
        assert_eq!(card.name, "TestAgent");
    }

    #[tokio::test]
    async fn test_agent_card_cors_headers() {
        let app = make_router();
        let req = Request::builder()
            .uri("/.well-known/agent-card.json")
            .method("GET")
            .body(axum::body::Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();

        let allow_origin = resp
            .headers()
            .get("access-control-allow-origin")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(allow_origin, "*");

        let allow_methods = resp
            .headers()
            .get("access-control-allow-methods")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(allow_methods, "GET, OPTIONS");

        let allow_headers = resp
            .headers()
            .get("access-control-allow-headers")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(allow_headers, "Content-Type");
    }

    #[tokio::test]
    async fn test_agent_card_with_optional_fields() {
        let handler = DefaultHandler::builder()
            .executor(Arc::new(NoopExecutor))
            .build();

        let mut card = make_test_agent_card();
        card.skills = Some(vec![a2a_types::AgentSkill {
            id: "skill-1".into(),
            name: "TestSkill".into(),
            description: Some("A test skill".into()),
            tags: None,
            examples: None,
            input_modes: None,
            output_modes: None,
        }]);

        let app = create_router(Arc::new(handler), card);
        let req = Request::builder()
            .uri("/.well-known/agent-card.json")
            .method("GET")
            .body(axum::body::Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let parsed: AgentCard = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed.skills.unwrap().len(), 1);
    }
}
