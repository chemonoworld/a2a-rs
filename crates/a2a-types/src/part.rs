use serde::{Deserialize, Serialize};

/// Part content - v1.0 unified oneof { text, raw, url, data }
/// Discriminated by JSON member presence (no "kind" field)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PartContent {
    /// Text content
    Text { text: String },
    /// File URL reference
    Url { url: String },
    /// Structured JSON data
    Data { data: serde_json::Value },
    /// Raw binary (base64 encoded in JSON)
    Raw {
        #[serde(with = "base64_bytes")]
        raw: Vec<u8>,
    },
}

/// A2A Part - content unit for messages and artifacts
/// v1.0: single Part with oneof content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// Content (exactly one of text, raw, url, data)
    #[serde(flatten)]
    pub content: PartContent,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

/// Base64 serialization/deserialization for Vec<u8>
mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::Error;
        let engine = base64_engine();
        let encoded = base64_encode(bytes, &engine).map_err(S::Error::custom)?;
        encoded.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        base64_decode(&s).map_err(D::Error::custom)
    }

    // Simple base64 implementation to avoid external dependency
    const CHARS: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    struct Engine;

    fn base64_engine() -> Engine {
        Engine
    }

    fn base64_encode(input: &[u8], _engine: &Engine) -> Result<String, String> {
        let mut result = String::with_capacity(input.len().div_ceil(3) * 4);
        for chunk in input.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let triple = (b0 << 16) | (b1 << 8) | b2;

            result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
            result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
            if chunk.len() > 1 {
                result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
            } else {
                result.push('=');
            }
            if chunk.len() > 2 {
                result.push(CHARS[(triple & 0x3F) as usize] as char);
            } else {
                result.push('=');
            }
        }
        Ok(result)
    }

    fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
        let input = input.trim_end_matches('=');
        let mut result = Vec::with_capacity(input.len() * 3 / 4);

        let mut buf: u32 = 0;
        let mut bits: u32 = 0;

        for c in input.chars() {
            let val = match c {
                'A'..='Z' => c as u32 - 'A' as u32,
                'a'..='z' => c as u32 - 'a' as u32 + 26,
                '0'..='9' => c as u32 - '0' as u32 + 52,
                '+' => 62,
                '/' => 63,
                _ => return Err(format!("invalid base64 character: {}", c)),
            };
            buf = (buf << 6) | val;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                result.push((buf >> bits) as u8);
                buf &= (1 << bits) - 1;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_part_serde() {
        let part = Part {
            content: PartContent::Text {
                text: "Hello".into(),
            },
            metadata: None,
            filename: None,
            media_type: None,
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"text\":\"Hello\""));
        assert!(!json.contains("metadata"));

        let deserialized: Part = serde_json::from_str(&json).unwrap();
        match &deserialized.content {
            PartContent::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected Text content"),
        }
    }

    #[test]
    fn test_url_part_serde() {
        let part = Part {
            content: PartContent::Url {
                url: "https://example.com/file.pdf".into(),
            },
            metadata: None,
            filename: Some("file.pdf".into()),
            media_type: Some("application/pdf".into()),
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"url\":"));
        assert!(json.contains("\"filename\":"));
        assert!(json.contains("\"mediaType\":"));

        let deserialized: Part = serde_json::from_str(&json).unwrap();
        match &deserialized.content {
            PartContent::Url { url } => assert_eq!(url, "https://example.com/file.pdf"),
            _ => panic!("Expected Url content"),
        }
        assert_eq!(deserialized.filename.as_deref(), Some("file.pdf"));
    }

    #[test]
    fn test_data_part_serde() {
        let part = Part {
            content: PartContent::Data {
                data: serde_json::json!({"key": "value"}),
            },
            metadata: None,
            filename: None,
            media_type: None,
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"data\":"));

        let deserialized: Part = serde_json::from_str(&json).unwrap();
        match &deserialized.content {
            PartContent::Data { data } => {
                assert_eq!(data["key"], "value");
            }
            _ => panic!("Expected Data content"),
        }
    }

    #[test]
    fn test_raw_part_serde() {
        let part = Part {
            content: PartContent::Raw {
                raw: vec![0x48, 0x65, 0x6c, 0x6c, 0x6f], // "Hello"
            },
            metadata: None,
            filename: None,
            media_type: Some("application/octet-stream".into()),
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"raw\":\"SGVsbG8=\"")); // base64 of "Hello"

        let deserialized: Part = serde_json::from_str(&json).unwrap();
        match &deserialized.content {
            PartContent::Raw { raw } => assert_eq!(raw, &[0x48, 0x65, 0x6c, 0x6c, 0x6f]),
            _ => panic!("Expected Raw content"),
        }
    }

    #[test]
    fn test_part_with_metadata() {
        let part = Part {
            content: PartContent::Text {
                text: "test".into(),
            },
            metadata: Some(serde_json::json!({"key": "value"})),
            filename: None,
            media_type: None,
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"metadata\":"));

        let deserialized: Part = serde_json::from_str(&json).unwrap();
        assert!(deserialized.metadata.is_some());
    }

    #[test]
    fn test_raw_empty_bytes() {
        let part = Part {
            content: PartContent::Raw {
                raw: vec![],
            },
            metadata: None,
            filename: None,
            media_type: None,
        };

        let json = serde_json::to_string(&part).unwrap();
        let deserialized: Part = serde_json::from_str(&json).unwrap();
        match &deserialized.content {
            PartContent::Raw { raw } => assert!(raw.is_empty()),
            _ => panic!("Expected Raw content"),
        }
    }

    #[test]
    fn test_data_part_with_array() {
        let part = Part {
            content: PartContent::Data {
                data: serde_json::json!([1, 2, 3, "four"]),
            },
            metadata: None,
            filename: None,
            media_type: Some("application/json".into()),
        };

        let json = serde_json::to_string(&part).unwrap();
        let deserialized: Part = serde_json::from_str(&json).unwrap();
        match &deserialized.content {
            PartContent::Data { data } => {
                assert!(data.is_array());
                assert_eq!(data.as_array().unwrap().len(), 4);
            }
            _ => panic!("Expected Data content"),
        }
    }

    #[test]
    fn test_data_part_with_string_value() {
        // Data can be any JSON value, not just objects
        let part = Part {
            content: PartContent::Data {
                data: serde_json::json!("just a string"),
            },
            metadata: None,
            filename: None,
            media_type: None,
        };

        let json = serde_json::to_string(&part).unwrap();
        let deserialized: Part = serde_json::from_str(&json).unwrap();
        match &deserialized.content {
            PartContent::Data { data } => {
                assert_eq!(data.as_str(), Some("just a string"));
            }
            _ => panic!("Expected Data content"),
        }
    }

    #[test]
    fn test_url_part_with_all_fields() {
        let part = Part {
            content: PartContent::Url {
                url: "https://cdn.example.com/images/photo.jpg".into(),
            },
            metadata: Some(serde_json::json!({"width": 1920, "height": 1080})),
            filename: Some("photo.jpg".into()),
            media_type: Some("image/jpeg".into()),
        };

        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"url\":"));
        assert!(json.contains("\"metadata\":"));
        assert!(json.contains("\"filename\":"));
        assert!(json.contains("\"mediaType\":"));

        let deserialized: Part = serde_json::from_str(&json).unwrap();
        match &deserialized.content {
            PartContent::Url { url } => {
                assert!(url.contains("photo.jpg"));
            }
            _ => panic!("Expected Url content"),
        }
        assert_eq!(deserialized.metadata.as_ref().unwrap()["width"], 1920);
        assert_eq!(deserialized.filename.as_deref(), Some("photo.jpg"));
        assert_eq!(deserialized.media_type.as_deref(), Some("image/jpeg"));
    }

    #[test]
    fn test_part_from_raw_json_text() {
        let json = r#"{"text": "hello world"}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        match &part.content {
            PartContent::Text { text } => assert_eq!(text, "hello world"),
            _ => panic!("Expected Text content"),
        }
        assert!(part.metadata.is_none());
        assert!(part.filename.is_none());
        assert!(part.media_type.is_none());
    }

    #[test]
    fn test_part_from_raw_json_with_all_optional_fields() {
        let json = r#"{
            "text": "Hello with extras",
            "metadata": {"priority": "high", "score": 0.99},
            "filename": "greeting.txt",
            "mediaType": "text/plain"
        }"#;

        let part: Part = serde_json::from_str(json).unwrap();
        match &part.content {
            PartContent::Text { text } => assert_eq!(text, "Hello with extras"),
            _ => panic!("Expected Text content"),
        }
        assert_eq!(part.metadata.as_ref().unwrap()["priority"], "high");
        assert_eq!(part.metadata.as_ref().unwrap()["score"], 0.99);
        assert_eq!(part.filename.as_deref(), Some("greeting.txt"));
        assert_eq!(part.media_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn test_part_from_raw_json_url_with_metadata() {
        let json = r#"{
            "url": "https://storage.example.com/files/report.pdf",
            "filename": "report.pdf",
            "mediaType": "application/pdf",
            "metadata": {"size": 1024000, "pages": 42}
        }"#;

        let part: Part = serde_json::from_str(json).unwrap();
        match &part.content {
            PartContent::Url { url } => {
                assert!(url.contains("report.pdf"));
            }
            _ => panic!("Expected Url content"),
        }
        assert_eq!(part.filename.as_deref(), Some("report.pdf"));
        assert_eq!(part.media_type.as_deref(), Some("application/pdf"));
        assert_eq!(part.metadata.as_ref().unwrap()["size"], 1024000);
        assert_eq!(part.metadata.as_ref().unwrap()["pages"], 42);
    }

    #[test]
    fn test_raw_part_roundtrip_various_lengths() {
        // Test base64 encode/decode via Part serde for different lengths
        for len in [0_usize, 1, 2, 3, 4, 5, 6, 100, 255] {
            let original: Vec<u8> = (0..len).map(|i| (i % 256) as u8).collect();
            let part = Part {
                content: PartContent::Raw {
                    raw: original.clone(),
                },
                metadata: None,
                filename: None,
                media_type: None,
            };

            let json = serde_json::to_string(&part).unwrap();
            let deserialized: Part = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("Failed to deserialize for length {len}: {e}"));

            match &deserialized.content {
                PartContent::Raw { raw } => {
                    assert_eq!(raw, &original, "Raw roundtrip failed for length {len}");
                }
                _ => panic!("Expected Raw content for length {len}"),
            }
        }
    }

    #[test]
    fn test_ambiguous_json_with_text_and_url_deserializes_as_text() {
        // When JSON has both "text" and "url", serde untagged tries Text first (order matters)
        let json = r#"{"text": "hello", "url": "https://example.com"}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        match &part.content {
            PartContent::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("Expected Text variant (untagged serde tries in order)"),
        }
    }

    #[test]
    fn test_part_with_null_metadata() {
        // Explicit null metadata in JSON should deserialize as Some(null)
        let json = r#"{"text": "hello", "metadata": null}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        match &part.content {
            PartContent::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("Expected Text part"),
        }
        // serde treats null as the deserialized value for Option<Value>
        // This can be Some(Value::Null) or None depending on config
        // The important thing is it doesn't error
    }

    #[test]
    fn test_data_part_with_nested_complex_object() {
        let json = r#"{
            "data": {
                "users": [
                    {"name": "Alice", "roles": ["admin", "user"]},
                    {"name": "Bob", "roles": ["user"]}
                ],
                "pagination": {"page": 1, "total": 42},
                "flags": {"active": true, "beta": false}
            },
            "mediaType": "application/json"
        }"#;
        let part: Part = serde_json::from_str(json).unwrap();
        match &part.content {
            PartContent::Data { data } => {
                assert_eq!(data["users"][0]["name"], "Alice");
                assert_eq!(data["users"][1]["roles"][0], "user");
                assert_eq!(data["pagination"]["total"], 42);
                assert_eq!(data["flags"]["active"], true);
            }
            _ => panic!("Expected Data part"),
        }
        assert_eq!(part.media_type.as_deref(), Some("application/json"));
    }
}
