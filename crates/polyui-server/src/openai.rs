use crate::anthropic::ApiKey;
use crate::provider::{
    ErrorKind, ModelEvent, ModelProvider, ModelRequest, ProviderError, StopReason, Usage,
};
use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

pub const DEFAULT_KEY_VARIABLE: &str = "OPENAI_API_KEY";
pub const CHAT_PATH: &str = "/chat/completions";
const MAX_ATTEMPTS: u32 = 4;
const MAX_OVERLOADED_ATTEMPTS: u32 = 2;
const FIRST_WAIT: Duration = Duration::from_secs(1);
const MAX_ERROR_BODY_CHARS: usize = 600;
const BUSY_CODES: [&str; 2] = ["concurrency_limited", "rate_limit"];

#[derive(Debug, Clone)]
pub struct OpenAiCompatProvider {
    client: reqwest::Client,
    key: Option<ApiKey>,
    model: String,
    endpoint: String,
    response_format: Option<Value>,
    temperature: Option<f64>,
    first_wait: Duration,
    overloaded_attempts: u32,
    sent: Arc<AtomicUsize>,
}

pub fn chat_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with(CHAT_PATH) {
        base.to_string()
    } else {
        format!("{base}{CHAT_PATH}")
    }
}

pub fn json_schema_format(name: &str, schema: Value) -> Value {
    json!({
        "type": "json_schema",
        "json_schema": { "name": name, "schema": schema }
    })
}

impl OpenAiCompatProvider {
    pub fn new(base_url: &str, model: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_default();
        Self {
            client,
            key: None,
            model: model.into(),
            endpoint: chat_url(base_url),
            response_format: None,
            temperature: None,
            first_wait: FIRST_WAIT,
            overloaded_attempts: MAX_OVERLOADED_ATTEMPTS,
            sent: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn with_key(mut self, key: ApiKey) -> Self {
        self.key = Some(key);
        self
    }

    pub fn with_response_format(mut self, format: Value) -> Self {
        self.response_format = Some(format);
        self
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_first_wait(mut self, wait: Duration) -> Self {
        self.first_wait = wait;
        self
    }

    pub fn with_overloaded_attempts(mut self, attempts: u32) -> Self {
        self.overloaded_attempts = attempts.max(1);
        self
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn requests_sent(&self) -> usize {
        self.sent.load(Ordering::SeqCst)
    }

    pub fn body(&self, request: &ModelRequest) -> Value {
        let mut body = Map::new();
        body.insert("model".into(), Value::from(self.model.clone()));
        body.insert(
            "messages".into(),
            Value::Array(chat_messages(&request.system, &request.messages)),
        );
        body.insert("max_tokens".into(), Value::from(request.max_tokens));
        if let Some(temperature) = self.temperature {
            body.insert("temperature".into(), Value::from(temperature));
        }
        if !request.tools.is_empty() {
            body.insert("tools".into(), Value::Array(chat_tools(&request.tools)));
            body.insert("tool_choice".into(), Value::from("auto"));
        }
        if let Some(format) = &self.response_format {
            body.insert("response_format".into(), format.clone());
        }
        Value::Object(body)
    }

    async fn complete(&self, request: &ModelRequest) -> Result<Value, ProviderError> {
        let body = self.body(request);
        let mut attempt = 0;
        loop {
            attempt += 1;
            self.sent.fetch_add(1, Ordering::SeqCst);
            let mut builder = self.client.post(&self.endpoint).json(&body);
            if let Some(key) = &self.key {
                builder = builder.bearer_auth(key.expose());
            }
            let failure = match builder.send().await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    match response.text().await {
                        Ok(text) => match read_reply(status, &text) {
                            Ok(reply) => return Ok(reply),
                            Err(failure) => failure,
                        },
                        Err(error) => network_error(&error),
                    }
                }
                Err(error) => network_error(&error),
            };
            let limit = match failure.kind {
                ErrorKind::RateLimited => MAX_ATTEMPTS,
                ErrorKind::Overloaded => self.overloaded_attempts,
                _ => 1,
            };
            if attempt >= limit {
                return Err(failure);
            }
            tokio::time::sleep(self.first_wait * 2u32.pow(attempt - 1)).await;
        }
    }

    async fn run(&self, request: ModelRequest, events: &mpsc::UnboundedSender<ModelEvent>) {
        let events_out = match self.complete(&request).await {
            Ok(reply) => reply_events(&reply),
            Err(failure) => vec![ModelEvent::Error(failure)],
        };
        for event in events_out {
            if events.send(event).is_err() {
                return;
            }
        }
    }
}

impl ModelProvider for OpenAiCompatProvider {
    fn stream(&self, request: ModelRequest) -> BoxStream<'static, ModelEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let provider = self.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = sender.closed() => {}
                () = provider.run(request, &sender) => {}
            }
        });
        UnboundedReceiverStream::new(receiver).boxed()
    }
}

