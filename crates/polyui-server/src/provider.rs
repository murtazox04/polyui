use futures_util::stream::BoxStream;
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequest {
    pub system: String,
    pub tools: Vec<Value>,
    pub messages: Vec<Value>,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    StopSequence,
    PauseTurn,
    Other(String),
}

impl StopReason {
    pub fn parse(text: &str) -> Self {
        match text {
            "end_turn" => Self::EndTurn,
            "tool_use" => Self::ToolUse,
            "max_tokens" => Self::MaxTokens,
            "refusal" => Self::Refusal,
            "stop_sequence" => Self::StopSequence,
            "pause_turn" => Self::PauseTurn,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::EndTurn => "end_turn",
            Self::ToolUse => "tool_use",
            Self::MaxTokens => "max_tokens",
            Self::Refusal => "refusal",
            Self::StopSequence => "stop_sequence",
            Self::PauseTurn => "pause_turn",
            Self::Other(text) => text,
        }
    }

    pub fn is_cut_off(&self) -> bool {
        matches!(self, Self::MaxTokens | Self::Refusal)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

impl Usage {
    pub fn merge_from(&mut self, value: &Value) {
        let read = |key: &str| value.get(key).and_then(Value::as_u64);
        if let Some(tokens) = read("input_tokens") {
            self.input_tokens = tokens;
        }
        if let Some(tokens) = read("output_tokens") {
            self.output_tokens = tokens;
        }
        if let Some(tokens) = read("cache_creation_input_tokens") {
            self.cache_creation_input_tokens = tokens;
        }
        if let Some(tokens) = read("cache_read_input_tokens") {
            self.cache_read_input_tokens = tokens;
        }
    }

    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_input_tokens += other.cache_creation_input_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
    }
}

impl fmt::Display for Usage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "input {} (cache write {}, cache read {}), output {}",
            self.input_tokens,
            self.cache_creation_input_tokens,
            self.cache_read_input_tokens,
            self.output_tokens
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Auth,
    RateLimited,
    Overloaded,
    BadRequest,
    Network,
    Protocol,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub detail: String,
}

impl ProviderError {
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub fn user_message(&self) -> &'static str {
        match self.kind {
            ErrorKind::RateLimited | ErrorKind::Overloaded => {
                "The assistant is busy right now. Please try again in a moment."
            }
            ErrorKind::Network => "The assistant could not be reached. Please try again.",
            _ => "The assistant could not answer. Please try again.",
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for ProviderError {}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelEvent {
    TextDelta(String),
    ToolCallStarted {
        id: String,
        name: String,
    },
    ToolInputDelta {
        id: String,
        partial_json: String,
    },
    ToolCallFinished {
        id: String,
        name: String,
        input: Result<Value, String>,
    },
    MessageFinished {
        stop_reason: StopReason,
        content: Vec<Value>,
        usage: Usage,
    },
    Error(ProviderError),
}

pub trait ModelProvider: Send + Sync {
    fn stream(&self, request: ModelRequest) -> BoxStream<'static, ModelEvent>;
}
