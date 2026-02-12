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
}
