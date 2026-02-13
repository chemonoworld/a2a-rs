use async_trait::async_trait;

use crate::error::ServerError;

/// Request interceptor for before/after hooks on JSON-RPC method calls.
#[async_trait]
pub trait CallInterceptor: Send + Sync + 'static {
    async fn before(
        &self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<(), ServerError>;

    async fn after(
        &self,
        method: &str,
        result: &Result<serde_json::Value, ServerError>,
    ) -> Result<(), ServerError>;
}

#[cfg(test)]
mod tests {
    use super::CallInterceptor;
    use crate::error::ServerError;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// A mock interceptor that records calls and optionally rejects.
    struct RecordingInterceptor {
        before_calls: Mutex<Vec<(String, serde_json::Value)>>,
        after_calls: Mutex<Vec<String>>,
        reject_method: Option<String>,
    }

    impl RecordingInterceptor {
        fn new() -> Self {
            Self {
                before_calls: Mutex::new(Vec::new()),
                after_calls: Mutex::new(Vec::new()),
                reject_method: None,
            }
        }

        fn rejecting(method: &str) -> Self {
            Self {
                before_calls: Mutex::new(Vec::new()),
                after_calls: Mutex::new(Vec::new()),
                reject_method: Some(method.to_string()),
            }
        }
    }

    #[async_trait]
    impl CallInterceptor for RecordingInterceptor {
        async fn before(
            &self,
            method: &str,
            params: &serde_json::Value,
        ) -> Result<(), ServerError> {
            self.before_calls
                .lock()
                .unwrap()
                .push((method.to_string(), params.clone()));

            if self.reject_method.as_deref() == Some(method) {
                return Err(ServerError::Internal(format!(
                    "Rejected method: {method}"
                )));
            }
            Ok(())
        }

        async fn after(
            &self,
            method: &str,
            _result: &Result<serde_json::Value, ServerError>,
        ) -> Result<(), ServerError> {
            self.after_calls
                .lock()
                .unwrap()
                .push(method.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_interceptor_before_records_calls() {
        let interceptor = RecordingInterceptor::new();
        let params = serde_json::json!({"task_id": "t-1"});

        let result = interceptor.before("message/send", &params).await;
        assert!(result.is_ok());

        let calls = interceptor.before_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "message/send");
        assert_eq!(calls[0].1["task_id"], "t-1");
    }

    #[tokio::test]
    async fn test_interceptor_before_rejects_specific_method() {
        let interceptor = RecordingInterceptor::rejecting("tasks/cancel");

        // Allowed method passes
        let ok = interceptor.before("message/send", &serde_json::Value::Null).await;
        assert!(ok.is_ok());

        // Rejected method fails
        let err = interceptor.before("tasks/cancel", &serde_json::Value::Null).await;
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("Rejected method"));
        assert!(msg.contains("tasks/cancel"));
    }

    #[tokio::test]
    async fn test_interceptor_after_records_success_and_error() {
        let interceptor = RecordingInterceptor::new();

        // After with success result
        let success: Result<serde_json::Value, ServerError> =
            Ok(serde_json::json!({"status": "ok"}));
        interceptor.after("message/send", &success).await.unwrap();

        // After with error result
        let error: Result<serde_json::Value, ServerError> =
            Err(ServerError::TaskNotFound("t-1".into()));
        interceptor.after("tasks/get", &error).await.unwrap();

        let calls = interceptor.after_calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], "message/send");
        assert_eq!(calls[1], "tasks/get");
    }

    #[tokio::test]
    async fn test_interceptor_is_send_sync() {
        let interceptor: Arc<dyn CallInterceptor> = Arc::new(RecordingInterceptor::new());

        // Verify it can be shared across threads (Send + Sync)
        let interceptor_clone = interceptor.clone();
        let handle = tokio::spawn(async move {
            interceptor_clone
                .before("message/send", &serde_json::Value::Null)
                .await
        });
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
}
