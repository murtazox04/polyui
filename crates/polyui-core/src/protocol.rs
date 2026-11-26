#[cfg(not(feature = "client-lite"))]
use crate::limits::Limits;
use crate::limits::{self, LimitError};
use crate::{Map, Value};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const VERSION: u64 = 1;

pub const MAX_DETAIL_BYTES: usize = 4096;

pub const MESSAGE_TYPES: [&str; 13] = [
    "hello",
    "welcome",
    "ui.open",
    "ui.patch",
    "ui.data",
    "ui.close",
    "ui.sync",
    "ui.snapshot",
    "ui.confirm",
    "ui.action",
    "ui.action.result",
    "ui.resync",
    "ui.error",
];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Screen,
    Voice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientInfo>,
    #[serde(default)]
    pub catalogs: Vec<String>,
    pub modalities: Vec<Modality>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[cfg(not(feature = "client-lite"))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<ResumeRequest>,
}

#[cfg(not(feature = "client-lite"))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeRequest {
    pub session: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub session: String,
    #[serde(default)]
    pub catalogs: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VoiceHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordinal: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub synonyms: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub props: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub say: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceHint>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum DataOp {
    Set { path: String, value: Value },
    Merge { path: String, value: Value },
    Remove { path: String },
}

#[derive(Deserialize)]
struct RawDataOp {
    op: String,
    path: String,
    #[serde(default)]
    value: Option<Value>,
}

impl<'de> Deserialize<'de> for DataOp {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawDataOp::deserialize(deserializer)?;
        let missing_value = || serde::de::Error::missing_field("value");
        match raw.op.as_str() {
            "set" => Ok(Self::Set {
                path: raw.path,
                value: raw.value.ok_or_else(missing_value)?,
            }),
            "merge" => Ok(Self::Merge {
                path: raw.path,
                value: raw.value.ok_or_else(missing_value)?,
            }),
            "remove" => Ok(Self::Remove { path: raw.path }),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["set", "merge", "remove"],
            )),
        }
    }
}

impl DataOp {
    pub fn path(&self) -> &str {
        match self {
            Self::Set { path, .. } | Self::Merge { path, .. } | Self::Remove { path } => path,
        }
    }

