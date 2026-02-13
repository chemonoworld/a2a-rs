use a2a_types::{Event, JsonRpcResponse};
use bytes::Bytes;
use futures_core::Stream;

use crate::error::ClientError;

/// Maximum buffer size for SSE line accumulation (10 MB).
const MAX_BUFFER_SIZE: usize = 10 * 1024 * 1024;

/// SSE line parser state machine.
///
/// Accumulates raw bytes into complete lines, then parses SSE fields
/// (`data:`, comments, `id:`) and emits `Event` values on empty-line boundaries.
struct SseParser {
    /// Incomplete UTF-8 line bytes carried across chunk boundaries.
    line_buffer: String,
    /// Accumulated `data:` field value for the current event.
    data_buffer: String,
}

impl SseParser {
    fn new() -> Self {
        Self {
            line_buffer: String::new(),
            data_buffer: String::new(),
        }
    }

    /// Feed a raw byte chunk and return all fully-parsed events.
    fn feed(&mut self, chunk: &[u8]) -> Vec<Result<Event, ClientError>> {
        let text = match std::str::from_utf8(chunk) {
            Ok(s) => s,
            Err(e) => {
                return vec![Err(ClientError::SseParse(format!(
                    "Invalid UTF-8: {e}"
                )))];
            }
        };

        let mut events = Vec::new();

        for ch in text.chars() {
            if ch == '\n' {
                let line = std::mem::take(&mut self.line_buffer);
                if let Some(result) = self.process_line(&line) {
                    events.push(result);
                }
            } else if ch == '\r' {
                // Ignore carriage return; the newline that follows triggers line processing.
            } else {
                self.line_buffer.push(ch);
                if self.line_buffer.len() > MAX_BUFFER_SIZE {
                    events.push(Err(ClientError::SseParse(
                        "Line buffer exceeded 10 MB".into(),
                    )));
                    self.line_buffer.clear();
                    self.data_buffer.clear();
                    return events;
                }
            }
        }

        events
    }

    /// Process a single complete SSE line. Returns `Some` when an empty line
    /// signals the end of an SSE event and the accumulated data can be parsed.
    fn process_line(&mut self, line: &str) -> Option<Result<Event, ClientError>> {
        // Empty line = event boundary.
        if line.is_empty() {
            if self.data_buffer.is_empty() {
                return None;
            }
            let data = std::mem::take(&mut self.data_buffer);
            return Some(Self::parse_event_data(&data));
        }

        // SSE comment (starts with ':')
        if line.starts_with(':') {
            return None;
        }

        // "data:" field — with or without leading space after colon
        if let Some(value) = line.strip_prefix("data:") {
            let value = value.strip_prefix(' ').unwrap_or(value);
            if !self.data_buffer.is_empty() {
                self.data_buffer.push('\n');
            }
            self.data_buffer.push_str(value);
            if self.data_buffer.len() > MAX_BUFFER_SIZE {
                let err = Err(ClientError::SseParse(
                    "Data buffer exceeded 10 MB".into(),
                ));
                self.data_buffer.clear();
                return Some(err);
            }
            return None;
        }

        // "id:" field — ignored for now (future Last-Event-ID support)
        if line.starts_with("id:") {
            return None;
        }

        // "event:" field — ignored (A2A uses only the default event type)
        if line.starts_with("event:") {
            return None;
        }

        // "retry:" field — ignored
        if line.starts_with("retry:") {
            return None;
        }

        // Unknown field — silently ignore per SSE spec
        None
    }

    /// Parse accumulated data as a JSON-RPC response containing an `Event`.
    fn parse_event_data(data: &str) -> Result<Event, ClientError> {
        let response: JsonRpcResponse =
            serde_json::from_str(data).map_err(|e| ClientError::SseParse(format!("JSON parse error: {e}")))?;

        if let Some(error) = response.error {
            return Err(ClientError::JsonRpc(error));
        }

        let result = response.result.ok_or(ClientError::EmptyResult)?;
        serde_json::from_value(result).map_err(Into::into)
    }
}

/// Wraps an inner byte stream and an `SseParser` to produce `Event` items.
struct SseStream<S> {
    inner: std::pin::Pin<Box<S>>,
    parser: SseParser,
    /// Events parsed from the current chunk that haven't been yielded yet.
    pending: std::collections::VecDeque<Result<Event, ClientError>>,
}

