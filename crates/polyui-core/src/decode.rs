use crate::client::{CHART_TYPE, MAX_SCENE_ITEMS, MAX_SCENE_STRING_CHARS, SCENE_PROP};
use crate::limits::{self, LimitError, Limits};
use crate::protocol::{
    ActionResult, ActionStatus, ClientInfo, Close, Component, Confirm, DataOp, DataUpdate, Hello,
    Incoming, Message, Modality, Open, Patch, ProtocolError, Snapshot, Sync, VERSION, VoiceHint,
    Welcome,
};
use serde_json::{Map, Value};

type Decoded<T> = Result<T, String>;

fn unexpected(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(flag) => format!("boolean `{flag}`"),
        Value::Number(number) if number.is_f64() => format!("floating point `{number}`"),
        Value::Number(number) => format!("integer `{number}`"),
        Value::String(text) => format!("string {text:?}"),
        Value::Array(_) => "sequence".into(),
        Value::Object(_) => "map".into(),
    }
}

fn invalid_type(value: &Value, expected: &str) -> String {
    format!("invalid type: {}, expected {expected}", unexpected(value))
}

fn unknown_variant(found: &str, expected: &str) -> String {
    format!("unknown variant `{found}`, expected {expected}")
}

fn string(value: Value) -> Decoded<String> {
    match value {
        Value::String(text) => Ok(text),
        other => Err(invalid_type(&other, "a string")),
    }
}

fn unsigned(value: Value, expected: &str, max: u64) -> Decoded<u64> {
    match &value {
        Value::Number(number) => match number.as_u64() {
            Some(found) if found <= max => Ok(found),
            Some(_) => Err(format!(
                "invalid value: integer `{number}`, expected {expected}"
            )),
            None if number.is_i64() => Err(format!(
                "invalid value: integer `{number}`, expected {expected}"
            )),
            None => Err(invalid_type(&value, expected)),
        },
        _ => Err(invalid_type(&value, expected)),
    }
}

fn ordinal(value: Value) -> Decoded<u32> {
    unsigned(value, "u32", u64::from(u32::MAX))
        .map(|found| u32::try_from(found).unwrap_or(u32::MAX))
}

fn list<T>(value: Value, item: fn(Value) -> Decoded<T>) -> Decoded<Vec<T>> {
    match value {
        Value::Array(items) => items.into_iter().map(item).collect(),
        other => Err(invalid_type(&other, "a sequence")),
    }
}

fn object(value: Value, expected: &str) -> Decoded<Map<String, Value>> {
    match value {
        Value::Object(map) => Ok(map),
        other => Err(invalid_type(&other, expected)),
    }
}

struct Fields(Map<String, Value>);

impl Fields {
    fn required(&mut self, key: &str) -> Decoded<Value> {
        self.0
            .remove(key)
            .ok_or_else(|| format!("missing field `{key}`"))
    }

    fn present(&mut self, key: &str) -> Option<Value> {
        self.0.remove(key).filter(|value| !value.is_null())
    }

    fn string(&mut self, key: &str) -> Decoded<String> {
        self.required(key).and_then(string)
    }

    fn optional_string(&mut self, key: &str) -> Decoded<Option<String>> {
        self.present(key).map(string).transpose()
    }

    fn seq(&mut self) -> Decoded<u64> {
        self.required("seq")
            .and_then(|value| unsigned(value, "u64", u64::MAX))
    }

    fn optional_u64(&mut self, key: &str) -> Decoded<Option<u64>> {
        self.present(key)
            .map(|value| unsigned(value, "u64", u64::MAX))
            .transpose()
    }

    fn list<T>(&mut self, key: &str, item: fn(Value) -> Decoded<T>) -> Decoded<Vec<T>> {
        self.0
            .remove(key)
            .map_or_else(|| Ok(Vec::new()), |value| list(value, item))
    }

    fn map(&mut self, key: &str) -> Decoded<Map<String, Value>> {
        self.0
            .remove(key)
            .map_or_else(|| Ok(Map::new()), |value| object(value, "a map"))
    }
}

fn fields(value: Value, name: &str) -> Decoded<Fields> {
    object(value, name).map(Fields)
}

fn voice_hint(value: Value) -> Decoded<VoiceHint> {
    let mut fields = fields(value, "struct VoiceHint")?;
    Ok(VoiceHint {
        ordinal: fields.present("ordinal").map(ordinal).transpose()?,
        synonyms: fields.list("synonyms", string)?,
    })
}

fn component(value: Value) -> Decoded<Component> {
    let mut fields = fields(value, "struct Component")?;
    Ok(Component {
        id: fields.string("id")?,
        type_name: fields.string("type")?,
        props: fields.map("props")?,
        children: fields.list("children", string)?,
        alt: fields.optional_string("alt")?,
        say: fields.optional_string("say")?,
        voice: fields.present("voice").map(voice_hint).transpose()?,
    })
}

