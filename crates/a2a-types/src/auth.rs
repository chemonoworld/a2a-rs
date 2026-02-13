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

    #[test]
    fn test_openid_connect_scheme_serde() {
        let scheme = SecurityScheme::OpenIdConnect(OpenIdConnectSecurityScheme {
            openid_connect_url: "https://auth.example.com/.well-known/openid-configuration"
                .into(),
            description: Some("OpenID Connect auth".into()),
        });

        let json = serde_json::to_string(&scheme).unwrap();
        assert!(json.contains("\"type\":\"openIdConnect\""));
        assert!(json.contains("openid-configuration"));

        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::OpenIdConnect(s) => {
                assert!(s.openid_connect_url.contains("openid-configuration"));
                assert_eq!(s.description.as_deref(), Some("OpenID Connect auth"));
            }
            _ => panic!("Expected OpenIdConnect"),
        }
    }

    #[test]
    fn test_mutual_tls_scheme_serde() {
        let scheme = SecurityScheme::MutualTls(MutualTlsSecurityScheme {
            description: Some("Client certificate required".into()),
        });

        let json = serde_json::to_string(&scheme).unwrap();
        assert!(json.contains("\"type\":\"mutualTLS\""));

        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::MutualTls(s) => {
                assert_eq!(
                    s.description.as_deref(),
                    Some("Client certificate required")
                );
            }
            _ => panic!("Expected MutualTls"),
        }
    }

    #[test]
    fn test_mutual_tls_minimal() {
        let scheme = SecurityScheme::MutualTls(MutualTlsSecurityScheme {
            description: None,
        });

        let json = serde_json::to_string(&scheme).unwrap();
        assert!(!json.contains("\"description\""));

        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::MutualTls(s) => assert!(s.description.is_none()),
            _ => panic!("Expected MutualTls"),
        }
    }

    #[test]
    fn test_api_key_all_locations() {
        for (loc, expected) in [
            (ApiKeyLocation::Header, "header"),
            (ApiKeyLocation::Query, "query"),
            (ApiKeyLocation::Cookie, "cookie"),
        ] {
            let scheme = SecurityScheme::ApiKey(ApiKeySecurityScheme {
                name: "key".into(),
                location: loc.clone(),
                description: None,
            });
            let json = serde_json::to_string(&scheme).unwrap();
            assert!(json.contains(&format!("\"in\":\"{expected}\"")));

            let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
            match deserialized {
                SecurityScheme::ApiKey(s) => assert_eq!(s.location, loc),
                _ => panic!("Expected ApiKey"),
            }
        }
    }

    #[test]
    fn test_oauth2_client_credentials_flow() {
        let scheme = SecurityScheme::OAuth2(OAuth2SecurityScheme {
            flows: OAuthFlows {
                authorization_code: None,
                client_credentials: Some(ClientCredentialsOAuthFlow {
                    token_url: "https://auth.example.com/token".into(),
                    scopes: Some({
                        let mut m = serde_json::Map::new();
                        m.insert("read".into(), serde_json::json!("Read access"));
                        m.insert("write".into(), serde_json::json!("Write access"));
                        m
                    }),
                }),
                device_code: None,
            },
            description: None,
        });

        let json = serde_json::to_string(&scheme).unwrap();
        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::OAuth2(s) => {
                assert!(s.flows.authorization_code.is_none());
                let cc = s.flows.client_credentials.unwrap();
                assert_eq!(cc.token_url, "https://auth.example.com/token");
                assert_eq!(cc.scopes.as_ref().unwrap().len(), 2);
            }
            _ => panic!("Expected OAuth2"),
        }
    }

    #[test]
    fn test_oauth2_device_code_flow() {
        let scheme = SecurityScheme::OAuth2(OAuth2SecurityScheme {
            flows: OAuthFlows {
                authorization_code: None,
                client_credentials: None,
                device_code: Some(DeviceCodeOAuthFlow {
                    device_authorization_url: "https://auth.example.com/device".into(),
                    token_url: "https://auth.example.com/token".into(),
                    scopes: None,
                }),
            },
            description: Some("Device flow".into()),
        });

        let json = serde_json::to_string(&scheme).unwrap();
        assert!(json.contains("deviceAuthorizationUrl"));

        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::OAuth2(s) => {
                assert!(s.flows.device_code.is_some());
                let dc = s.flows.device_code.unwrap();
                assert!(dc.device_authorization_url.contains("device"));
            }
            _ => panic!("Expected OAuth2"),
        }
    }

    #[test]
    fn test_oauth2_authorization_code_with_all_fields() {
        let scheme = SecurityScheme::OAuth2(OAuth2SecurityScheme {
            flows: OAuthFlows {
                authorization_code: Some(AuthorizationCodeOAuthFlow {
                    authorization_url: "https://auth.example.com/authorize".into(),
                    token_url: "https://auth.example.com/token".into(),
                    scopes: Some({
                        let mut m = serde_json::Map::new();
                        m.insert("openid".into(), serde_json::json!("OpenID scope"));
                        m.insert("profile".into(), serde_json::json!("Profile scope"));
                        m
                    }),
                    pkce_required: Some(true),
                }),
                client_credentials: Some(ClientCredentialsOAuthFlow {
                    token_url: "https://auth.example.com/token".into(),
                    scopes: None,
                }),
                device_code: Some(DeviceCodeOAuthFlow {
                    device_authorization_url: "https://auth.example.com/device".into(),
                    token_url: "https://auth.example.com/token".into(),
                    scopes: None,
                }),
            },
            description: Some("Full OAuth2 with all flows".into()),
        });

        let json = serde_json::to_string(&scheme).unwrap();
        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::OAuth2(s) => {
                assert!(s.flows.authorization_code.is_some());
                assert!(s.flows.client_credentials.is_some());
                assert!(s.flows.device_code.is_some());
                let ac = s.flows.authorization_code.unwrap();
                assert_eq!(ac.pkce_required, Some(true));
                assert_eq!(ac.scopes.as_ref().unwrap().len(), 2);
                assert_eq!(s.description.as_deref(), Some("Full OAuth2 with all flows"));
            }
            _ => panic!("Expected OAuth2"),
        }
    }

    #[test]
    fn test_api_key_with_description() {
        let scheme = SecurityScheme::ApiKey(ApiKeySecurityScheme {
            name: "Authorization".into(),
            location: ApiKeyLocation::Header,
            description: Some("API key in header".into()),
        });

        let json = serde_json::to_string(&scheme).unwrap();
        assert!(json.contains("\"description\""));

        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::ApiKey(s) => {
                assert_eq!(s.description.as_deref(), Some("API key in header"));
            }
            _ => panic!("Expected ApiKey"),
        }
    }

    #[test]
    fn test_security_scheme_from_raw_json_api_key() {
        let json = r#"{
            "type": "apiKey",
            "name": "X-Custom-Key",
            "in": "query",
            "description": "API key in query parameter"
        }"#;
        let scheme: SecurityScheme = serde_json::from_str(json).unwrap();
        match scheme {
            SecurityScheme::ApiKey(s) => {
                assert_eq!(s.name, "X-Custom-Key");
                assert_eq!(s.location, ApiKeyLocation::Query);
                assert_eq!(s.description.as_deref(), Some("API key in query parameter"));
            }
            _ => panic!("Expected ApiKey scheme"),
        }
    }

    #[test]
    fn test_security_scheme_from_raw_json_http_bearer() {
        let json = r#"{
            "type": "http",
            "scheme": "bearer",
            "bearerFormat": "JWT",
            "description": "JWT Bearer token"
        }"#;
        let scheme: SecurityScheme = serde_json::from_str(json).unwrap();
        match scheme {
            SecurityScheme::Http(s) => {
                assert_eq!(s.scheme, "bearer");
                assert_eq!(s.bearer_format.as_deref(), Some("JWT"));
                assert_eq!(s.description.as_deref(), Some("JWT Bearer token"));
            }
            _ => panic!("Expected Http scheme"),
        }
    }

    #[test]
    fn test_security_scheme_from_raw_json_openid_connect() {
        let json = r#"{
            "type": "openIdConnect",
            "openidConnectUrl": "https://accounts.example.com/.well-known/openid-configuration"
        }"#;
        let scheme: SecurityScheme = serde_json::from_str(json).unwrap();
        match scheme {
            SecurityScheme::OpenIdConnect(s) => {
                assert!(s.openid_connect_url.contains("openid-configuration"));
                assert!(s.description.is_none());
            }
            _ => panic!("Expected OpenIdConnect scheme"),
        }
    }

    #[test]
    fn test_invalid_security_scheme_type() {
        let json = r#"{"type":"unknown","value":"test"}"#;
        let result: Result<SecurityScheme, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_oauth2_empty_scopes_map() {
        let scheme = SecurityScheme::OAuth2(OAuth2SecurityScheme {
            flows: OAuthFlows {
                authorization_code: Some(AuthorizationCodeOAuthFlow {
                    authorization_url: "https://auth.example.com/authorize".into(),
                    token_url: "https://auth.example.com/token".into(),
                    scopes: Some(serde_json::Map::new()),
                    pkce_required: None,
                }),
                client_credentials: None,
                device_code: None,
            },
            description: None,
        });

        let json = serde_json::to_string(&scheme).unwrap();
        assert!(json.contains("\"scopes\":{}"));

        let deserialized: SecurityScheme = serde_json::from_str(&json).unwrap();
        match deserialized {
            SecurityScheme::OAuth2(s) => {
                let scopes = s.flows.authorization_code.unwrap().scopes.unwrap();
                assert!(scopes.is_empty());
            }
            _ => panic!("Expected OAuth2"),
        }
    }

    #[test]
    fn test_api_key_location_case_sensitivity() {
        // Correct lowercase should work
        let json = r#"{"type":"apiKey","name":"key","in":"header"}"#;
        let result: Result<SecurityScheme, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        // Incorrect case should fail
        let json_bad = r#"{"type":"apiKey","name":"key","in":"Header"}"#;
        let result_bad: Result<SecurityScheme, _> = serde_json::from_str(json_bad);
        assert!(result_bad.is_err(), "Case-sensitive: 'Header' should fail");

        let json_bad2 = r#"{"type":"apiKey","name":"key","in":"HEADER"}"#;
        let result_bad2: Result<SecurityScheme, _> = serde_json::from_str(json_bad2);
        assert!(result_bad2.is_err(), "Case-sensitive: 'HEADER' should fail");
    }

    #[test]
    fn test_oauth2_missing_required_token_url_fails() {
        // AuthorizationCodeOAuthFlow requires token_url
        let json = r#"{
            "type": "oauth2",
            "flows": {
                "authorizationCode": {
                    "authorizationUrl": "https://auth.example.com/authorize"
                }
            }
        }"#;
        let result: Result<SecurityScheme, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Missing token_url should fail deserialization");
    }

    #[test]
    fn test_http_scheme_basic_auth() {
        let json = r#"{"type":"http","scheme":"basic"}"#;
        let scheme: SecurityScheme = serde_json::from_str(json).unwrap();
        match scheme {
            SecurityScheme::Http(s) => {
                assert_eq!(s.scheme, "basic");
                assert!(s.bearer_format.is_none());
                assert!(s.description.is_none());
            }
            _ => panic!("Expected Http"),
        }
    }
}
