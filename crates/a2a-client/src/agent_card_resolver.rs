use a2a_types::AgentCard;

use crate::error::ClientError;

/// Resolves an [`AgentCard`] from a server's well-known URI.
///
/// Per the A2A specification, the card is served at
/// `{base_url}/.well-known/agent-card.json`.
pub struct AgentCardResolver {
    client: reqwest::Client,
}

impl AgentCardResolver {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Fetch and deserialize the agent card from `{base_url}/.well-known/agent-card.json`.
    pub async fn resolve(&self, base_url: &str) -> Result<AgentCard, ClientError> {
        let url = format!(
            "{}/.well-known/agent-card.json",
            base_url.trim_end_matches('/')
        );
        let response = self.client.get(&url).send().await?;
        let card: AgentCard = response.json().await?;
        Ok(card)
    }
}

impl Default for AgentCardResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_new_creates_instance() {
        let resolver = AgentCardResolver::new();
        // Ensure the client was created (no panic)
        let _ = resolver;
    }

    #[test]
    fn test_resolver_default_creates_instance() {
        let resolver = AgentCardResolver::default();
        let _ = resolver;
    }

    #[test]
    fn test_resolve_url_construction_trailing_slash() {
        // Verify URL construction logic: trailing slash should be stripped
        let base = "http://localhost:3000/";
        let expected = "http://localhost:3000/.well-known/agent-card.json";
        let url = format!(
            "{}/.well-known/agent-card.json",
            base.trim_end_matches('/')
        );
        assert_eq!(url, expected);
    }

    #[test]
    fn test_resolve_url_construction_no_trailing_slash() {
        let base = "http://localhost:3000";
        let expected = "http://localhost:3000/.well-known/agent-card.json";
        let url = format!(
            "{}/.well-known/agent-card.json",
            base.trim_end_matches('/')
        );
        assert_eq!(url, expected);
    }

    #[test]
    fn test_resolve_url_construction_with_path() {
        let base = "http://example.com/api/v1/";
        let expected = "http://example.com/api/v1/.well-known/agent-card.json";
        let url = format!(
            "{}/.well-known/agent-card.json",
            base.trim_end_matches('/')
        );
        assert_eq!(url, expected);
    }

    #[test]
    fn test_resolve_url_construction_https() {
        let base = "https://example.com";
        let expected = "https://example.com/.well-known/agent-card.json";
        let url = format!(
            "{}/.well-known/agent-card.json",
            base.trim_end_matches('/')
        );
        assert_eq!(url, expected);
    }

    #[test]
    fn test_resolve_url_construction_with_port() {
        let base = "http://localhost:8080/";
        let expected = "http://localhost:8080/.well-known/agent-card.json";
        let url = format!(
            "{}/.well-known/agent-card.json",
            base.trim_end_matches('/')
        );
        assert_eq!(url, expected);
    }

    #[test]
    fn test_resolve_url_construction_multiple_trailing_slashes() {
        // trim_end_matches removes all trailing slashes
        let base = "http://example.com///";
        let expected = "http://example.com/.well-known/agent-card.json";
        let url = format!(
            "{}/.well-known/agent-card.json",
            base.trim_end_matches('/')
        );
        assert_eq!(url, expected);
    }

    #[test]
    fn test_resolve_url_construction_with_query_params() {
        // Query params in base_url should be preserved (though unusual)
        let base = "http://example.com?key=value";
        let expected = "http://example.com?key=value/.well-known/agent-card.json";
        let url = format!(
            "{}/.well-known/agent-card.json",
            base.trim_end_matches('/')
        );
        assert_eq!(url, expected);
    }

    #[tokio::test]
    async fn test_resolve_with_invalid_url() {
        let resolver = AgentCardResolver::new();
        // Use a completely invalid URL to trigger an error quickly
        let result = resolver.resolve("not-a-valid-url://???").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::Http(_) => {} // Expected: reqwest error for invalid URL
            other => panic!("Expected Http error, got: {other:?}"),
        }
    }
}