fn data_op(value: Value) -> Decoded<DataOp> {
    let mut fields = fields(value, "struct RawDataOp")?;
    let op = fields.string("op")?;
    let path = fields.string("path")?;
    let value = fields.0.remove("value");
    let with_value =
        |value: Option<Value>| value.ok_or_else(|| "missing field `value`".to_string());
    match op.as_str() {
        "set" => Ok(DataOp::Set {
            path,
            value: with_value(value)?,
        }),
        "merge" => Ok(DataOp::Merge {
            path,
            value: with_value(value)?,
        }),
        "remove" => Ok(DataOp::Remove { path }),
        other => Err(unknown_variant(other, "one of `set`, `merge`, `remove`")),
    }
}

fn modality(value: Value) -> Decoded<Modality> {
    match value {
        Value::String(name) => match name.as_str() {
            "screen" => Ok(Modality::Screen),
            "voice" => Ok(Modality::Voice),
            other => Err(unknown_variant(other, "`screen` or `voice`")),
        },
        other => Err(invalid_type(&other, "string or map")),
    }
}

fn status(value: Value) -> Decoded<ActionStatus> {
    match value {
        Value::String(name) => match name.as_str() {
            "ok" => Ok(ActionStatus::Ok),
            "rejected" => Ok(ActionStatus::Rejected),
            "expired" => Ok(ActionStatus::Expired),
            "error" => Ok(ActionStatus::Error),
            other => Err(unknown_variant(
                other,
                "one of `ok`, `rejected`, `expired`, `error`",
            )),
        },
        other => Err(invalid_type(&other, "string or map")),
    }
}

fn client_info(value: Value) -> Decoded<ClientInfo> {
    let mut fields = fields(value, "struct ClientInfo")?;
    Ok(ClientInfo {
        name: fields.string("name")?,
        version: fields
            .0
            .remove("version")
            .map_or_else(|| Ok(String::new()), string)?,
    })
}

fn welcome(mut fields: Fields) -> Decoded<Message> {
    Ok(Message::Welcome(Welcome {
        session: fields.string("session")?,
        catalogs: fields.list("catalogs", string)?,
    }))
}

fn open(mut fields: Fields) -> Decoded<Message> {
    Ok(Message::Open(Open {
        surface: fields.string("surface")?,
        seq: fields.seq()?,
        root: fields.string("root")?,
        components: fields.list("components", component)?,
        data: fields.map("data")?,
        fallback: fields.optional_string("fallback")?,
    }))
}

fn snapshot(mut fields: Fields) -> Decoded<Message> {
    Ok(Message::Snapshot(Snapshot {
        surface: fields.string("surface")?,
        seq: fields.seq()?,
        root: fields.string("root")?,
        components: fields.list("components", component)?,
        data: fields.map("data")?,
    }))
}

fn patch(mut fields: Fields) -> Decoded<Message> {
    Ok(Message::Patch(Patch {
        surface: fields.string("surface")?,
        seq: fields.seq()?,
        upsert: fields.list("upsert", component)?,
        remove: fields.list("remove", string)?,
        root: fields.optional_string("root")?,
    }))
}

fn data(mut fields: Fields) -> Decoded<Message> {
    Ok(Message::Data(DataUpdate {
        surface: fields.string("surface")?,
        seq: fields.seq()?,
        ops: fields.list("ops", data_op)?,
    }))
}

fn close(mut fields: Fields) -> Decoded<Message> {
    Ok(Message::Close(Close {
        surface: fields.string("surface")?,
        seq: fields.seq()?,
    }))
}

fn sync(mut fields: Fields) -> Decoded<Message> {
    Ok(Message::Sync(Sync {
        surface: fields.string("surface")?,
        highlight: fields.optional_string("highlight")?,
        at_ms: fields.optional_u64("at_ms")?,
        seq: fields.optional_u64("seq")?,
    }))
}

fn confirm(mut fields: Fields) -> Decoded<Message> {
    Ok(Message::Confirm(Confirm {
        surface: fields.string("surface")?,
        action_id: fields.string("action_id")?,
        nonce: fields.string("nonce")?,
        title: fields.string("title")?,
        body: fields.optional_string("body")?,
        confirm_label: fields.optional_string("confirm_label")?,
        expires_ms: fields.optional_u64("expires_ms")?,
        seq: fields.optional_u64("seq")?,
    }))
}

fn action_result(mut fields: Fields) -> Decoded<Message> {
    Ok(Message::ActionResult(ActionResult {
        surface: fields.string("surface")?,
        action_id: fields.string("action_id")?,
        status: fields.required("status").and_then(status)?,
        message: fields.optional_string("message")?,
        seq: fields.optional_u64("seq")?,
    }))
}

fn server_message(message_type: &str, map: Map<String, Value>) -> Option<Decoded<Message>> {
    let decode: fn(Fields) -> Decoded<Message> = match message_type {
        "welcome" => welcome,
        "ui.open" => open,
        "ui.snapshot" => snapshot,
        "ui.patch" => patch,
        "ui.data" => data,
        "ui.close" => close,
        "ui.sync" => sync,
        "ui.confirm" => confirm,
        "ui.action.result" => action_result,
        _ => return None,
    };
    Some(decode(Fields(map)))
}

