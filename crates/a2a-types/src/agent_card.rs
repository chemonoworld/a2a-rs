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

    #[test]
    fn test_agent_card_multiple_interfaces() {
        let card = AgentCard {
            name: "Multi".into(),
            description: None,
            supported_interfaces: vec![
                AgentInterface {
                    url: "http://localhost:3000".into(),
                    protocol_binding: ProtocolBinding::JsonRpc,
                    protocol_version: "1.0".into(),
                    tenant: None,
                },
                AgentInterface {
                    url: "grpc://localhost:50051".into(),
                    protocol_binding: ProtocolBinding::Grpc,
                    protocol_version: "1.0".into(),
                    tenant: Some("tenant-1".into()),
                },
                AgentInterface {
                    url: "http://localhost:8080/api".into(),
                    protocol_binding: ProtocolBinding::HttpJson,
                    protocol_version: "1.0".into(),
                    tenant: None,
                },
            ],
            capabilities: None,
            security_schemes: None,
            skills: None,
            default_input_modes: None,
            default_output_modes: None,
        };

        let json = serde_json::to_string(&card).unwrap();
        let deserialized: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.supported_interfaces.len(), 3);
        assert_eq!(
            deserialized.supported_interfaces[1].protocol_binding,
            ProtocolBinding::Grpc
        );
        assert_eq!(
            deserialized.supported_interfaces[1].tenant.as_deref(),
            Some("tenant-1")
        );
    }

    #[test]
    fn test_agent_card_with_extensions() {
        let card = AgentCard {
            name: "Extended".into(),
            description: None,
            supported_interfaces: vec![AgentInterface {
                url: "http://localhost:3000".into(),
                protocol_binding: ProtocolBinding::JsonRpc,
                protocol_version: "1.0".into(),
                tenant: None,
            }],
            capabilities: Some(AgentCapabilities {
                streaming: true,
                push_notifications: true,
                extended_agent_card: true,
                extensions: Some(vec![AgentExtension {
                    uri: "urn:a2a:ext:custom".into(),
                    description: Some("Custom extension".into()),
                    required: Some(true),
                }]),
            }),
            security_schemes: None,
            skills: None,
            default_input_modes: Some(vec!["text/plain".into()]),
            default_output_modes: Some(vec!["text/plain".into(), "application/json".into()]),
        };

        let json = serde_json::to_string(&card).unwrap();
        assert!(json.contains("urn:a2a:ext:custom"));
        assert!(json.contains("defaultInputModes"));
        assert!(json.contains("defaultOutputModes"));

        let deserialized: AgentCard = serde_json::from_str(&json).unwrap();
        let caps = deserialized.capabilities.unwrap();
        assert!(caps.push_notifications);
        assert!(caps.extended_agent_card);
        let exts = caps.extensions.unwrap();
        assert_eq!(exts.len(), 1);
        assert_eq!(exts[0].required, Some(true));

        assert_eq!(deserialized.default_output_modes.unwrap().len(), 2);
    }

    #[test]
    fn test_agent_card_skill_all_fields() {
        let skill = AgentSkill {
            id: "translate".into(),
            name: "Translation".into(),
            description: Some("Translates text between languages".into()),
            tags: Some(vec!["nlp".into(), "translation".into()]),
            examples: Some(vec![
                "Translate 'hello' to Spanish".into(),
                "What is 'goodbye' in French?".into(),
            ]),
            input_modes: Some(vec!["text/plain".into()]),
            output_modes: Some(vec!["text/plain".into(), "application/json".into()]),
        };

        let json = serde_json::to_string(&skill).unwrap();
        let deserialized: AgentSkill = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tags.unwrap().len(), 2);
        assert_eq!(deserialized.examples.unwrap().len(), 2);
        assert_eq!(deserialized.input_modes.unwrap(), vec!["text/plain"]);
    }

    #[test]
    fn test_agent_capabilities_defaults() {
        // When capabilities JSON has missing fields, defaults should be false
        let json = r#"{}"#;
        let caps: AgentCapabilities = serde_json::from_str(json).unwrap();
        assert!(!caps.streaming);
        assert!(!caps.push_notifications);
        assert!(!caps.extended_agent_card);
        assert!(caps.extensions.is_none());
    }

    #[test]
    fn test_agent_card_with_security_schemes() {
        let card = AgentCard {
            name: "Secured Agent".into(),
            description: None,
            supported_interfaces: vec![AgentInterface {
                url: "http://localhost:3000".into(),
                protocol_binding: ProtocolBinding::JsonRpc,
                protocol_version: "1.0".into(),
                tenant: None,
            }],
            capabilities: None,
            security_schemes: Some({
                let mut m = serde_json::Map::new();
                m.insert("bearerAuth".into(), serde_json::json!({
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "JWT"
                }));
                m.insert("apiKeyAuth".into(), serde_json::json!({
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-API-Key"
                }));
                m
            }),
            skills: None,
            default_input_modes: None,
            default_output_modes: None,
        };

        let json = serde_json::to_string(&card).unwrap();
        assert!(json.contains("\"securitySchemes\""));
        assert!(json.contains("bearerAuth"));
        assert!(json.contains("apiKeyAuth"));

        let deserialized: AgentCard = serde_json::from_str(&json).unwrap();
        let schemes = deserialized.security_schemes.unwrap();
        assert_eq!(schemes.len(), 2);
        assert!(schemes.contains_key("bearerAuth"));
        assert!(schemes.contains_key("apiKeyAuth"));
    }

    #[test]
    fn test_agent_extension_minimal() {
        let ext = AgentExtension {
            uri: "urn:a2a:ext:minimal".into(),
            description: None,
            required: None,
        };

        let json = serde_json::to_string(&ext).unwrap();
        assert!(!json.contains("description"));
        assert!(!json.contains("required"));

        let deserialized: AgentExtension = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.uri, "urn:a2a:ext:minimal");
        assert!(deserialized.description.is_none());
        assert!(deserialized.required.is_none());
    }

    #[test]
    fn test_agent_card_from_raw_json_full() {
        let json = r#"{
            "name": "Full Agent",
            "description": "A fully populated agent card",
            "supportedInterfaces": [
                {
                    "url": "http://localhost:3000",
                    "protocolBinding": "JSONRPC",
                    "protocolVersion": "1.0",
                    "tenant": "my-tenant"
                },
                {
                    "url": "http://localhost:3001",
                    "protocolBinding": "HTTP+JSON",
                    "protocolVersion": "1.0"
                }
            ],
            "capabilities": {
                "streaming": true,
                "pushNotifications": true,
                "extendedAgentCard": false
            },
            "skills": [
                {
                    "id": "skill-1",
                    "name": "Skill One"
                }
            ],
            "defaultInputModes": ["text/plain"],
            "defaultOutputModes": ["text/plain", "application/json"]
        }"#;

        let card: AgentCard = serde_json::from_str(json).unwrap();
        assert_eq!(card.name, "Full Agent");
        assert_eq!(card.description.as_deref(), Some("A fully populated agent card"));
        assert_eq!(card.supported_interfaces.len(), 2);
        assert_eq!(card.supported_interfaces[0].tenant.as_deref(), Some("my-tenant"));
        assert_eq!(card.supported_interfaces[1].protocol_binding, ProtocolBinding::HttpJson);
        let caps = card.capabilities.unwrap();
        assert!(caps.streaming);
        assert!(caps.push_notifications);
        assert!(!caps.extended_agent_card);
        assert_eq!(card.skills.unwrap().len(), 1);
        assert_eq!(card.default_input_modes.unwrap(), vec!["text/plain"]);
        assert_eq!(card.default_output_modes.unwrap().len(), 2);
    }

    #[test]
    fn test_agent_card_optional_fields_omitted_in_json() {
        let card = AgentCard {
            name: "Sparse".into(),
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
        };

        let json = serde_json::to_string(&card).unwrap();
        assert!(!json.contains("description"));
        assert!(!json.contains("capabilities"));
        assert!(!json.contains("securitySchemes"));
        assert!(!json.contains("skills"));
        assert!(!json.contains("defaultInputModes"));
        assert!(!json.contains("defaultOutputModes"));
        assert!(!json.contains("tenant"));
    }

    #[test]
    fn test_invalid_protocol_binding_deserialization() {
        let json = r#"{
            "name": "Bad",
            "supportedInterfaces": [{
                "url": "http://localhost",
                "protocolBinding": "INVALID_PROTOCOL",
                "protocolVersion": "1.0"
            }]
        }"#;

        let result: Result<AgentCard, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Invalid protocol binding should fail");
    }

    #[test]
    fn test_capabilities_partial_json() {
        // JSON with only some capabilities fields - others should default
        let json = r#"{"streaming": true}"#;
        let caps: AgentCapabilities = serde_json::from_str(json).unwrap();
        assert!(caps.streaming);
        assert!(!caps.push_notifications); // default false
        assert!(!caps.extended_agent_card); // default false
        assert!(caps.extensions.is_none());

        // JSON with only pushNotifications
        let json2 = r#"{"pushNotifications": true, "extendedAgentCard": true}"#;
        let caps2: AgentCapabilities = serde_json::from_str(json2).unwrap();
        assert!(!caps2.streaming); // default false
        assert!(caps2.push_notifications);
        assert!(caps2.extended_agent_card);
    }

    #[test]
    fn test_agent_card_unknown_fields_ignored() {
        let json = r#"{
            "name": "Test",
            "supportedInterfaces": [{
                "url": "http://localhost:3000",
                "protocolBinding": "JSONRPC",
                "protocolVersion": "1.0"
            }],
            "unknownField": "should be ignored",
            "anotherExtra": 42
        }"#;
        let card: AgentCard = serde_json::from_str(json).unwrap();
        assert_eq!(card.name, "Test");
        assert_eq!(card.supported_interfaces.len(), 1);
    }

    #[test]
    fn test_agent_card_all_optionals_populated() {
        let card = AgentCard {
            name: "Full Agent".into(),
            description: Some("Complete description".into()),
            supported_interfaces: vec![
                AgentInterface {
                    url: "http://localhost:3000".into(),
                    protocol_binding: ProtocolBinding::JsonRpc,
                    protocol_version: "1.0".into(),
                    tenant: Some("tenant-1".into()),
                },
                AgentInterface {
                    url: "grpc://localhost:50051".into(),
                    protocol_binding: ProtocolBinding::Grpc,
                    protocol_version: "1.0".into(),
                    tenant: None,
                },
            ],
            capabilities: Some(AgentCapabilities {
                streaming: true,
                push_notifications: true,
                extended_agent_card: true,
                extensions: Some(vec![AgentExtension {
                    uri: "urn:a2a:ext:custom".into(),
                    description: Some("Custom extension".into()),
                    required: Some(true),
                }]),
            }),
            security_schemes: Some({
                let mut m = serde_json::Map::new();
                m.insert("bearer".into(), serde_json::json!({"type": "http", "scheme": "bearer"}));
                m
            }),
            skills: Some(vec![AgentSkill {
                id: "skill-1".into(),
                name: "Skill One".into(),
                description: Some("Does something".into()),
                tags: Some(vec!["tag1".into(), "tag2".into()]),
                examples: Some(vec!["example 1".into()]),
                input_modes: Some(vec!["text/plain".into()]),
                output_modes: Some(vec!["application/json".into()]),
            }]),
            default_input_modes: Some(vec!["text/plain".into()]),
            default_output_modes: Some(vec!["application/json".into(), "text/plain".into()]),
        };

        let json = serde_json::to_string(&card).unwrap();
        assert!(json.contains("\"tenant\":\"tenant-1\""));
        assert!(json.contains("\"defaultInputModes\""));
        assert!(json.contains("\"defaultOutputModes\""));
        assert!(json.contains("\"securitySchemes\""));

        let deserialized: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.supported_interfaces.len(), 2);
        assert!(deserialized.capabilities.as_ref().unwrap().streaming);
        assert_eq!(deserialized.default_input_modes.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.default_output_modes.as_ref().unwrap().len(), 2);
        let skills = deserialized.skills.as_ref().unwrap();
        assert_eq!(skills[0].tags.as_ref().unwrap().len(), 2);
        assert_eq!(skills[0].input_modes.as_ref().unwrap()[0], "text/plain");
    }

    #[test]
    fn test_agent_card_missing_required_name_fails() {
        let json = r#"{
            "supportedInterfaces": [{
                "url": "http://localhost:3000",
                "protocolBinding": "JSONRPC",
                "protocolVersion": "1.0"
            }]
        }"#;
        let result: Result<AgentCard, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing name should fail deserialization");
    }

    #[test]
    fn test_agent_card_missing_supported_interfaces_fails() {
        let json = r#"{"name": "No Interfaces"}"#;
        let result: Result<AgentCard, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing supportedInterfaces should fail");
    }

    #[test]
    fn test_agent_skill_minimal() {
        let json = r#"{"id": "s-1", "name": "Basic Skill"}"#;
        let skill: AgentSkill = serde_json::from_str(json).unwrap();
        assert_eq!(skill.id, "s-1");
        assert_eq!(skill.name, "Basic Skill");
        assert!(skill.description.is_none());
        assert!(skill.tags.is_none());
        assert!(skill.examples.is_none());
        assert!(skill.input_modes.is_none());
        assert!(skill.output_modes.is_none());

        // Roundtrip
        let json_out = serde_json::to_string(&skill).unwrap();
        assert!(!json_out.contains("description"));
        assert!(!json_out.contains("tags"));
    }

    #[test]
    fn test_agent_interface_missing_required_fields_fails() {
        // Missing url
        let json = r#"{"protocolBinding": "JSONRPC", "protocolVersion": "1.0"}"#;
        let result: Result<AgentInterface, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing url should fail");

        // Missing protocolBinding
        let json = r#"{"url": "http://localhost", "protocolVersion": "1.0"}"#;
        let result: Result<AgentInterface, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing protocolBinding should fail");

        // Missing protocolVersion
        let json = r#"{"url": "http://localhost", "protocolBinding": "JSONRPC"}"#;
        let result: Result<AgentInterface, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing protocolVersion should fail");
    }

    #[test]
    fn test_agent_card_empty_skills_array() {
        // Empty skills array should serialize/deserialize distinct from None
        let card = AgentCard {
            name: "EmptySkills".into(),
            description: None,
            supported_interfaces: vec![AgentInterface {
                url: "http://localhost:3000".into(),
                protocol_binding: ProtocolBinding::JsonRpc,
                protocol_version: "1.0".into(),
                tenant: None,
            }],
            capabilities: None,
            security_schemes: None,
            skills: Some(vec![]),
            default_input_modes: None,
            default_output_modes: None,
        };

        let json = serde_json::to_string(&card).unwrap();
        assert!(json.contains("\"skills\":[]"), "Empty array should be serialized");

        let deserialized: AgentCard = serde_json::from_str(&json).unwrap();
        let skills = deserialized.skills.unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn test_agent_card_empty_supported_interfaces_deserializes() {
        // Empty supportedInterfaces is syntactically valid JSON but semantically questionable
        let json = r#"{"name": "Empty", "supportedInterfaces": []}"#;
        let card: AgentCard = serde_json::from_str(json).unwrap();
        assert!(card.supported_interfaces.is_empty());
    }

    #[test]
    fn test_agent_extension_required_false_vs_none() {
        // required: false should be different from omitted (None)
        let ext_false = AgentExtension {
            uri: "urn:ext:test".into(),
            description: None,
            required: Some(false),
        };
        let json_false = serde_json::to_string(&ext_false).unwrap();
        assert!(json_false.contains("\"required\":false"));

        let ext_none = AgentExtension {
            uri: "urn:ext:test".into(),
            description: None,
            required: None,
        };
        let json_none = serde_json::to_string(&ext_none).unwrap();
        assert!(!json_none.contains("required"));
    }

    #[test]
    fn test_agent_interface_with_tenant_and_query_params() {
        let json = r#"{
            "url": "https://agent.example.com/v1?key=value&token=abc#section",
            "protocolBinding": "HTTP+JSON",
            "protocolVersion": "2.0.1",
            "tenant": "org-123"
        }"#;
        let iface: AgentInterface = serde_json::from_str(json).unwrap();
        assert!(iface.url.contains("key=value"));
        assert_eq!(iface.protocol_binding, ProtocolBinding::HttpJson);
        assert_eq!(iface.protocol_version, "2.0.1");
        assert_eq!(iface.tenant.as_deref(), Some("org-123"));
    }
}