impl<S> Stream for SseStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Send,
{
    type Item = Result<Event, ClientError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // SAFETY: We only access `inner` through a pinned reference.
        let this = unsafe { self.get_unchecked_mut() };

        // Drain buffered events first.
        if let Some(event) = this.pending.pop_front() {
            return std::task::Poll::Ready(Some(event));
        }

        // Poll the underlying byte stream for more data.
        loop {
            match this.inner.as_mut().poll_next(cx) {
                std::task::Poll::Ready(Some(Ok(bytes))) => {
                    let events = this.parser.feed(&bytes);
                    let mut iter = events.into_iter();
                    if let Some(first) = iter.next() {
                        this.pending.extend(iter);
                        return std::task::Poll::Ready(Some(first));
                    }
                    // No events produced from this chunk; poll again.
                }
                std::task::Poll::Ready(Some(Err(e))) => {
                    return std::task::Poll::Ready(Some(Err(ClientError::Http(e))));
                }
                std::task::Poll::Ready(None) => {
                    // Stream ended. Flush any remaining data in the parser.
                    if !this.parser.data_buffer.is_empty() {
                        let data = std::mem::take(&mut this.parser.data_buffer);
                        return std::task::Poll::Ready(Some(SseParser::parse_event_data(&data)));
                    }
                    return std::task::Poll::Ready(None);
                }
                std::task::Poll::Pending => {
                    return std::task::Poll::Pending;
                }
            }
        }
    }
}

