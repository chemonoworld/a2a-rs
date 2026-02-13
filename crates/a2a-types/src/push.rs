use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPushNotificationConfig {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<PushNotificationAuthInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushNotificationAuthInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schemes: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_config_serde() {
        let config = TaskPushNotificationConfig {
            url: "https://example.com/webhook".into(),
            token: Some("secret-token".into()),
            authentication: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TaskPushNotificationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.url, "https://example.com/webhook");
        assert_eq!(deserialized.token.as_deref(), Some("secret-token"));
    }

    #[test]
    fn test_push_config_minimal() {
        let json = r#"{"url": "https://hooks.example.com/a2a"}"#;
        let config: TaskPushNotificationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.url, "https://hooks.example.com/a2a");
        assert!(config.token.is_none());
        assert!(config.authentication.is_none());
    }

    #[test]
    fn test_push_config_with_auth() {
        let config = TaskPushNotificationConfig {
            url: "https://example.com/webhook".into(),
            token: None,
            authentication: Some(PushNotificationAuthInfo {
                schemes: Some(vec!["bearer".into(), "basic".into()]),
            }),
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"authentication\""));
        assert!(json.contains("\"schemes\""));
        assert!(!json.contains("\"token\""));

        let deserialized: TaskPushNotificationConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.token.is_none());
        let auth = deserialized.authentication.unwrap();
        assert_eq!(auth.schemes.unwrap().len(), 2);
    }

    #[test]
    fn test_push_config_optional_fields_omitted() {
        let config = TaskPushNotificationConfig {
            url: "https://example.com/hook".into(),
            token: None,
            authentication: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("token"));
        assert!(!json.contains("authentication"));
    }

    #[test]
    fn test_push_config_from_raw_json_all_fields() {
        let json = r#"{
            "url": "https://webhook.example.com/a2a-notify",
            "token": "bearer-abc-123",
            "authentication": {
                "schemes": ["bearer", "basic", "oauth2"]
            }
        }"#;

        let config: TaskPushNotificationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.url, "https://webhook.example.com/a2a-notify");
        assert_eq!(config.token.as_deref(), Some("bearer-abc-123"));

        let auth = config.authentication.unwrap();
        let schemes = auth.schemes.unwrap();
        assert_eq!(schemes.len(), 3);
        assert_eq!(schemes[0], "bearer");
        assert_eq!(schemes[2], "oauth2");
    }

    #[test]
    fn test_push_auth_info_empty_schemes() {
        let auth = PushNotificationAuthInfo {
            schemes: Some(vec![]),
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"schemes\":[]"));

        let deserialized: PushNotificationAuthInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.schemes.as_ref().unwrap().len(), 0);

        // Also test with schemes omitted entirely
        let auth_none = PushNotificationAuthInfo { schemes: None };
        let json_none = serde_json::to_string(&auth_none).unwrap();
        assert_eq!(json_none, "{}");
    }

    #[test]
    fn test_push_config_missing_url_fails() {
        let json = r#"{"token": "abc"}"#;
        let result: Result<TaskPushNotificationConfig, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing required url should fail");
    }

    #[test]
    fn test_push_config_unknown_fields_ignored() {
        let json = r#"{
            "url": "https://hooks.example.com/a2a",
            "token": "secret",
            "extraField": "ignored",
            "anotherExtra": 42
        }"#;
        let config: TaskPushNotificationConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.url, "https://hooks.example.com/a2a");
        assert_eq!(config.token.as_deref(), Some("secret"));
    }
}