fn network_error(error: &reqwest::Error) -> ProviderError {
    let what = if error.is_timeout() {
        "timed out: "
    } else {
        ""
    };
    ProviderError::new(
        ErrorKind::Network,
        format!("{what}{}", error_without_url(error)),
    )
}

fn error_without_url(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    if let Some(url) = error.url() {
        text = text.replace(url.as_str(), "<endpoint>");
    }
    text
}

fn read_reply(status: u16, text: &str) -> Result<Value, ProviderError> {
    let value = serde_json::from_str::<Value>(text).ok();
    if (200..300).contains(&status)
        && let Some(reply) = value
            .as_ref()
            .filter(|reply| reply.get("choices").is_some())
    {
        return Ok(reply.clone());
    }
    Err(failure(status, text, value.as_ref()))
}

fn status_kind(status: u16) -> ErrorKind {
    match status {
        401 | 403 => ErrorKind::Auth,
        429 => ErrorKind::RateLimited,
        502..=504 | 529 => ErrorKind::Overloaded,
        400 | 404 | 413 | 422 => ErrorKind::BadRequest,
        200..=299 => ErrorKind::Protocol,
        _ => ErrorKind::Other,
    }
}

fn first_text(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        value.pointer(pointer).and_then(|found| match found {
            Value::String(text) => Some(text.clone()),
            Value::Null => None,
            other => Some(other.to_string()),
        })
    })
}

fn failure(status: u16, text: &str, value: Option<&Value>) -> ProviderError {
    let message = value
        .and_then(|value| first_text(value, &["/error/message", "/message", "/detail", "/error"]))
        .unwrap_or_else(|| text.chars().take(MAX_ERROR_BODY_CHARS).collect());
    let code = value.and_then(|value| {
        first_text(
            value,
            &[
                "/error_code",
                "/error/code",
                "/code",
                "/error/type",
                "/type",
            ],
        )
    });
    let lowered = text.to_lowercase();
    let busy = BUSY_CODES.iter().any(|code| lowered.contains(code));
    let kind = if busy {
        ErrorKind::RateLimited
    } else {
        status_kind(status)
    };
    let detail = match code {
        Some(code) if !message.contains(&code) => format!("HTTP {status}: {code}: {message}"),
        _ => format!("HTTP {status}: {message}"),
    };
    ProviderError::new(kind, detail)
}

fn block_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn joined_text(blocks: &[Value]) -> String {
    blocks
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn assistant_message(blocks: &[Value]) -> Value {
    let text = joined_text(blocks);
    let calls: Vec<Value> = blocks
        .iter()
        .filter(|block| block["type"] == "tool_use")
        .map(|block| {
            json!({
                "id": block["id"],
                "type": "function",
                "function": {
                    "name": block["name"],
                    "arguments": block.get("input").cloned().unwrap_or_else(|| json!({})).to_string()
                }
            })
        })
        .collect();
    let mut message = Map::new();
    message.insert("role".into(), Value::from("assistant"));
    let content = if text.is_empty() && !calls.is_empty() {
        Value::Null
    } else {
        Value::from(text)
    };
    message.insert("content".into(), content);
    if !calls.is_empty() {
        message.insert("tool_calls".into(), Value::Array(calls));
    }
    Value::Object(message)
}

fn user_messages(blocks: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = blocks
        .iter()
        .filter(|block| block["type"] == "tool_result")
        .map(|block| {
            let text = block_text(block);
            let content = if block["is_error"] == true {
                format!("Error: {text}")
            } else {
                text
            };
            json!({ "role": "tool", "tool_call_id": block["tool_use_id"], "content": content })
        })
        .collect();
    let text = joined_text(blocks);
    if !text.is_empty() {
        out.push(json!({ "role": "user", "content": text }));
    }
    out
}

pub fn chat_messages(system: &str, messages: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    if !system.is_empty() {
        out.push(json!({ "role": "system", "content": system }));
    }
    for message in messages {
        let role = message["role"].as_str().unwrap_or("user");
        match &message["content"] {
            Value::String(text) => out.push(json!({ "role": role, "content": text })),
            Value::Array(blocks) if role == "assistant" => out.push(assistant_message(blocks)),
            Value::Array(blocks) => out.extend(user_messages(blocks)),
            _ => {}
        }
    }
    out
}

pub fn chat_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            let mut function = Map::new();
            function.insert("name".into(), tool["name"].clone());
            if let Some(description) = tool.get("description").filter(|value| !value.is_null()) {
                function.insert("description".into(), description.clone());
            }
            function.insert(
                "parameters".into(),
                tool.get("input_schema")
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object", "properties": {} })),
            );
            json!({ "type": "function", "function": function })
        })
        .collect()
}

