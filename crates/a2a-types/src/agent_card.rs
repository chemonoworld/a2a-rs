use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolBinding {
    #[serde(rename = "JSONRPC")]
    JsonRpc,
    #[serde(rename = "GRPC")]
    Grpc,
    #[serde(rename = "HTTP+JSON")]
    HttpJson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInterface {
    pub url: String,
    pub protocol_binding: ProtocolBinding,
    pub protocol_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub push_notifications: bool,
    #[serde(default)]
    pub extended_agent_card: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<AgentExtension>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExtension {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_modes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub supported_interfaces: Vec<AgentInterface>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<AgentCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_schemes: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<AgentSkill>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_input_modes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_output_modes: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_binding_serde() {
        assert_eq!(
            serde_json::to_string(&ProtocolBinding::JsonRpc).unwrap(),
            "\"JSONRPC\""
        );
        assert_eq!(
            serde_json::to_string(&ProtocolBinding::Grpc).unwrap(),
            "\"GRPC\""
        );
        assert_eq!(
            serde_json::to_string(&ProtocolBinding::HttpJson).unwrap(),
            "\"HTTP+JSON\""
        );
    }

    #[test]
    fn test_agent_card_roundtrip() {
        let card = AgentCard {
            name: "Test Agent".into(),
            description: Some("A test agent".into()),
            supported_interfaces: vec![AgentInterface {
                url: "http://localhost:3000".into(),
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
                description: Some("Echoes messages".into()),
                tags: Some(vec!["test".into()]),
                examples: None,
                input_modes: None,
                output_modes: None,
            }]),
            default_input_modes: None,
            default_output_modes: None,
        };

        let json = serde_json::to_string_pretty(&card).unwrap();
        let deserialized: AgentCard = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "Test Agent");
        assert_eq!(deserialized.supported_interfaces.len(), 1);
        assert_eq!(
            deserialized.supported_interfaces[0].protocol_binding,
            ProtocolBinding::JsonRpc
        );
        assert!(deserialized.capabilities.unwrap().streaming);
        assert_eq!(deserialized.skills.unwrap().len(), 1);
    }

    #[test]
    fn test_agent_card_minimal() {
        let json = r#"{
            "name": "Minimal",
            "supportedInterfaces": [{
                "url": "http://localhost:8080",
                "protocolBinding": "JSONRPC",
                "protocolVersion": "1.0"
            }]
        }"#;

        let card: AgentCard = serde_json::from_str(json).unwrap();
        assert_eq!(card.name, "Minimal");
        assert!(card.capabilities.is_none());
        assert!(card.skills.is_none());
    }
}