fn is_chart(component: &Map<String, Value>) -> bool {
    component.get("type").and_then(Value::as_str) == Some(CHART_TYPE)
}

fn check_components(
    items: &[Value],
    limits: &Limits,
    scene_limits: &Limits,
) -> Result<(), LimitError> {
    if items.len() > limits.max_array_len {
        return Err(LimitError::ArrayTooLong {
            limit: limits.max_array_len,
        });
    }
    for item in items {
        let Value::Object(component) = item else {
            limits::walk(item, 2, limits)?;
            continue;
        };
        for (key, field) in component {
            limits::check_key(key, 3, limits)?;
            match field {
                Value::Object(props) if key == "props" && is_chart(component) => {
                    for (name, prop) in props {
                        limits::check_key(name, 4, limits)?;
                        let applied = if name == SCENE_PROP {
                            scene_limits
                        } else {
                            limits
                        };
                        limits::walk(prop, 4, applied)?;
                    }
                }
                other => limits::walk(other, 3, limits)?,
            }
        }
    }
    Ok(())
}

fn check_message(
    map: &Map<String, Value>,
    limits: &Limits,
    scene_limits: &Limits,
) -> Result<(), LimitError> {
    for (key, item) in map {
        match item {
            Value::Array(items) if key == "components" || key == "upsert" => {
                check_components(items, limits, scene_limits)?;
            }
            other => limits::walk(other, 1, limits)?,
        }
    }
    Ok(())
}

fn scene_limits(limits: &Limits) -> Limits {
    Limits {
        max_array_len: limits.max_array_len.max(MAX_SCENE_ITEMS),
        max_string_chars: limits.max_string_chars.max(MAX_SCENE_STRING_CHARS),
        ..*limits
    }
}

pub(crate) fn check_outgoing(message: &Value, limits: &Limits) -> Result<(), LimitError> {
    let size = limits::json_size(message);
    if size > limits.max_message_bytes {
        return Err(LimitError::MessageTooLarge {
            size,
            limit: limits.max_message_bytes,
        });
    }
    let relaxed = scene_limits(limits);
    match message {
        Value::Object(map) => check_message(map, limits, &relaxed),
        other => limits::walk(other, 0, limits),
    }
}

fn read(input: &str, limits: &Limits) -> Result<Value, ProtocolError> {
    limits::prescan(input, limits)?;
    serde_json::from_str(input).map_err(|error| ProtocolError::Json(error.to_string()))
}

pub(crate) fn from_server(input: &str, limits: &Limits) -> Result<Incoming, ProtocolError> {
    let relaxed = scene_limits(limits);
    let value = read(input, &relaxed)?;
    let Value::Object(map) = value else {
        limits::walk(&value, 0, limits)?;
        return Err(ProtocolError::NotAnObject);
    };
    check_message(&map, limits, &relaxed)?;
    let version = map.get("v").cloned().ok_or(ProtocolError::MissingVersion)?;
    if version.as_u64() != Some(VERSION) {
        return Ok(Incoming::UnsupportedVersion {
            version,
            surface: map
                .get("surface")
                .and_then(Value::as_str)
                .map(str::to_string),
            fallback: map
                .get("fallback")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    let message_type = map
        .get("t")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(ProtocolError::MissingType)?;
    let surface = map
        .get("surface")
        .and_then(Value::as_str)
        .map(str::to_string);
    match server_message(&message_type, map) {
        None => Ok(Incoming::UnknownType {
            message_type,
            surface,
        }),
        Some(Ok(message)) => Ok(Incoming::Message(message)),
        Some(Err(detail)) => Err(ProtocolError::Invalid {
            message_type,
            detail,
        }),
    }
}

fn hello_fields(mut fields: Fields) -> Decoded<Hello> {
    Ok(Hello {
        client: fields.present("client").map(client_info).transpose()?,
        catalogs: fields.list("catalogs", string)?,
        modalities: fields
            .required("modalities")
            .and_then(|value| list(value, modality))?,
        features: fields.list("features", string)?,
        locale: fields.optional_string("locale")?,
    })
}

pub(crate) fn hello(input: &str, limits: &Limits) -> Result<Hello, ProtocolError> {
    let invalid = |detail: String| ProtocolError::Invalid {
        message_type: "hello".into(),
        detail,
    };
    let value = read(input, limits)?;
    if value.get("v").and_then(Value::as_u64) != Some(VERSION) {
        return Err(invalid("expected version 1".into()));
    }
    if value.get("t").and_then(Value::as_str) != Some("hello") {
        return Err(invalid("expected a hello message".into()));
    }
    let hello = fields(value, "struct Hello")
        .and_then(hello_fields)
        .map_err(invalid)?;
    if hello.modalities.is_empty() {
        return Err(invalid("modalities must not be empty".into()));
    }
    Ok(hello)
}
