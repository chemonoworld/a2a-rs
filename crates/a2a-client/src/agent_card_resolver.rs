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
