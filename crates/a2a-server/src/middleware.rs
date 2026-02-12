use async_trait::async_trait;

use crate::error::ServerError;

/// Request interceptor for before/after hooks on JSON-RPC method calls.
#[async_trait]
pub trait CallInterceptor: Send + Sync + 'static {
    async fn before(
        &self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<(), ServerError>;

    async fn after(
        &self,
        method: &str,
        result: &Result<serde_json::Value, ServerError>,
    ) -> Result<(), ServerError>;
}
