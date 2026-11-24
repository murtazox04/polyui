use crate::provider::{
    ErrorKind, ModelEvent, ModelProvider, ModelRequest, ProviderError, StopReason, Usage,
};
use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
pub const API_VERSION: &str = "2026-10-21";
pub const DEFAULT_MODEL: &str = "claude-haiku-5-5";
pub const KEY_VARIABLE: &str = "ANTHROPIC_API_KEY";
const MAX_ATTEMPTS: u32 = 3;
const MAX_ERROR_BODY_CHARS: usize = 600;

#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Option<Self> {
        let key = key.into().trim().to_string();
        (!key.is_empty()).then_some(Self(key))
    }

    pub fn from_env() -> Option<Self> {
        std::env::var(KEY_VARIABLE).ok().and_then(Self::new)
    }

    pub fn from_env_file(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        read_env_text(&text)
            .into_iter()
            .find(|(name, _)| name == KEY_VARIABLE)
            .and_then(|(_, value)| Self::new(value))
            .ok_or_else(|| format!("{} does not set {KEY_VARIABLE}", path.display()))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(hidden)")
    }
}

pub fn read_env_text(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (name, value) = line.split_once('=')?;
            let value = value.trim();
            let unquoted = [('"', '"'), ('\'', '\'')]
                .iter()
                .find_map(|(open, close)| {
                    value
                        .strip_prefix(*open)
                        .and_then(|rest| rest.strip_suffix(*close))
                })
                .unwrap_or(value);
            Some((name.trim().to_string(), unquoted.to_string()))
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    client: reqwest::Client,
    key: ApiKey,
    model: String,
    endpoint: String,
    effort: Option<String>,
    strict_tools: Arc<AtomicBool>,
}

impl AnthropicProvider {
    pub fn new(key: ApiKey, model: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(120))
            .build()
            .unwrap_or_default();
        Self {
            client,
            key,
            model: model.into(),
            endpoint: ENDPOINT.to_string(),
            effort: None,
            strict_tools: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn with_effort(mut self, effort: impl Into<String>) -> Self {
        self.effort = Some(effort.into());
        self
    }

    pub fn strict_tools(&self) -> bool {
        self.strict_tools.load(Ordering::Relaxed)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn body(&self, request: &ModelRequest) -> Value {
        let mut body = Map::new();
        body.insert("model".into(), Value::from(self.model.clone()));
        body.insert("max_tokens".into(), Value::from(request.max_tokens));
        body.insert("stream".into(), Value::Bool(true));
        if !request.system.is_empty() {
            body.insert("system".into(), Value::from(request.system.clone()));
        }
        body.insert("messages".into(), Value::from(request.messages.clone()));
        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .cloned()
                .map(|mut tool| {
                    if let Value::Object(map) = &mut tool {
                        map.insert("eager_input_streaming".into(), Value::Bool(true));
                        if !self.strict_tools() {
                            map.remove("strict");
                        }
                    }
                    tool
                })
                .collect();
            body.insert("tools".into(), Value::from(tools));
            body.insert("tool_choice".into(), json!({ "type": "auto" }));
        }
        body.insert("cache_control".into(), json!({ "type": "ephemeral" }));
        if let Some(effort) = &self.effort {
            body.insert("output_config".into(), json!({ "effort": effort }));
        }
        Value::Object(body)
    }

    async fn run(self, request: ModelRequest, events: mpsc::UnboundedSender<ModelEvent>) {
        let mut body = self.body(&request);
        let mut attempt = 0;
        let response = loop {
            attempt += 1;
            let sent = self
                .client
                .post(&self.endpoint)
                .header("x-api-key", self.key.expose())
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await;
            let failure = match sent {
                Ok(response) if response.status().is_success() => break response,
                Ok(response) => {
                    let status = response.status().as_u16();
                    let text = response.text().await.unwrap_or_default();
                    ProviderError::new(status_kind(status), error_detail(status, &text))
                }
                Err(error) => {
                    ProviderError::new(ErrorKind::Network, error.without_url().to_string())
                }
            };
            if self.strict_tools() && is_strict_schema_refusal(&failure) {
                self.strict_tools.store(false, Ordering::Relaxed);
                eprintln!(
                    "polyui: the API refused strict tool schemas ({}); sending the same schemas without strict from now on",
                    failure.detail
                );
                body = self.body(&request);
                continue;
            }
            let retry = matches!(
                failure.kind,
                ErrorKind::RateLimited | ErrorKind::Overloaded | ErrorKind::Network
            );
            if !retry || attempt >= MAX_ATTEMPTS {
                let _ = events.send(ModelEvent::Error(failure));
                return;
            }
            tokio::time::sleep(Duration::from_millis(800 * u64::from(attempt))).await;
        };
        let mut decoder = SseDecoder::default();
        let mut assembler = Assembler::default();
        let mut bytes = response.bytes_stream();
        while let Some(chunk) = bytes.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    let _ = events.send(ModelEvent::Error(ProviderError::new(
                        ErrorKind::Network,
                        error.without_url().to_string(),
                    )));
                    return;
                }
            };
            for data in decoder.push(&chunk) {
                for event in assembler.handle_data(&data) {
                    let last = matches!(
                        event,
                        ModelEvent::MessageFinished { .. } | ModelEvent::Error(_)
                    );
                    if events.send(event).is_err() || last {
                        return;
                    }
                }
            }
        }
        let _ = events.send(ModelEvent::Error(ProviderError::new(
            ErrorKind::Protocol,
            "the stream ended before message_stop",
        )));
    }
}

