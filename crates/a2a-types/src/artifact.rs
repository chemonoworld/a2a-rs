use serde::{Deserialize, Serialize};

use crate::part::Part;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub artifact_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parts: Vec<Part>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::part::PartContent;

    #[test]
    fn test_artifact_serde_roundtrip() {
        let artifact = Artifact {
            artifact_id: "art-1".into(),
            name: Some("Result".into()),
            description: None,
            parts: vec![Part {
                content: PartContent::Text {
                    text: "output data".into(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            }],
            metadata: None,
            extensions: None,
        };

        let json = serde_json::to_string(&artifact).unwrap();
        assert!(json.contains("\"artifactId\":\"art-1\""));

        let deserialized: Artifact = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.artifact_id, "art-1");
        assert_eq!(deserialized.name.as_deref(), Some("Result"));
        assert_eq!(deserialized.parts.len(), 1);
    }

    #[test]
    fn test_artifact_optional_fields_omitted() {
        let artifact = Artifact {
            artifact_id: "art-1".into(),
            name: None,
            description: None,
            parts: vec![],
            metadata: None,
            extensions: None,
        };

        let json = serde_json::to_string(&artifact).unwrap();
        assert!(!json.contains("name"));
        assert!(!json.contains("description"));
    }
}
