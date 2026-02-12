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
}