impl ModelProvider for AnthropicProvider {
    fn stream(&self, request: ModelRequest) -> BoxStream<'static, ModelEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        tokio::spawn(self.clone().run(request, sender));
        UnboundedReceiverStream::new(receiver).boxed()
    }
}

fn is_strict_schema_refusal(error: &ProviderError) -> bool {
    error.kind == ErrorKind::BadRequest
        && (error.detail.contains("optional parameters") || error.detail.contains("grammar"))
}

fn status_kind(status: u16) -> ErrorKind {
    match status {
        401 | 403 => ErrorKind::Auth,
        429 => ErrorKind::RateLimited,
        529 | 503 => ErrorKind::Overloaded,
        400 | 404 | 413 | 422 => ErrorKind::BadRequest,
        _ => ErrorKind::Other,
    }
}

fn error_detail(status: u16, body: &str) -> String {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.chars().take(MAX_ERROR_BODY_CHARS).collect());
    format!("HTTP {status}: {message}")
}

fn error_kind(name: &str) -> ErrorKind {
    match name {
        "authentication_error" | "permission_error" => ErrorKind::Auth,
        "rate_limit_error" => ErrorKind::RateLimited,
        "overloaded_error" => ErrorKind::Overloaded,
        "invalid_request_error" | "not_found_error" | "request_too_large" => ErrorKind::BadRequest,
        _ => ErrorKind::Other,
    }
}

#[derive(Debug, Default)]
pub struct SseDecoder {
    pending: Vec<u8>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = self.pending.drain(..=end).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line);
            if line.is_empty() {
                if !self.data.is_empty() {
                    out.push(self.data.join("\n"));
                    self.data.clear();
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("data:") {
                self.data
                    .push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
            }
        }
        out
    }
}

#[derive(Debug)]
struct Block {
    value: Value,
    json: Option<String>,
}

#[derive(Debug, Default)]
pub struct Assembler {
    blocks: BTreeMap<u64, Block>,
    stop_reason: Option<StopReason>,
    usage: Usage,
}

fn append(target: &mut Value, key: &str, text: &str) {
    if let Value::Object(map) = target {
        let slot = map
            .entry(key.to_string())
            .or_insert_with(|| Value::from(""));
        match slot {
            Value::String(existing) => existing.push_str(text),
            other => *other = Value::from(text),
        }
    }
}

impl Assembler {
    pub fn handle_data(&mut self, data: &str) -> Vec<ModelEvent> {
        match serde_json::from_str::<Value>(data) {
            Ok(event) => self.handle(&event),
            Err(error) => vec![ModelEvent::Error(ProviderError::new(
                ErrorKind::Protocol,
                format!("unreadable stream event: {error}"),
            ))],
        }
    }

