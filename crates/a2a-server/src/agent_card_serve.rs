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