fn message_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect(),
        _ => String::new(),
    }
}

fn arguments(raw: Option<&Value>) -> (String, Result<Value, String>) {
    match raw {
        Some(Value::String(text)) if text.trim().is_empty() => ("{}".into(), Ok(json!({}))),
        Some(Value::String(text)) => match serde_json::from_str::<Value>(text) {
            Ok(value @ Value::Object(_)) => (text.clone(), Ok(value)),
            _ => (text.clone(), Err(text.clone())),
        },
        Some(value @ Value::Object(_)) => (value.to_string(), Ok(value.clone())),
        Some(Value::Null) | None => ("{}".into(), Ok(json!({}))),
        Some(other) => (other.to_string(), Err(other.to_string())),
    }
}

fn stop_reason(finish: Option<&str>, called: bool) -> StopReason {
    match finish {
        Some("tool_calls" | "function_call") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        Some("content_filter") => StopReason::Refusal,
        Some("stop") | None if called => StopReason::ToolUse,
        Some("stop") | None => StopReason::EndTurn,
        Some(other) => StopReason::Other(other.to_string()),
    }
}

pub fn reply_usage(reply: &Value) -> Usage {
    let read = |key: &str| {
        reply
            .pointer(&format!("/usage/{key}"))
            .and_then(Value::as_u64)
            .unwrap_or_default()
    };
    Usage {
        input_tokens: read("prompt_tokens"),
        output_tokens: read("completion_tokens"),
        ..Usage::default()
    }
}