    pub fn handle(&mut self, event: &Value) -> Vec<ModelEvent> {
        let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
        match event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "message_start" => {
                if let Some(usage) = event.pointer("/message/usage") {
                    self.usage.merge_from(usage);
                }
                Vec::new()
            }
            "content_block_start" => {
                let value = event.get("content_block").cloned().unwrap_or(Value::Null);
                let is_tool = value.get("type").and_then(Value::as_str) == Some("tool_use");
                let mut out = Vec::new();
                if is_tool {
                    out.push(ModelEvent::ToolCallStarted {
                        id: text_field(&value, "id"),
                        name: text_field(&value, "name"),
                    });
                }
                self.blocks.insert(
                    index,
                    Block {
                        value,
                        json: is_tool.then(String::new),
                    },
                );
                out
            }
            "content_block_delta" => self.delta(index, event.get("delta").unwrap_or(&Value::Null)),
            "content_block_stop" => self.stop(index),
            "message_delta" => {
                if let Some(reason) = event.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(StopReason::parse(reason));
                }
                if let Some(usage) = event.get("usage") {
                    self.usage.merge_from(usage);
                }
                Vec::new()
            }
            "message_stop" => vec![ModelEvent::MessageFinished {
                stop_reason: self
                    .stop_reason
                    .clone()
                    .unwrap_or_else(|| StopReason::Other("missing".into())),
                content: std::mem::take(&mut self.blocks)
                    .into_values()
                    .map(|block| block.value)
                    .collect(),
                usage: self.usage,
            }],
            "error" => {
                let kind = event
                    .pointer("/error/type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let message = event
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("stream error");
                vec![ModelEvent::Error(ProviderError::new(
                    error_kind(kind),
                    format!("{kind}: {message}"),
                ))]
            }
            _ => Vec::new(),
        }
    }

    fn delta(&mut self, index: u64, delta: &Value) -> Vec<ModelEvent> {
        let Some(block) = self.blocks.get_mut(&index) else {
            return Vec::new();
        };
        let text = |key: &str| delta.get(key).and_then(Value::as_str).unwrap_or_default();
        match delta
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "text_delta" => {
                append(&mut block.value, "text", text("text"));
                vec![ModelEvent::TextDelta(text("text").to_string())]
            }
            "thinking_delta" => {
                append(&mut block.value, "thinking", text("thinking"));
                Vec::new()
            }
            "signature_delta" => {
                append(&mut block.value, "signature", text("signature"));
                Vec::new()
            }
            "input_json_delta" => {
                let piece = text("partial_json");
                if let Some(json) = &mut block.json {
                    json.push_str(piece);
                }
                vec![ModelEvent::ToolInputDelta {
                    id: text_field(&block.value, "id"),
                    partial_json: piece.to_string(),
                }]
            }
            "citations_delta" => {
                if let (Value::Object(map), Some(citation)) =
                    (&mut block.value, delta.get("citation"))
                {
                    let list = map
                        .entry("citations")
                        .or_insert_with(|| Value::Array(Vec::new()));
                    if let Value::Array(items) = list {
                        items.push(citation.clone());
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn stop(&mut self, index: u64) -> Vec<ModelEvent> {
        let Some(block) = self.blocks.get_mut(&index) else {
            return Vec::new();
        };
        let Some(json) = block.json.take() else {
            return Vec::new();
        };
        let id = text_field(&block.value, "id");
        let name = text_field(&block.value, "name");
        let input = if json.trim().is_empty() {
            Ok(block
                .value
                .get("input")
                .cloned()
                .unwrap_or_else(|| json!({})))
        } else {
            match serde_json::from_str::<Value>(&json) {
                Ok(value @ Value::Object(_)) => Ok(value),
                _ => Err(json),
            }
        };
        if let Value::Object(map) = &mut block.value {
            map.insert(
                "input".into(),
                input.as_ref().cloned().unwrap_or_else(|_| json!({})),
            );
        }
        vec![ModelEvent::ToolCallFinished { id, name, input }]
    }
}

fn text_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STREAM: &str = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"usage\":{\"input_tokens\":12,\"output_tokens\":1}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig==\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: ping\n",
        "data: {\"type\":\"ping\"}\n\n",
        "event: content_block_start\r\n",
        "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\r\n\r\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Here\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"close_ui\",\"input\":{}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"surf\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"ace\\\":\\\"s1\\\"}\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":40}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );

    fn run(stream: &str, chunk: usize) -> Vec<ModelEvent> {
        let mut decoder = SseDecoder::default();
        let mut assembler = Assembler::default();
        let mut events = Vec::new();
        for piece in stream.as_bytes().chunks(chunk) {
            for data in decoder.push(piece) {
                events.extend(assembler.handle_data(&data));
            }
        }
        events
    }

    #[test]
    fn assembles_thinking_text_and_tool_calls_in_any_chunking() {
        for chunk in [1, 7, 4096] {
            let events = run(STREAM, chunk);
            assert!(events.contains(&ModelEvent::TextDelta("Here".into())));
            assert!(events.contains(&ModelEvent::ToolCallFinished {
                id: "toolu_1".into(),
                name: "close_ui".into(),
                input: Ok(json!({ "surface": "s1" })),
            }));
            let Some(ModelEvent::MessageFinished {
                stop_reason,
                content,
                usage,
            }) = events.last()
            else {
                panic!("no finish: {events:?}");
            };
            assert_eq!(*stop_reason, StopReason::ToolUse);
            assert_eq!(usage.input_tokens, 12);
            assert_eq!(usage.output_tokens, 40);
            assert_eq!(
                content,
                &vec![
                    json!({ "type": "thinking", "thinking": "", "signature": "sig==" }),
                    json!({ "type": "text", "text": "Here" }),
                    json!({ "type": "tool_use", "id": "toolu_1", "name": "close_ui", "input": { "surface": "s1" } }),
                ]
            );
        }
    }

    #[test]
    fn broken_tool_json_is_reported_with_the_raw_text() {
        let mut assembler = Assembler::default();
        assembler.handle(&json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "tool_use", "id": "t", "name": "show_ui", "input": {} } }));
        assembler.handle(&json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "input_json_delta", "partial_json": "{\"root\": \"a" } }));
        let events = assembler.handle(&json!({ "type": "content_block_stop", "index": 0 }));
        assert_eq!(
            events,
            vec![ModelEvent::ToolCallFinished {
                id: "t".into(),
                name: "show_ui".into(),
                input: Err("{\"root\": \"a".into()),
            }]
        );
    }

    #[test]
    fn stream_errors_and_env_files() {
        let mut assembler = Assembler::default();
        let events = assembler.handle(
            &json!({ "type": "error", "error": { "type": "overloaded_error", "message": "Overloaded" } }),
        );
        assert!(
            matches!(&events[..], [ModelEvent::Error(error)] if error.kind == ErrorKind::Overloaded)
        );
        let pairs =
            read_env_text("# key\nexport ANTHROPIC_API_KEY=\"abc\"\nOTHER = 'x'\n\nbad line\n");
        assert_eq!(
            pairs,
            vec![
                ("ANTHROPIC_API_KEY".to_string(), "abc".to_string()),
                ("OTHER".to_string(), "x".to_string())
            ]
        );
        assert_eq!(
            format!("{:?}", ApiKey::new("secret").unwrap()),
            "ApiKey(hidden)"
        );
    }

    #[test]
    fn request_body_marks_tools_for_eager_streaming() {
        let provider = AnthropicProvider::new(ApiKey::new("k").unwrap(), DEFAULT_MODEL);
        let body = provider.body(&ModelRequest {
            system: "sys".into(),
            tools: vec![json!({ "name": "close_ui", "strict": true, "input_schema": {} })],
            messages: vec![json!({ "role": "user", "content": "hi" })],
            max_tokens: 100,
        });
        assert_eq!(body["tools"][0]["eager_input_streaming"], json!(true));
        assert_eq!(body["tools"][0]["strict"], json!(true));
        assert_eq!(body["tool_choice"], json!({ "type": "auto" }));
        assert_eq!(body["stream"], json!(true));
        assert!(!body.to_string().contains("\"k\""));
        assert!(is_strict_schema_refusal(&ProviderError::new(
            ErrorKind::BadRequest,
            "HTTP 400: Schemas contains too many optional parameters (322)"
        )));
        provider.strict_tools.store(false, Ordering::Relaxed);
        let loose = provider.body(&ModelRequest {
            system: String::new(),
            tools: vec![json!({ "name": "close_ui", "strict": true, "input_schema": {} })],
            messages: Vec::new(),
            max_tokens: 1,
        });
        assert_eq!(loose["tools"][0].get("strict"), None);
    }
}
