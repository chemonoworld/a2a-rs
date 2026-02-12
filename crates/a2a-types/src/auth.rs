use serde::{Deserialize, Serialize};

/// Security scheme - v1.0 supports 5 types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SecurityScheme {
    #[serde(rename = "apiKey")]
    ApiKey(ApiKeySecurityScheme),
    #[serde(rename = "http")]
    Http(HttpAuthSecurityScheme),
    #[serde(rename = "oauth2")]
    OAuth2(OAuth2SecurityScheme),
    #[serde(rename = "openIdConnect")]
    OpenIdConnect(OpenIdConnectSecurityScheme),
    #[serde(rename = "mutualTLS")]
    MutualTls(MutualTlsSecurityScheme),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    Header,
    Query,
    Cookie,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeySecurityScheme {
    pub name: String,
    #[serde(rename = "in")]
    pub location: ApiKeyLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpAuthSecurityScheme {
    pub scheme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2SecurityScheme {
    pub flows: OAuthFlows,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthFlows {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_code: Option<AuthorizationCodeOAuthFlow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_credentials: Option<ClientCredentialsOAuthFlow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_code: Option<DeviceCodeOAuthFlow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationCodeOAuthFlow {
    pub authorization_url: String,
    pub token_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pkce_required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCredentialsOAuthFlow {
    pub token_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeOAuthFlow {
    pub device_authorization_url: String,
    pub token_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenIdConnectSecurityScheme {
    pub openid_connect_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutualTlsSecurityScheme {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_scheme_serde() {
        let scheme = SecurityScheme::ApiKey(ApiKeySecurityScheme {
            name: "X-API-Key".into(),
            location: ApiKeyLocation::Header,
            description: None,
        });

        let json = serde_json::to_string(&scheme).unwrap();
        assert!(json.contains("\"type\":\"apiKey\""));
        assert!(json.contains("\"in\":\"header\""));

        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::ApiKey(s) => {
                assert_eq!(s.name, "X-API-Key");
                assert_eq!(s.location, ApiKeyLocation::Header);
            }
            _ => panic!("Expected ApiKey"),
        }
    }

    #[test]
    fn test_http_auth_scheme_serde() {
        let scheme = SecurityScheme::Http(HttpAuthSecurityScheme {
            scheme: "bearer".into(),
            bearer_format: Some("JWT".into()),
            description: None,
        });

        let json = serde_json::to_string(&scheme).unwrap();
        assert!(json.contains("\"type\":\"http\""));

        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::Http(s) => assert_eq!(s.scheme, "bearer"),
            _ => panic!("Expected Http"),
        }
    }

    #[test]
    fn test_oauth2_scheme_serde() {
        let scheme = SecurityScheme::OAuth2(OAuth2SecurityScheme {
            flows: OAuthFlows {
                authorization_code: Some(AuthorizationCodeOAuthFlow {
                    authorization_url: "https://auth.example.com/authorize".into(),
                    token_url: "https://auth.example.com/token".into(),
                    scopes: None,
                    pkce_required: Some(true),
                }),
                client_credentials: None,
                device_code: None,
            },
            description: None,
        });

        let json = serde_json::to_string(&scheme).unwrap();
        assert!(json.contains("\"type\":\"oauth2\""));

        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::OAuth2(s) => {
                assert!(s.flows.authorization_code.is_some());
            }
            _ => panic!("Expected OAuth2"),
        }
    }
}
