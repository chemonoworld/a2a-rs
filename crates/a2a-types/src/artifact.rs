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
    fn test_artifact_all_fields_populated() {
        let artifact = Artifact {
            artifact_id: "art-full".into(),
            name: Some("Full Artifact".into()),
            description: Some("An artifact with all fields".into()),
            parts: vec![
                Part {
                    content: PartContent::Text {
                        text: "text part".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                },
                Part {
                    content: PartContent::Data {
                        data: serde_json::json!({"key": "value"}),
                    },
                    metadata: Some(serde_json::json!({"source": "test"})),
                    filename: Some("data.json".into()),
                    media_type: Some("application/json".into()),
                },
            ],
            metadata: Some(serde_json::json!({"version": 2, "tags": ["important"]})),
            extensions: Some(vec!["urn:a2a:ext:custom".into()]),
        };

        let json = serde_json::to_string(&artifact).unwrap();
        assert!(json.contains("\"name\""));
        assert!(json.contains("\"description\""));
        assert!(json.contains("\"extensions\""));
        assert!(json.contains("\"metadata\""));

        let deserialized: Artifact = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.artifact_id, "art-full");
        assert_eq!(deserialized.name.as_deref(), Some("Full Artifact"));
        assert_eq!(deserialized.description.as_deref(), Some("An artifact with all fields"));
        assert_eq!(deserialized.parts.len(), 2);
        assert_eq!(deserialized.extensions.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.metadata.as_ref().unwrap()["version"], 2);
    }

    #[test]
    fn test_artifact_from_raw_json() {
        let json = r#"{
            "artifactId": "a-raw",
            "name": "Generated Report",
            "description": "Monthly report",
            "parts": [
                {"text": "Report content"},
                {"data": {"revenue": 50000, "growth": 0.15}}
            ],
            "metadata": {"generatedAt": "2026-02-12"},
            "extensions": ["urn:a2a:ext:reports"]
        }"#;

        let artifact: Artifact = serde_json::from_str(json).unwrap();
        assert_eq!(artifact.artifact_id, "a-raw");
        assert_eq!(artifact.name.as_deref(), Some("Generated Report"));
        assert_eq!(artifact.description.as_deref(), Some("Monthly report"));
        assert_eq!(artifact.parts.len(), 2);
        match &artifact.parts[0].content {
            PartContent::Text { text } => assert_eq!(text, "Report content"),
            _ => panic!("Expected Text part"),
        }
        match &artifact.parts[1].content {
            PartContent::Data { data } => {
                assert_eq!(data["revenue"], 50000);
            }
            _ => panic!("Expected Data part"),
        }
        assert_eq!(artifact.extensions.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_artifact_with_multiple_part_types() {
        let artifact = Artifact {
            artifact_id: "a-multi".into(),
            name: Some("Multi-part".into()),
            description: None,
            parts: vec![
                Part {
                    content: PartContent::Text {
                        text: "text part".into(),
                    },
                    metadata: None,
                    filename: None,
                    media_type: None,
                },
                Part {
                    content: PartContent::Url {
                        url: "https://example.com/file.pdf".into(),
                    },
                    metadata: None,
                    filename: Some("file.pdf".into()),
                    media_type: Some("application/pdf".into()),
                },
                Part {
                    content: PartContent::Data {
                        data: serde_json::json!({"key": "value"}),
                    },
                    metadata: None,
                    filename: None,
                    media_type: Some("application/json".into()),
                },
                Part {
                    content: PartContent::Raw {
                        raw: vec![0xDE, 0xAD, 0xBE, 0xEF],
                    },
                    metadata: None,
                    filename: Some("binary.bin".into()),
                    media_type: Some("application/octet-stream".into()),
                },
            ],
            metadata: None,
            extensions: None,
        };

        let json = serde_json::to_string(&artifact).unwrap();
        let deserialized: Artifact = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.parts.len(), 4);

        // Verify each part type roundtripped correctly
        assert!(matches!(&deserialized.parts[0].content, PartContent::Text { .. }));
        assert!(matches!(&deserialized.parts[1].content, PartContent::Url { .. }));
        assert!(matches!(&deserialized.parts[2].content, PartContent::Data { .. }));
        assert!(matches!(&deserialized.parts[3].content, PartContent::Raw { raw } if raw == &[0xDE, 0xAD, 0xBE, 0xEF]));
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

    #[test]
    fn test_artifact_from_minimal_raw_json() {
        // Only required fields: artifactId and parts
        let json = r#"{"artifactId": "a-min", "parts": []}"#;
        let artifact: Artifact = serde_json::from_str(json).unwrap();
        assert_eq!(artifact.artifact_id, "a-min");
        assert!(artifact.parts.is_empty());
        assert!(artifact.name.is_none());
        assert!(artifact.description.is_none());
        assert!(artifact.metadata.is_none());
        assert!(artifact.extensions.is_none());
    }

    #[test]
    fn test_artifact_from_raw_json_with_url_and_raw_parts() {
        let json = r#"{
            "artifactId": "a-mixed",
            "name": "Mixed Parts",
            "parts": [
                {"url": "https://cdn.example.com/output.png", "filename": "output.png", "mediaType": "image/png"},
                {"raw": "AQID", "filename": "binary.bin", "mediaType": "application/octet-stream"}
            ]
        }"#;

        let artifact: Artifact = serde_json::from_str(json).unwrap();
        assert_eq!(artifact.artifact_id, "a-mixed");
        assert_eq!(artifact.parts.len(), 2);

        match &artifact.parts[0].content {
            PartContent::Url { url } => assert!(url.contains("output.png")),
            _ => panic!("Expected Url part"),
        }
        assert_eq!(artifact.parts[0].filename.as_deref(), Some("output.png"));
        assert_eq!(artifact.parts[0].media_type.as_deref(), Some("image/png"));

        match &artifact.parts[1].content {
            PartContent::Raw { raw } => assert_eq!(raw, &[1, 2, 3]),
            _ => panic!("Expected Raw part"),
        }
    }

    #[test]
    fn test_artifact_with_nested_data_part() {
        let json = r#"{
            "artifactId": "a-nested",
            "parts": [
                {"data": {"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}], "total": 2}}
            ]
        }"#;

        let artifact: Artifact = serde_json::from_str(json).unwrap();
        match &artifact.parts[0].content {
            PartContent::Data { data } => {
                assert_eq!(data["users"].as_array().unwrap().len(), 2);
                assert_eq!(data["users"][0]["name"], "Alice");
                assert_eq!(data["total"], 2);
            }
            _ => panic!("Expected Data part"),
        }
    }

    #[test]
    fn test_artifact_missing_required_artifact_id_fails() {
        let json = r#"{"parts": [{"text": "hello"}]}"#;
        let result: Result<Artifact, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_artifact_missing_required_parts_fails() {
        let json = r#"{"artifactId": "a-1"}"#;
        let result: Result<Artifact, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_artifact_unknown_fields_ignored() {
        let json = r#"{
            "artifactId": "a-unk",
            "parts": [{"text": "data"}],
            "unknownField": "should be ignored",
            "extraNumber": 42
        }"#;
        let artifact: Artifact = serde_json::from_str(json).unwrap();
        assert_eq!(artifact.artifact_id, "a-unk");
        assert_eq!(artifact.parts.len(), 1);
    }

    #[test]
    fn test_artifact_url_with_query_and_fragment() {
        let json = r#"{
            "artifactId": "a-url",
            "parts": [
                {"url": "https://cdn.example.com/files/report.pdf?token=abc123&format=pdf#page=5"}
            ]
        }"#;

        let artifact: Artifact = serde_json::from_str(json).unwrap();
        match &artifact.parts[0].content {
            PartContent::Url { url } => {
                assert!(url.contains("token=abc123"));
                assert!(url.contains("format=pdf"));
                assert!(url.contains("#page=5"));
            }
            _ => panic!("Expected Url part"),
        }
    }

    #[test]
    fn test_artifact_data_part_with_null_values() {
        let json = r#"{
            "artifactId": "a-null",
            "parts": [
                {"data": {"key": null, "nested": {"also": null}, "array": [null, 1, null]}}
            ]
        }"#;

        let artifact: Artifact = serde_json::from_str(json).unwrap();
        match &artifact.parts[0].content {
            PartContent::Data { data } => {
                assert!(data["key"].is_null());
                assert!(data["nested"]["also"].is_null());
                assert!(data["array"][0].is_null());
                assert_eq!(data["array"][1], 1);
            }
            _ => panic!("Expected Data part"),
        }
    }

    #[test]
    fn test_artifact_unicode_name_and_description() {
        let artifact = Artifact {
            artifact_id: "a-unicode".into(),
            name: Some("レポート 📊".into()),
            description: Some("Résumé des données – année 2026".into()),
            parts: vec![Part {
                content: PartContent::Text {
                    text: "内容".into(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            }],
            metadata: None,
            extensions: None,
        };

        let json = serde_json::to_string(&artifact).unwrap();
        let roundtripped: Artifact = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtripped.name.as_deref(), Some("レポート 📊"));
        assert!(roundtripped.description.unwrap().contains("Résumé"));
    }

    #[test]
    fn test_artifact_with_empty_extensions_and_metadata() {
        let artifact = Artifact {
            artifact_id: "a-ext".into(),
            name: None,
            description: None,
            parts: vec![Part {
                content: PartContent::Text {
                    text: "hello".into(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            }],
            metadata: Some(serde_json::json!({})),
            extensions: Some(vec![]),
        };

        let json = serde_json::to_string(&artifact).unwrap();
        assert!(json.contains("\"metadata\":{}"));
        assert!(json.contains("\"extensions\":[]"));

        let deserialized: Artifact = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.metadata.as_ref().unwrap(),
            &serde_json::json!({})
        );
        assert!(deserialized.extensions.as_ref().unwrap().is_empty());
    }
}