pub fn reply_events(reply: &Value) -> Vec<ModelEvent> {
    let Some(choice) = reply.pointer("/choices/0") else {
        return vec![ModelEvent::Error(ProviderError::new(
            ErrorKind::Protocol,
            "the reply has no choices",
        ))];
    };
    let message = choice.get("message").unwrap_or(&Value::Null);
    let text = message_text(message);
    let mut events = Vec::new();
    let mut content = Vec::new();
    if !text.is_empty() {
        events.push(ModelEvent::TextDelta(text.clone()));
        content.push(json!({ "type": "text", "text": text }));
    }
    let calls = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (index, call) in calls.iter().enumerate() {
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map_or_else(|| format!("call_{index}"), str::to_string);
        let name = call
            .pointer("/function/name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let (raw, input) = arguments(call.pointer("/function/arguments"));
        events.push(ModelEvent::ToolCallStarted {
            id: id.clone(),
            name: name.clone(),
        });
        events.push(ModelEvent::ToolInputDelta {
            id: id.clone(),
            partial_json: raw,
        });
        content.push(json!({
            "type": "tool_use",
            "id": id,
            "name": name,
            "input": input.as_ref().cloned().unwrap_or_else(|_| json!({}))
        }));
        events.push(ModelEvent::ToolCallFinished { id, name, input });
    }
    events.push(ModelEvent::MessageFinished {
        stop_reason: stop_reason(
            choice.get("finish_reason").and_then(Value::as_str),
            !calls.is_empty(),
        ),
        content,
        usage: reply_usage(reply),
    });
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Mock {
        replies: Mutex<VecDeque<(StatusCode, Value)>>,
        bodies: Mutex<Vec<Value>>,
        auth: Mutex<Vec<String>>,
    }

    async fn answer(
        State(mock): State<Arc<Mock>>,
        headers: HeaderMap,
        axum::Json(body): axum::Json<Value>,
    ) -> (StatusCode, axum::Json<Value>) {
        if let Ok(mut bodies) = mock.bodies.lock() {
            bodies.push(body);
        }
        let auth = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if let Ok(mut seen) = mock.auth.lock() {
            seen.push(auth);
        }
        let (status, reply) = mock
            .replies
            .lock()
            .ok()
            .and_then(|mut replies| replies.pop_front())
            .unwrap_or((
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": "no reply left" }),
            ));
        (status, axum::Json(reply))
    }

    async fn serve(replies: Vec<(StatusCode, Value)>) -> (Arc<Mock>, String) {
        let mock = Arc::new(Mock {
            replies: Mutex::new(replies.into()),
            ..Mock::default()
        });
        let app = Router::new()
            .route("/v1/chat/completions", post(answer))
            .with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (mock, format!("http://{address}/v1"))
    }

    fn provider(base: &str) -> OpenAiCompatProvider {
        OpenAiCompatProvider::new(base, "ember")
            .with_key(ApiKey::new("secret-key").unwrap())
            .with_first_wait(Duration::from_millis(5))
    }

    fn request(messages: Vec<Value>, tools: Vec<Value>) -> ModelRequest {
        ModelRequest {
            system: "Be brief.".into(),
            tools,
            messages,
            max_tokens: 512,
        }
    }

    fn text_reply(text: &str) -> Value {
        json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "choices": [{ "index": 0, "finish_reason": "stop", "message": { "role": "assistant", "content": text } }],
            "usage": { "prompt_tokens": 30, "completion_tokens": 4, "total_tokens": 34 }
        })
    }

    fn close_tool() -> Value {
        json!({
            "name": "close_ui",
            "description": "Close a surface.",
            "strict": true,
            "input_schema": { "type": "object", "properties": { "surface": { "type": "string" } }, "required": ["surface"], "additionalProperties": false }
        })
    }

    #[test]
    fn urls_end_in_the_chat_path() {
        assert_eq!(
            chat_url("https://ai.example.uz/v1/"),
            "https://ai.example.uz/v1/chat/completions"
        );
        assert_eq!(
            chat_url("http://localhost:8080/v1/chat/completions"),
            "http://localhost:8080/v1/chat/completions"
        );
    }

    #[test]
    fn history_becomes_chat_messages_with_tool_calls_and_tool_results() {
        let history = vec![
            json!({ "role": "user", "content": [{ "type": "text", "text": "Close it" }] }),
            json!({ "role": "assistant", "content": [
                { "type": "thinking", "thinking": "", "signature": "s" },
                { "type": "text", "text": "Closing." },
                { "type": "tool_use", "id": "call_1", "name": "close_ui", "input": { "surface": "s1" } }
            ] }),
            json!({ "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "call_1", "content": "1 problem", "is_error": true },
                { "type": "text", "text": "[UI event] done" }
            ] }),
            json!({ "role": "assistant", "content": [
                { "type": "tool_use", "id": "call_2", "name": "close_ui", "input": { "surface": "s2" } }
            ] }),
            json!({ "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "call_2", "content": [{ "type": "text", "text": "closed" }] }
            ] }),
        ];
        assert_eq!(
            chat_messages("sys", &history),
            vec![
                json!({ "role": "system", "content": "sys" }),
                json!({ "role": "user", "content": "Close it" }),
                json!({ "role": "assistant", "content": "Closing.", "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "close_ui", "arguments": "{\"surface\":\"s1\"}" } }] }),
                json!({ "role": "tool", "tool_call_id": "call_1", "content": "Error: 1 problem" }),
                json!({ "role": "user", "content": "[UI event] done" }),
                json!({ "role": "assistant", "content": null, "tool_calls": [{ "id": "call_2", "type": "function", "function": { "name": "close_ui", "arguments": "{\"surface\":\"s2\"}" } }] }),
                json!({ "role": "tool", "tool_call_id": "call_2", "content": "closed" }),
            ]
        );
    }

    #[test]
    fn tools_become_functions_without_the_anthropic_fields() {
        assert_eq!(
            chat_tools(&[close_tool()]),
            vec![json!({
                "type": "function",
                "function": {
                    "name": "close_ui",
                    "description": "Close a surface.",
                    "parameters": { "type": "object", "properties": { "surface": { "type": "string" } }, "required": ["surface"], "additionalProperties": false }
                }
            })]
        );
    }

    #[test]
    fn replies_with_broken_arguments_keep_the_raw_text() {
        let reply = json!({
            "choices": [{ "finish_reason": "tool_calls", "message": { "role": "assistant", "content": null, "tool_calls": [
                { "id": "", "type": "function", "function": { "name": "show_ui", "arguments": "{\"root\": \"a" } }
            ] } }]
        });
        let events = reply_events(&reply);
        assert!(events.contains(&ModelEvent::ToolCallFinished {
            id: "call_0".into(),
            name: "show_ui".into(),
            input: Err("{\"root\": \"a".into()),
        }));
        let Some(ModelEvent::MessageFinished {
            stop_reason,
            content,
            ..
        }) = events.last()
        else {
            panic!("no finish: {events:?}");
        };
        assert_eq!(*stop_reason, StopReason::ToolUse);
        assert_eq!(
            content,
            &vec![json!({ "type": "tool_use", "id": "call_0", "name": "show_ui", "input": {} })]
        );
        assert_eq!(stop_reason_of("length"), StopReason::MaxTokens);
        assert_eq!(stop_reason_of("content_filter"), StopReason::Refusal);
    }

    fn stop_reason_of(finish: &str) -> StopReason {
        stop_reason(Some(finish), false)
    }

    #[tokio::test]
    async fn text_replies_carry_usage_and_the_key_stays_in_the_header() {
        let (mock, base) = serve(vec![(StatusCode::OK, text_reply("Hello"))]).await;
        let provider = provider(&base);
        let events: Vec<ModelEvent> = provider
            .stream(request(
                vec![json!({ "role": "user", "content": [{ "type": "text", "text": "Hi" }] })],
                Vec::new(),
            ))
            .collect()
            .await;
        assert_eq!(events[0], ModelEvent::TextDelta("Hello".into()));
        assert_eq!(
            events[1],
            ModelEvent::MessageFinished {
                stop_reason: StopReason::EndTurn,
                content: vec![json!({ "type": "text", "text": "Hello" })],
                usage: Usage {
                    input_tokens: 30,
                    output_tokens: 4,
                    ..Usage::default()
                },
            }
        );
        let bodies = mock.bodies.lock().unwrap().clone();
        assert_eq!(
            bodies[0],
            json!({
                "model": "ember",
                "messages": [{ "role": "system", "content": "Be brief." }, { "role": "user", "content": "Hi" }],
                "max_tokens": 512
            })
        );
        assert!(!bodies[0].to_string().contains("secret-key"));
        assert_eq!(
            mock.auth.lock().unwrap().clone(),
            vec!["Bearer secret-key".to_string()]
        );
        assert_eq!(provider.requests_sent(), 1);
    }

    #[tokio::test]
    async fn tool_calls_round_trip_and_response_format_is_sent() {
        let call = json!({
            "choices": [{ "finish_reason": "tool_calls", "message": { "role": "assistant", "content": "", "tool_calls": [
                { "id": "call_9", "type": "function", "function": { "name": "close_ui", "arguments": "{\"surface\": \"s1\"}" } }
            ] } }],
            "usage": { "prompt_tokens": 50, "completion_tokens": 12 }
        });
        let (mock, base) = serve(vec![
            (StatusCode::OK, call),
            (StatusCode::OK, text_reply("Closed.")),
        ])
        .await;
        let format = json_schema_format("reply", json!({ "type": "object" }));
        let provider = provider(&base).with_response_format(format.clone());
        let first: Vec<ModelEvent> = provider
            .stream(request(
                vec![json!({ "role": "user", "content": "Close it" })],
                vec![close_tool()],
            ))
            .collect()
            .await;
        let Some(ModelEvent::MessageFinished { content, .. }) = first.last() else {
            panic!("no finish: {first:?}");
        };
        assert!(first.contains(&ModelEvent::ToolCallFinished {
            id: "call_9".into(),
            name: "close_ui".into(),
            input: Ok(json!({ "surface": "s1" })),
        }));
        let history = vec![
            json!({ "role": "user", "content": "Close it" }),
            json!({ "role": "assistant", "content": content }),
            json!({ "role": "user", "content": [{ "type": "tool_result", "tool_use_id": "call_9", "content": "closed" }] }),
        ];
        let second: Vec<ModelEvent> = provider
            .stream(request(history, vec![close_tool()]))
            .collect()
            .await;
        assert_eq!(second[0], ModelEvent::TextDelta("Closed.".into()));
        let bodies = mock.bodies.lock().unwrap().clone();
        assert_eq!(bodies[0]["tool_choice"], json!("auto"));
        assert_eq!(bodies[0]["tools"][0]["function"]["name"], json!("close_ui"));
        assert_eq!(bodies[0]["response_format"], format);
        assert_eq!(
            bodies[1]["messages"][2],
            json!({ "role": "assistant", "content": null, "tool_calls": [{ "id": "call_9", "type": "function", "function": { "name": "close_ui", "arguments": "{\"surface\":\"s1\"}" } }] })
        );
        assert_eq!(
            bodies[1]["messages"][3],
            json!({ "role": "tool", "tool_call_id": "call_9", "content": "closed" })
        );
    }

    #[tokio::test]
    async fn busy_replies_are_retried_and_auth_errors_are_not() {
        let (mock, base) = serve(vec![
            (
                StatusCode::TOO_MANY_REQUESTS,
                json!({ "error_code": "concurrency_limited", "message": "too many requests at once" }),
            ),
            (
                StatusCode::OK,
                json!({ "error_code": "concurrency_limited", "message": "too many requests at once" }),
            ),
            (StatusCode::OK, text_reply("ok")),
            (
                StatusCode::UNAUTHORIZED,
                json!({ "error": { "message": "bad key", "type": "auth" } }),
            ),
        ])
        .await;
        let provider = provider(&base);
        let events: Vec<ModelEvent> = provider
            .stream(request(
                vec![json!({ "role": "user", "content": "Hi" })],
                Vec::new(),
            ))
            .collect()
            .await;
        assert_eq!(events[0], ModelEvent::TextDelta("ok".into()));
        assert_eq!(provider.requests_sent(), 3);
        let failed: Vec<ModelEvent> = provider
            .stream(request(
                vec![json!({ "role": "user", "content": "Hi" })],
                Vec::new(),
            ))
            .collect()
            .await;
        assert_eq!(
            failed,
            vec![ModelEvent::Error(ProviderError::new(
                ErrorKind::Auth,
                "HTTP 401: auth: bad key"
            ))]
        );
        assert_eq!(provider.requests_sent(), 4);
        assert_eq!(mock.bodies.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn repeated_server_errors_stop_after_one_retry() {
        let (_, base) = serve(vec![
            (StatusCode::BAD_GATEWAY, json!({ "detail": "upstream" })),
            (StatusCode::BAD_GATEWAY, json!({ "detail": "upstream" })),
            (StatusCode::OK, text_reply("never read")),
        ])
        .await;
        let provider = provider(&base);
        let events: Vec<ModelEvent> = provider
            .stream(request(
                vec![json!({ "role": "user", "content": "Hi" })],
                Vec::new(),
            ))
            .collect()
            .await;
        assert_eq!(
            events,
            vec![ModelEvent::Error(ProviderError::new(
                ErrorKind::Overloaded,
                "HTTP 502: upstream"
            ))]
        );
        assert_eq!(provider.requests_sent(), 2);
    }

    #[tokio::test]
    async fn server_error_retries_can_be_turned_off() {
        let (_, base) = serve(vec![(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error_code": "model_call_failed", "message": "model unavailable" }),
        )])
        .await;
        let provider = provider(&base).with_overloaded_attempts(1);
        let events: Vec<ModelEvent> = provider
            .stream(request(
                vec![json!({ "role": "user", "content": "Hi" })],
                Vec::new(),
            ))
            .collect()
            .await;
        assert_eq!(
            events,
            vec![ModelEvent::Error(ProviderError::new(
                ErrorKind::Overloaded,
                "HTTP 503: model_call_failed: model unavailable"
            ))]
        );
        assert_eq!(provider.requests_sent(), 1);
    }
}