    pub fn apply(&self, data: &mut Value) -> Result<(), crate::pointer::PointerError> {
        match self {
            Self::Set { path, value } => crate::pointer::set(data, path, value.clone()),
            Self::Merge { path, value } => crate::pointer::merge(data, path, value.clone()),
            Self::Remove { path } => crate::pointer::remove(data, path),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Open {
    pub surface: String,
    pub seq: u64,
    pub root: String,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub data: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Patch {
    pub surface: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upsert: Vec<Component>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataUpdate {
    pub surface: String,
    pub seq: u64,
    #[serde(default)]
    pub ops: Vec<DataOp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Close {
    pub surface: String,
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sync {
    pub surface: String,
    #[serde(default)]
    pub highlight: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub surface: String,
    pub seq: u64,
    pub root: String,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub data: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Confirm {
    pub surface: String,
    pub action_id: String,
    pub nonce: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionSource {
    #[default]
    Tap,
    Voice,
    Keyboard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub surface: String,
    pub action_id: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub values: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
    #[serde(default)]
    pub source: ActionSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionStatus {
    Ok,
    Rejected,
    Expired,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResult {
    pub surface: String,
    pub action_id: String,
    pub status: ActionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resync {
    pub surface: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorReason {
    UnsupportedVersion,
    UnknownComponent,
    RenderFailed,
    BadMessage,
    UnknownMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorReport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    pub reason: ErrorReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "t")]
pub enum Message {
    #[serde(rename = "hello")]
    Hello(Hello),
    #[serde(rename = "welcome")]
    Welcome(Welcome),
    #[serde(rename = "ui.open")]
    Open(Open),
    #[serde(rename = "ui.patch")]
    Patch(Patch),
    #[serde(rename = "ui.data")]
    Data(DataUpdate),
    #[serde(rename = "ui.close")]
    Close(Close),
    #[serde(rename = "ui.sync")]
    Sync(Sync),
    #[serde(rename = "ui.snapshot")]
    Snapshot(Snapshot),
    #[serde(rename = "ui.confirm")]
    Confirm(Confirm),
    #[serde(rename = "ui.action")]
    Action(Action),
    #[serde(rename = "ui.action.result")]
    ActionResult(ActionResult),
    #[serde(rename = "ui.resync")]
    Resync(Resync),
    #[serde(rename = "ui.error")]
    Error(ErrorReport),
}

impl Message {
    pub fn message_type(&self) -> &'static str {
        match self {
            Self::Hello(_) => "hello",
            Self::Welcome(_) => "welcome",
            Self::Open(_) => "ui.open",
            Self::Patch(_) => "ui.patch",
            Self::Data(_) => "ui.data",
            Self::Close(_) => "ui.close",
            Self::Sync(_) => "ui.sync",
            Self::Snapshot(_) => "ui.snapshot",
            Self::Confirm(_) => "ui.confirm",
            Self::Action(_) => "ui.action",
            Self::ActionResult(_) => "ui.action.result",
            Self::Resync(_) => "ui.resync",
            Self::Error(_) => "ui.error",
        }
    }

    pub fn surface(&self) -> Option<&str> {
        match self {
            Self::Hello(_) | Self::Welcome(_) => None,
            Self::Open(message) => Some(&message.surface),
            Self::Patch(message) => Some(&message.surface),
            Self::Data(message) => Some(&message.surface),
            Self::Close(message) => Some(&message.surface),
            Self::Sync(message) => Some(&message.surface),
            Self::Snapshot(message) => Some(&message.surface),
            Self::Confirm(message) => Some(&message.surface),
            Self::Action(message) => Some(&message.surface),
            Self::ActionResult(message) => Some(&message.surface),
            Self::Resync(message) => Some(&message.surface),
            Self::Error(message) => message.surface.as_deref(),
        }
    }

    #[cfg(not(feature = "client-lite"))]
    pub fn from_value(value: Value) -> Result<Self, serde_json::Error> {
        let message_type = value
            .get("t")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        decode_any(&message_type, value).unwrap_or_else(|| {
            Err(serde::de::Error::unknown_variant(
                &message_type,
                &MESSAGE_TYPES,
            ))
        })
    }

    #[cfg(not(feature = "client-lite"))]
    pub fn to_value(&self) -> Value {
        let mut out = Map::new();
        out.insert("v".to_string(), Value::from(VERSION));
        if let Ok(Value::Object(fields)) = serde_json::to_value(self) {
            out.extend(fields);
        }
        Value::Object(out)
    }

    #[cfg(not(feature = "client-lite"))]
    pub fn to_json(&self) -> String {
        self.to_value().to_string()
    }

    pub fn resync(surface: &str) -> Self {
        Self::Resync(Resync {
            surface: surface.to_string(),
        })
    }

    pub fn error(surface: Option<&str>, reason: ErrorReason, detail: Option<String>) -> Self {
        Self::Error(ErrorReport {
            surface: surface.map(str::to_string),
            reason,
            detail,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Message(Message),
    UnsupportedVersion {
        version: Value,
        surface: Option<String>,
        fallback: Option<String>,
    },
    UnknownType {
        message_type: String,
        surface: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolError {
    Limit(LimitError),
    Json(String),
    NotAnObject,
    MissingVersion,
    MissingType,
    Invalid {
        message_type: String,
        detail: String,
    },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Limit(error) => write!(f, "{error}"),
            Self::Json(detail) => write!(f, "invalid JSON: {detail}"),
            Self::NotAnObject => write!(f, "a message must be a JSON object"),
            Self::MissingVersion => write!(f, "a message must have an integer \"v\""),
            Self::MissingType => write!(f, "a message must have a string \"t\""),
            Self::Invalid {
                message_type,
                detail,
            } => write!(f, "invalid {message_type}: {detail}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<LimitError> for ProtocolError {
    fn from(error: LimitError) -> Self {
        Self::Limit(error)
    }
}

#[cfg(not(feature = "client-lite"))]
type Decoder = fn(&str, Value) -> Option<Result<Message, serde_json::Error>>;

#[cfg(not(feature = "client-lite"))]
fn decode_any(message_type: &str, value: Value) -> Option<Result<Message, serde_json::Error>> {
    Some(match message_type {
        "hello" => serde_json::from_value(value).map(Message::Hello),
        "welcome" => serde_json::from_value(value).map(Message::Welcome),
        "ui.open" => serde_json::from_value(value).map(Message::Open),
        "ui.patch" => serde_json::from_value(value).map(Message::Patch),
        "ui.data" => serde_json::from_value(value).map(Message::Data),
        "ui.close" => serde_json::from_value(value).map(Message::Close),
        "ui.sync" => serde_json::from_value(value).map(Message::Sync),
        "ui.snapshot" => serde_json::from_value(value).map(Message::Snapshot),
        "ui.confirm" => serde_json::from_value(value).map(Message::Confirm),
        "ui.action" => serde_json::from_value(value).map(Message::Action),
        "ui.action.result" => serde_json::from_value(value).map(Message::ActionResult),
        "ui.resync" => serde_json::from_value(value).map(Message::Resync),
        "ui.error" => serde_json::from_value(value).map(Message::Error),
        _ => return None,
    })
}

#[cfg(not(feature = "client-lite"))]
fn decode_from_server(
    message_type: &str,
    value: Value,
) -> Option<Result<Message, serde_json::Error>> {
    Some(match message_type {
        "welcome" => serde_json::from_value(value).map(Message::Welcome),
        "ui.open" => serde_json::from_value(value).map(Message::Open),
        "ui.patch" => serde_json::from_value(value).map(Message::Patch),
        "ui.data" => serde_json::from_value(value).map(Message::Data),
        "ui.close" => serde_json::from_value(value).map(Message::Close),
        "ui.sync" => serde_json::from_value(value).map(Message::Sync),
        "ui.snapshot" => serde_json::from_value(value).map(Message::Snapshot),
        "ui.confirm" => serde_json::from_value(value).map(Message::Confirm),
        "ui.action.result" => serde_json::from_value(value).map(Message::ActionResult),
        _ => return None,
    })
}

#[cfg(not(feature = "client-lite"))]
pub fn envelope<T: Serialize>(message_type: &str, body: &T) -> Value {
    let mut out = Map::new();
    out.insert("v".to_string(), Value::from(VERSION));
    out.insert("t".to_string(), Value::from(message_type));
    if let Ok(Value::Object(fields)) = serde_json::to_value(body) {
        out.extend(fields);
    }
    Value::Object(out)
}

#[cfg(any(test, feature = "client-lite"))]
mod direct {
    use super::{Action, ActionSource, ErrorReason, VERSION};
    use crate::{Map, Value};

    fn header(message_type: &str) -> Map<String, Value> {
        let mut out = Map::new();
        out.insert("v".to_string(), Value::from(VERSION));
        out.insert("t".to_string(), Value::from(message_type));
        out
    }

    fn reason_name(reason: ErrorReason) -> &'static str {
        match reason {
            ErrorReason::UnsupportedVersion => "unsupported_version",
            ErrorReason::UnknownComponent => "unknown_component",
            ErrorReason::RenderFailed => "render_failed",
            ErrorReason::BadMessage => "bad_message",
            ErrorReason::UnknownMessage => "unknown_message",
        }
    }

    fn source_name(source: ActionSource) -> &'static str {
        match source {
            ActionSource::Tap => "tap",
            ActionSource::Voice => "voice",
            ActionSource::Keyboard => "keyboard",
        }
    }

    pub(super) fn resync(surface: &str) -> Value {
        let mut out = header("ui.resync");
        out.insert("surface".to_string(), Value::from(surface));
        Value::Object(out)
    }

    pub(super) fn error(
        surface: Option<&str>,
        reason: ErrorReason,
        detail: Option<String>,
    ) -> Value {
        let mut out = header("ui.error");
        if let Some(surface) = surface {
            out.insert("surface".to_string(), Value::from(surface));
        }
        out.insert("reason".to_string(), Value::from(reason_name(reason)));
        if let Some(detail) = detail {
            out.insert("detail".to_string(), Value::String(detail));
        }
        Value::Object(out)
    }

    pub(super) fn action(action: &Action) -> Value {
        let mut out = header("ui.action");
        out.insert("surface".to_string(), Value::from(action.surface.as_str()));
        out.insert(
            "action_id".to_string(),
            Value::from(action.action_id.as_str()),
        );
        if !action.values.is_empty() {
            out.insert("values".to_string(), Value::Object(action.values.clone()));
        }
        if let Some(confirm) = &action.confirm {
            out.insert("confirm".to_string(), Value::from(confirm.as_str()));
        }
        out.insert(
            "source".to_string(),
            Value::from(source_name(action.source)),
        );
        if let Some(detail) = &action.detail {
            out.insert("detail".to_string(), Value::Object(detail.clone()));
        }
        Value::Object(out)
    }
}

#[cfg(feature = "client-lite")]
pub fn resync_value(surface: &str) -> Value {
    direct::resync(surface)
}

#[cfg(not(feature = "client-lite"))]
pub fn resync_value(surface: &str) -> Value {
    envelope(
        "ui.resync",
        &Resync {
            surface: surface.to_string(),
        },
    )
}

#[cfg(feature = "client-lite")]
pub fn error_value(surface: Option<&str>, reason: ErrorReason, detail: Option<String>) -> Value {
    direct::error(surface, reason, detail)
}

#[cfg(not(feature = "client-lite"))]
pub fn error_value(surface: Option<&str>, reason: ErrorReason, detail: Option<String>) -> Value {
    envelope(
        "ui.error",
        &ErrorReport {
            surface: surface.map(str::to_string),
            reason,
            detail,
        },
    )
}

#[cfg(feature = "client-lite")]
pub fn action_value(action: &Action) -> Value {
    direct::action(action)
}

#[cfg(not(feature = "client-lite"))]
pub fn action_value(action: &Action) -> Value {
    envelope("ui.action", action)
}

#[cfg(not(feature = "client-lite"))]
pub fn check_for_client(message: &Value, limits: &Limits) -> Result<(), LimitError> {
    crate::decode::check_outgoing(message, limits)
}

#[cfg(not(feature = "client-lite"))]
pub fn parse(input: &str) -> Result<Incoming, ProtocolError> {
    parse_with(input, &Limits::default())
}

#[cfg(not(feature = "client-lite"))]
pub fn parse_with(input: &str, limits: &Limits) -> Result<Incoming, ProtocolError> {
    checked(classify(read(input, limits)?, decode_any)?, limits)
}

#[cfg(not(feature = "client-lite"))]
pub fn parse_from_server(input: &str, limits: &Limits) -> Result<Incoming, ProtocolError> {
    classify(read(input, limits)?, decode_from_server)
}

#[cfg(not(feature = "client-lite"))]
pub fn parse_value(value: Value, limits: &Limits) -> Result<Incoming, ProtocolError> {
    limits::check_value(&value, limits)?;
    checked(classify(value, decode_any)?, limits)
}

pub fn detail_size(detail: &Map<String, Value>) -> usize {
    limits::json_size(&Value::Object(detail.clone()))
}

#[cfg(not(feature = "client-lite"))]
pub fn check_action(action: &Action, limits: &Limits) -> Result<(), ProtocolError> {
    let invalid = |detail: String| ProtocolError::Invalid {
        message_type: "ui.action".into(),
        detail,
    };
    if let Some(detail) = &action.detail {
        let size = detail_size(detail);
        if size > MAX_DETAIL_BYTES {
            return Err(invalid(format!(
                "detail is {size} bytes, max is {MAX_DETAIL_BYTES}"
            )));
        }
    }
    let size = limits::json_size(&Value::Object(action.values.clone()));
    if size > limits.max_data_bytes {
        return Err(invalid(format!(
            "values are {size} bytes, max is {}",
            limits.max_data_bytes
        )));
    }
    Ok(())
}

#[cfg(not(feature = "client-lite"))]
fn checked(incoming: Incoming, limits: &Limits) -> Result<Incoming, ProtocolError> {
    if let Incoming::Message(Message::Action(action)) = &incoming {
        check_action(action, limits)?;
    }
    Ok(incoming)
}

#[cfg(not(feature = "client-lite"))]
pub fn parse_hello(input: &str, limits: &Limits) -> Result<Hello, ProtocolError> {
    let value = read(input, limits)?;
    let invalid = |detail: String| ProtocolError::Invalid {
        message_type: "hello".into(),
        detail,
    };
    if value.get("v").and_then(Value::as_u64) != Some(VERSION) {
        return Err(invalid("expected version 1".into()));
    }
    if value.get("t").and_then(Value::as_str) != Some("hello") {
        return Err(invalid("expected a hello message".into()));
    }
    let hello: Hello = serde_json::from_value(value).map_err(|error| invalid(error.to_string()))?;
    if hello.modalities.is_empty() {
        return Err(invalid("modalities must not be empty".into()));
    }
    Ok(hello)
}

#[cfg(not(feature = "client-lite"))]
fn read(input: &str, limits: &Limits) -> Result<Value, ProtocolError> {
    limits::prescan(input, limits)?;
    serde_json::from_str(input).map_err(|error| ProtocolError::Json(error.to_string()))
}

#[cfg(not(feature = "client-lite"))]
fn string_field(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(not(feature = "client-lite"))]
fn classify(value: Value, decode: Decoder) -> Result<Incoming, ProtocolError> {
    let Value::Object(map) = value else {
        return Err(ProtocolError::NotAnObject);
    };
    let version = map.get("v").cloned().ok_or(ProtocolError::MissingVersion)?;
    if version.as_u64() != Some(VERSION) {
        return Ok(Incoming::UnsupportedVersion {
            version,
            surface: string_field(&map, "surface"),
            fallback: string_field(&map, "fallback"),
        });
    }
    let message_type = string_field(&map, "t").ok_or(ProtocolError::MissingType)?;
    let surface = string_field(&map, "surface");
    match decode(&message_type, Value::Object(map)) {
        None => Ok(Incoming::UnknownType {
            message_type,
            surface,
        }),
        Some(Ok(message)) => Ok(Incoming::Message(message)),
        Some(Err(error)) => Err(ProtocolError::Invalid {
            message_type,
            detail: error.to_string(),
        }),
    }
}

#[cfg(all(test, not(feature = "client-lite")))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn direct_envelopes_match_the_serde_ones() {
        assert_eq!(
            direct::resync("s1").to_string(),
            envelope(
                "ui.resync",
                &Resync {
                    surface: "s1".into()
                }
            )
            .to_string()
        );
        let reasons = [
            ErrorReason::UnsupportedVersion,
            ErrorReason::UnknownComponent,
            ErrorReason::RenderFailed,
            ErrorReason::BadMessage,
            ErrorReason::UnknownMessage,
        ];
        for reason in reasons {
            for surface in [None, Some("s1")] {
                for detail in [None, Some("bad \"thing\"".to_string())] {
                    let report = ErrorReport {
                        surface: surface.map(str::to_string),
                        reason,
                        detail: detail.clone(),
                    };
                    assert_eq!(
                        direct::error(surface, reason, detail).to_string(),
                        envelope("ui.error", &report).to_string()
                    );
                }
            }
        }
        let values = [
            Map::new(),
            json!({ "/b": 1, "/a": [true] })
                .as_object()
                .cloned()
                .unwrap_or_default(),
        ];
        for values in values {
            for confirm in [None, Some("n_1".to_string())] {
                for source in [
                    ActionSource::Tap,
                    ActionSource::Voice,
                    ActionSource::Keyboard,
                ] {
                    for detail in [None, json!({ "z": 1, "a": null }).as_object().cloned()] {
                        let action = Action {
                            surface: "s1".into(),
                            action_id: "act".into(),
                            values: values.clone(),
                            confirm: confirm.clone(),
                            source,
                            detail,
                        };
                        assert_eq!(
                            direct::action(&action).to_string(),
                            envelope("ui.action", &action).to_string()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn round_trips_every_example() {
        let examples = [
            json!({ "v": 1, "t": "hello", "client": { "name": "polyui-web", "version": "0.1.0" }, "catalogs": ["core@1", "chart@1"], "modalities": ["screen", "voice"], "features": ["sync", "stream"], "locale": "uz-Latn" }),
            json!({ "v": 1, "t": "welcome", "session": "sess_8f2c", "catalogs": ["core@1", "chart@1"] }),
            json!({ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "card", "components": [], "data": {}, "fallback": "Your bills" }),
            json!({ "v": 1, "t": "ui.patch", "surface": "s1", "seq": 2, "remove": ["old_note"], "root": "card" }),
            json!({ "v": 1, "t": "ui.data", "surface": "s1", "seq": 3, "ops": [ { "op": "set", "path": "/bills/0/paid", "value": true } ] }),
            json!({ "v": 1, "t": "ui.close", "surface": "s1", "seq": 4 }),
            json!({ "v": 1, "t": "ui.sync", "surface": "s1", "highlight": "opt_2", "at_ms": 1840 }),
            json!({ "v": 1, "t": "ui.sync", "surface": "s1", "highlight": null }),
            json!({ "v": 1, "t": "ui.snapshot", "surface": "s1", "seq": 9, "root": "card", "components": [], "data": {} }),
            json!({ "v": 1, "t": "ui.confirm", "surface": "s1", "action_id": "act_4kq9", "nonce": "n_77ad", "title": "Pay Hududgaz", "body": "29 322,19 so'm from card 4471", "confirm_label": "Pay", "expires_ms": 60000 }),
            json!({ "v": 1, "t": "ui.action", "surface": "s1", "action_id": "act_4kq9", "values": { "/form/phone": "+998901234567" }, "source": "tap" }),
            json!({ "v": 1, "t": "ui.action", "surface": "s1", "action_id": "act_4kq9", "confirm": "n_77ad", "source": "tap" }),
            json!({ "v": 1, "t": "ui.action.result", "surface": "s1", "action_id": "act_4kq9", "status": "ok" }),
            json!({ "v": 1, "t": "ui.resync", "surface": "s1" }),
            json!({ "v": 1, "t": "ui.error", "surface": "s1", "reason": "unknown_component", "detail": "acme:Card" }),
        ];
        for example in examples {
            let parsed = parse(&example.to_string()).unwrap();
            let Incoming::Message(message) = parsed else {
                panic!("not a message: {example}");
            };
            assert_eq!(message.to_value(), example);
        }
    }

    #[test]
    fn unknown_version_and_type_are_reported_not_errors() {
        let future = parse(r#"{"v":2,"t":"ui.open","surface":"s1","fallback":"hi"}"#).unwrap();
        assert_eq!(
            future,
            Incoming::UnsupportedVersion {
                version: json!(2),
                surface: Some("s1".into()),
                fallback: Some("hi".into())
            }
        );
        let unknown = parse(r#"{"v":1,"t":"ui.dance","surface":"s1"}"#).unwrap();
        assert!(matches!(unknown, Incoming::UnknownType { .. }));
    }

    #[test]
    fn rejects_bad_envelopes() {
        assert!(matches!(parse("[]"), Err(ProtocolError::NotAnObject)));
        assert!(matches!(parse("{}"), Err(ProtocolError::MissingVersion)));
        assert!(matches!(
            parse(r#"{"v":1}"#),
            Err(ProtocolError::MissingType)
        ));
        assert!(matches!(parse("{"), Err(ProtocolError::Json(_))));
        assert!(matches!(
            parse(r#"{"v":1,"t":"ui.open","surface":"s1"}"#),
            Err(ProtocolError::Invalid { .. })
        ));
        let deep = format!("{}{}", "[".repeat(33), "]".repeat(33));
        assert!(matches!(parse(&deep), Err(ProtocolError::Limit(_))));
    }

    #[test]
    fn action_detail_and_values_are_capped() {
        let limits = Limits::default();
        let action = |detail: Value, values: Value| {
            json!({ "v": 1, "t": "ui.action", "surface": "s1", "action_id": "act_1", "detail": detail, "values": values })
                .to_string()
        };
        let small = action(json!({ "row": 3 }), json!({}));
        assert!(parse_with(&small, &limits).is_ok());
        let words: Vec<String> = (0..600).map(|index| format!("word {index}")).collect();
        let big = action(json!({ "words": words }), json!({}));
        let error = parse_with(&big, &limits).unwrap_err().to_string();
        assert!(
            error.starts_with("invalid ui.action: detail is "),
            "{error}"
        );
        let wide = (0..60)
            .map(|index| (format!("/f{index}"), json!("x".repeat(4000))))
            .collect::<Map<String, Value>>();
        let flood = action(json!({}), Value::Object(wide.clone()));
        let error = parse_with(&flood, &limits).unwrap_err().to_string();
        assert!(error.contains("values are "), "{error}");
        let mut message: Value = serde_json::from_str(&flood).unwrap();
        message["values"] = Value::Object(wide.into_iter().take(20).collect());
        assert!(parse_value(message, &limits).is_ok());
        let edge = format!("{{\"pad\":\"{}\"}}", "ж".repeat(2043));
        assert_eq!(edge.len(), MAX_DETAIL_BYTES);
        assert!(
            parse_with(
                &action(serde_json::from_str(&edge).unwrap(), json!({})),
                &limits
            )
            .is_ok()
        );
        let over = format!("{{\"pad\":\"{}\"}}", "ж".repeat(2044));
        assert!(
            parse_with(
                &action(serde_json::from_str(&over).unwrap(), json!({})),
                &limits
            )
            .is_err()
        );
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let parsed = parse(r#"{"v":1,"t":"ui.close","surface":"s1","seq":4,"extra":{"x":1}}"#);
        assert!(matches!(parsed, Ok(Incoming::Message(Message::Close(_)))));
    }
}