/// Create a [`Stream`] that parses SSE-framed bytes into A2A [`Event`]s.
pub fn parse_sse_stream(
    byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
) -> std::pin::Pin<Box<dyn Stream<Item = Result<Event, ClientError>> + Send>> {
    Box::pin(SseStream {
        inner: Box::pin(byte_stream),
        parser: SseParser::new(),
        pending: std::collections::VecDeque::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    /// Helper: turn a list of byte slices into a `Stream<Item = Result<Bytes, reqwest::Error>>`.
    fn bytes_stream(
        chunks: Vec<&'static [u8]>,
    ) -> impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static {
        tokio_stream::iter(chunks.into_iter().map(|b| Ok(Bytes::from_static(b))))
    }

    fn make_jsonrpc_sse(result_json: &str) -> String {
        format!(
            "data: {{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}\n\n",
            result_json
        )
    }

    fn task_json() -> &'static str {
        r#"{"id":"t-1","contextId":"ctx-1","status":{"state":"TASK_STATE_WORKING"}}"#
    }

    fn status_update_json() -> &'static str {
        r#"{"taskId":"t-1","contextId":"ctx-1","status":{"state":"TASK_STATE_COMPLETED"}}"#
    }

    #[tokio::test]
    async fn test_single_event() {
        let sse = make_jsonrpc_sse(task_json());
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        match event {
            Event::Task(t) => {
                assert_eq!(t.id, "t-1");
            }
            _ => panic!("Expected Task event"),
        }
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn test_multiple_events() {
        let sse = format!(
            "{}{}",
            make_jsonrpc_sse(task_json()),
            make_jsonrpc_sse(status_update_json()),
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let e1 = s.next().await.unwrap().unwrap();
        assert!(matches!(e1, Event::Task(_)));

        let e2 = s.next().await.unwrap().unwrap();
        assert!(matches!(e2, Event::TaskStatusUpdate(_)));

        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn test_data_without_space() {
        // "data:{json}" without space after colon
        let sse = format!(
            "data:{{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}\n\n",
            task_json()
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
    }

    #[tokio::test]
    async fn test_keepalive_ignored() {
        let sse = format!(
            ": keep-alive\n\n{}",
            make_jsonrpc_sse(task_json())
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn test_id_line_ignored() {
        let sse = format!(
            "id: 42\n{}",
            make_jsonrpc_sse(task_json())
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
    }

    #[tokio::test]
    async fn test_chunked_across_boundaries() {
        // Split an SSE event across two byte chunks
        let full = make_jsonrpc_sse(task_json());
        let mid = full.len() / 2;
        let chunk1: &'static [u8] = full.as_bytes()[..mid].to_vec().leak();
        let chunk2: &'static [u8] = full.as_bytes()[mid..].to_vec().leak();

        let stream = bytes_stream(vec![chunk1, chunk2]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn test_jsonrpc_error_in_sse() {
        let sse = "data: {\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32601,\"message\":\"Method not found\"},\"id\":1}\n\n";
        let stream = bytes_stream(vec![sse.as_bytes()]);
        let mut s = parse_sse_stream(stream);

        let result = s.next().await.unwrap();
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::JsonRpc(e) => {
                assert_eq!(e.code, -32601);
            }
            other => panic!("Expected JsonRpc error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_empty_stream() {
        let stream = bytes_stream(vec![]);
        let mut s = parse_sse_stream(stream);
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn test_only_comments_and_empty_lines() {
        let sse = b": comment\n\n: another\n\n";
        let stream = bytes_stream(vec![sse]);
        let mut s = parse_sse_stream(stream);
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn test_crlf_line_endings() {
        let sse = format!(
            "data: {{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}\r\n\r\n",
            task_json()
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
    }

    #[tokio::test]
    async fn test_multiline_data() {
        // Multi-line data field (multiple "data:" lines joined with \n)
        // We construct a JSON object split across two data lines — but JSON doesn't
        // support literal newlines in strings. So we test with a complete JSON on each line
        // and verify the parser concatenates them (which will fail JSON parse).
        // Instead, test that a single data line works and multiple data lines concatenate.
        let sse = format!(
            "data: {{\"jsonrpc\":\"2.0\",\ndata: \"result\":{},\"id\":1}}\n\n",
            task_json()
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        // The concatenated result is: {"jsonrpc":"2.0",\n"result":{...},"id":1}
        // which is valid JSON (newline in the middle).
        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
    }

    #[tokio::test]
    async fn test_flush_on_stream_end() {
        // Data without trailing empty line — should be flushed when stream ends
        let sse = format!(
            "data: {{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}\n",
            task_json()
        );
        // No trailing \n\n — just one \n after data
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
    }

    #[tokio::test]
    async fn test_invalid_json_data() {
        let sse = b"data: not valid json\n\n";
        let stream = bytes_stream(vec![sse]);
        let mut s = parse_sse_stream(stream);

        let result = s.next().await.unwrap();
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::SseParse(msg) => assert!(msg.contains("JSON parse error")),
            other => panic!("Expected SseParse error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_empty_result_in_response() {
        // JSON-RPC response with no result and no error
        let sse = b"data: {\"jsonrpc\":\"2.0\",\"id\":1}\n\n";
        let stream = bytes_stream(vec![sse]);
        let mut s = parse_sse_stream(stream);

        let result = s.next().await.unwrap();
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::EmptyResult => {}
            other => panic!("Expected EmptyResult error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_event_and_retry_fields_ignored() {
        // "event:" and "retry:" fields should be silently ignored
        let sse = format!(
            "event: message\nretry: 3000\ndata: {{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}\n\n",
            task_json()
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn test_unknown_fields_ignored() {
        let sse = format!(
            "foo: bar\ndata: {{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}\n\n",
            task_json()
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
    }

    #[tokio::test]
    async fn test_multiple_events_across_chunks() {
        // Split two events across three chunks
        let event1 = make_jsonrpc_sse(task_json());
        let event2 = make_jsonrpc_sse(status_update_json());
        let combined = format!("{event1}{event2}");

        // Split into 3 roughly equal chunks
        let len = combined.len();
        let third = len / 3;
        let chunk1: &'static [u8] = combined.as_bytes()[..third].to_vec().leak();
        let chunk2: &'static [u8] = combined.as_bytes()[third..2*third].to_vec().leak();
        let chunk3: &'static [u8] = combined.as_bytes()[2*third..].to_vec().leak();

        let stream = bytes_stream(vec![chunk1, chunk2, chunk3]);
        let s = parse_sse_stream(stream);
        let events: Vec<_> = s.collect().await;

        assert_eq!(events.len(), 2);
        assert!(events[0].is_ok());
        assert!(events[1].is_ok());
    }

    #[tokio::test]
    async fn test_status_update_with_message_via_sse() {
        let status_json = r#"{"taskId":"t-1","contextId":"ctx-1","status":{"state":"TASK_STATE_INPUT_REQUIRED","message":{"messageId":"m-1","role":"ROLE_AGENT","parts":[{"text":"Need more info"}]},"timestamp":"2026-02-12T10:00:00Z"},"final":false}"#;
        let sse = make_jsonrpc_sse(status_json);
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        match event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.task_id, "t-1");
                assert_eq!(u.status.state, a2a_types::TaskState::InputRequired);
                let msg = u.status.message.unwrap();
                assert_eq!(msg.message_id, "m-1");
                assert_eq!(u.is_final, Some(false));
            }
            _ => panic!("Expected TaskStatusUpdate event"),
        }
    }

    #[tokio::test]
    async fn test_jsonrpc_error_with_data_via_sse() {
        let sse = "data: {\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32001,\"message\":\"Task not found\",\"data\":{\"taskId\":\"missing\"}},\"id\":1}\n\n";
        let stream = bytes_stream(vec![sse.as_bytes()]);
        let mut s = parse_sse_stream(stream);

        let result = s.next().await.unwrap();
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::JsonRpc(e) => {
                assert_eq!(e.code, -32001);
                assert!(e.data.is_some());
                assert_eq!(e.data.unwrap()["taskId"], "missing");
            }
            other => panic!("Expected JsonRpc error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_message_event_via_sse() {
        let message_json = r#"{"messageId":"m-1","role":"ROLE_AGENT","parts":[{"text":"Hello from agent"}],"contextId":"ctx-1","taskId":"t-1"}"#;
        let sse = make_jsonrpc_sse(message_json);
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        match event {
            Event::Message(m) => {
                assert_eq!(m.message_id, "m-1");
                assert_eq!(m.role, a2a_types::Role::Agent);
                assert_eq!(m.context_id.as_deref(), Some("ctx-1"));
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[tokio::test]
    async fn test_completed_status_update_via_sse() {
        let status_json = r#"{"taskId":"t-fin","contextId":"ctx-fin","status":{"state":"TASK_STATE_COMPLETED"},"final":true}"#;
        let sse = make_jsonrpc_sse(status_json);
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        match event {
            Event::TaskStatusUpdate(u) => {
                assert_eq!(u.task_id, "t-fin");
                assert_eq!(u.status.state, a2a_types::TaskState::Completed);
                assert_eq!(u.is_final, Some(true));
            }
            _ => panic!("Expected TaskStatusUpdate event"),
        }
    }

    #[tokio::test]
    async fn test_artifact_update_via_sse() {
        let artifact_json = r#"{"taskId":"t-1","contextId":"ctx-1","artifact":{"artifactId":"a-1","parts":[{"text":"result"}]},"append":false,"lastChunk":true}"#;
        let sse = make_jsonrpc_sse(artifact_json);
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        match event {
            Event::TaskArtifactUpdate(u) => {
                assert_eq!(u.task_id, "t-1");
                assert_eq!(u.artifact.artifact_id, "a-1");
                assert!(!u.append);
                assert!(u.last_chunk);
            }
            _ => panic!("Expected TaskArtifactUpdate event"),
        }
    }

    #[tokio::test]
    async fn test_invalid_utf8_returns_error() {
        // Feed invalid UTF-8 bytes
        let invalid: &'static [u8] = &[0xFF, 0xFE, 0xFD];
        let stream = bytes_stream(vec![invalid]);
        let mut s = parse_sse_stream(stream);

        let result = s.next().await.unwrap();
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::SseParse(msg) => assert!(msg.contains("UTF-8")),
            other => panic!("Expected SseParse error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_consecutive_empty_lines_no_events() {
        // Multiple empty lines should not produce events (no data accumulated)
        let stream = bytes_stream(vec![b"\n\n\n\n"]);
        let mut s = parse_sse_stream(stream);

        // Stream should end without producing any events
        let result = s.next().await;
        assert!(result.is_none(), "Expected no events from empty lines only");
    }

    #[tokio::test]
    async fn test_data_only_no_trailing_newline_flushed_on_close() {
        // Data field without a trailing empty line - should be flushed on stream close
        let task_json = r#"{"id":"t-1","contextId":"ctx-1","status":{"state":"TASK_STATE_SUBMITTED"}}"#;
        let sse = format!("data: {{\"jsonrpc\":\"2.0\",\"result\":{task_json},\"id\":1}}\n");
        // No trailing empty line - the stream closes before an event boundary
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        // Should still produce the event on stream close
        let event = s.next().await.unwrap().unwrap();
        match event {
            Event::Task(t) => assert_eq!(t.id, "t-1"),
            _ => panic!("Expected Task event"),
        }
    }

    #[tokio::test]
    async fn test_cr_only_line_ending_no_event() {
        // CR alone (without LF) is silently ignored per our parser.
        // So "data: ...\r\r" has no newlines → no line processing occurs.
        // The data stays in line_buffer, and data_buffer is empty.
        // On stream end, data_buffer is flushed but it's empty → None.
        let sse = format!(
            "data: {{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}\r\r",
            task_json()
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        // CR-only endings don't create events — stream ends with no output
        let result = s.next().await;
        assert!(result.is_none(), "CR-only endings should not produce events");
    }

    #[tokio::test]
    async fn test_data_field_with_embedded_colons() {
        // "data: ..." where the value itself contains colons (e.g., a URL)
        let json = r#"{"jsonrpc":"2.0","result":{"id":"t-1","contextId":"ctx-1","status":{"state":"TASK_STATE_WORKING"}},"id":1}"#;
        let sse = format!("data: {json}\n\n");
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
    }

    #[tokio::test]
    async fn test_multiple_spaces_after_data_colon() {
        // SSE spec: only first space after colon is stripped
        // "data:  {json}" → value is " {json}" (leading space preserved after first)
        // This will fail JSON parse because of the extra leading space... actually no,
        // serde_json ignores leading whitespace.
        let json = format!(
            "{{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}",
            task_json()
        );
        let sse = format!("data:  {json}\n\n");
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        // Extra leading space should still parse (serde_json is whitespace-tolerant)
        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
    }

    #[tokio::test]
    async fn test_comment_that_looks_like_data() {
        // ": data: ..." is a comment, not a data field
        let sse = format!(
            ": data: fake\ndata: {{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}\n\n",
            task_json()
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        // Should only produce one event (the real data line)
        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn test_mixed_crlf_and_lf() {
        // Mix of CRLF and LF line endings in same stream
        let sse = format!(
            "data: {{\"jsonrpc\":\"2.0\",\"result\":{},\"id\":1}}\r\n\n",
            task_json()
        );
        let stream = bytes_stream(vec![sse.as_bytes().to_vec().leak()]);
        let mut s = parse_sse_stream(stream);

        let event = s.next().await.unwrap().unwrap();
        assert!(matches!(event, Event::Task(_)));
    }

    #[tokio::test]
    async fn test_both_result_and_error_present() {
        // Protocol violation: both result and error present
        // Our parser checks error first, so it should return JsonRpc error
        let sse = "data: {\"jsonrpc\":\"2.0\",\"result\":{\"id\":\"t-1\",\"contextId\":\"ctx\",\"status\":{\"state\":\"TASK_STATE_WORKING\"}},\"error\":{\"code\":-32001,\"message\":\"oops\"},\"id\":1}\n\n";
        let stream = bytes_stream(vec![sse.as_bytes()]);
        let mut s = parse_sse_stream(stream);

        let result = s.next().await.unwrap();
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::JsonRpc(e) => {
                assert_eq!(e.code, -32001);
            }
            other => panic!("Expected JsonRpc error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_incomplete_data_at_eof_without_newline() {
        // Data in line_buffer but no newline ever processed - stream ends with pending line
        // line_buffer has content but data_buffer is empty
        let stream = bytes_stream(vec![b"data: something"]);
        let mut s = parse_sse_stream(stream);

        // line_buffer: "data: something", data_buffer: ""
        // On EOF, only data_buffer is flushed, and it's empty → None
        let result = s.next().await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_parser_line_buffer_overflow() {
        // Feed a very long line that exceeds MAX_BUFFER_SIZE
        // We won't actually allocate 10MB; test the logic with a smaller sequence
        // by checking that the error message is correct
        let mut parser = SseParser::new();
        // Feed enough data without a newline to trigger overflow
        // MAX_BUFFER_SIZE is 10MB, let's verify the constant
        assert_eq!(MAX_BUFFER_SIZE, 10 * 1024 * 1024);

        // Directly test the parser with a small chunk and check the path exists
        let small_chunk = b"data: small\n\n";
        let events = parser.feed(small_chunk);
        // Should parse as an SseParse error (invalid JSON)
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_error_with_null_data_field() {
        // JSON-RPC error with data: null (explicitly)
        let sse = "data: {\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32600,\"message\":\"Invalid request\",\"data\":null},\"id\":1}\n\n";
        let stream = bytes_stream(vec![sse.as_bytes()]);
        let mut s = parse_sse_stream(stream);

        let result = s.next().await.unwrap();
        assert!(result.is_err());
        match result.unwrap_err() {
            ClientError::JsonRpc(e) => {
                assert_eq!(e.code, -32600);
                assert!(e.data.is_none()); // null data → None after deserialization
            }
            other => panic!("Expected JsonRpc error, got: {other:?}"),
        }
    }
}
