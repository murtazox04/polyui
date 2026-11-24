pub use crate::fits::MAX_WITH_PATHS;

use crate::catalog::{self, CatalogError, CatalogSet, PropKind, PropSpec, TypeDef};
use crate::fits::is_blank_label;
use crate::limits::{self, Limits};
use crate::pointer::{self, PointerError};
use crate::protocol::{Component, DataOp, VoiceHint};
use crate::sanitize;
use crate::text::Locale;
use crate::view;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;

pub const MAX_ID_CHARS: usize = 64;
pub const MAX_SYNONYMS: usize = 12;
const MAX_LISTED_PROBLEMS: usize = 20;
const COMPONENT_FIELDS: [&str; 7] = ["id", "type", "props", "children", "alt", "say", "voice"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Input,
    Field(String),
    Item {
        list: String,
        index: usize,
        id: Option<String>,
        prop: Option<String>,
    },
    Entry {
        list: String,
        index: usize,
        path: Option<String>,
    },
    Existing {
        id: String,
        prop: Option<String>,
    },
}

impl Location {
    fn item(list: &str, index: usize) -> Self {
        Self::Item {
            list: list.to_string(),
            index,
            id: None,
            prop: None,
        }
    }

    fn with_prop(&self, prop: &str) -> Self {
        let mut out = self.clone();
        match &mut out {
            Self::Item { prop: slot, .. } | Self::Existing { prop: slot, .. } => {
                *slot = Some(prop.to_string());
            }
            _ => {}
        }
        out
    }
}

fn clip(text: &str) -> String {
    if text.chars().count() > MAX_ID_CHARS {
        let mut out: String = text.chars().take(MAX_ID_CHARS).collect();
        out.push_str("...");
        out
    } else {
        text.to_string()
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input => write!(f, "input"),
            Self::Field(name) => write!(f, "{name}"),
            Self::Item {
                list,
                index,
                id,
                prop,
            } => {
                write!(f, "{list}[{index}]")?;
                if let Some(id) = id {
                    write!(f, " id \"{}\"", clip(id))?;
                }
                if let Some(prop) = prop {
                    write!(f, " prop \"{prop}\"")?;
                }
                Ok(())
            }
            Self::Entry { list, index, path } => {
                write!(f, "{list}[{index}]")?;
                if let Some(path) = path {
                    write!(f, " path \"{}\"", clip(path))?;
                }
                Ok(())
            }
            Self::Existing { id, prop } => {
                write!(f, "component \"{}\"", clip(id))?;
                if let Some(prop) = prop {
                    write!(f, " prop \"{prop}\"")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub location: Location,
    pub message: String,
}

impl Problem {
    pub fn new(location: Location, message: impl Into<String>) -> Self {
        Self {
            location,
            message: message.into(),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Show,
    Update,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problems {
    pub operation: Operation,
    pub items: Vec<Problem>,
}

impl Problems {
    pub fn lines(&self) -> Vec<String> {
        self.items.iter().map(Problem::to_string).collect()
    }
}

impl fmt::Display for Problems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.items.len();
        let noun = if count == 1 { "problem" } else { "problems" };
        let outcome = match self.operation {
            Operation::Show => "nothing was shown",
            Operation::Update => "nothing was changed",
            Operation::Close => "nothing was closed",
        };
        write!(f, "{count} {noun}, {outcome}:")?;
        for (number, problem) in self.items.iter().take(MAX_LISTED_PROBLEMS).enumerate() {
            write!(f, "\n{}. {problem}", number + 1)?;
        }
        if count > MAX_LISTED_PROBLEMS {
            write!(f, "\n... and {} more.", count - MAX_LISTED_PROBLEMS)?;
        }
        Ok(())
    }
}

impl std::error::Error for Problems {}

#[derive(Debug, Clone, PartialEq)]
pub struct ActionSpec {
    pub args: Vec<(String, PropSpec)>,
    pub requires_confirm: bool,
    pub description: Option<String>,
}

impl ActionSpec {
    pub fn new(requires_confirm: bool) -> Self {
        Self {
            args: Vec::new(),
            requires_confirm,
            description: None,
        }
    }

    pub fn from_json(text: &str) -> Result<Self, CatalogError> {
        let value: Value = serde_json::from_str(text).map_err(|error| CatalogError {
            path: String::new(),
            message: format!("invalid JSON: {error}"),
        })?;
        Self::from_value(&value)
    }

    pub fn from_value(value: &Value) -> Result<Self, CatalogError> {
        let fail = |path: &str, message: &str| CatalogError {
            path: path.to_string(),
            message: message.to_string(),
        };
        let map = value
            .as_object()
            .ok_or_else(|| fail("", "an action spec must be an object"))?;
        if let Some(key) = map
            .keys()
            .find(|key| !["args", "requires_confirm", "description"].contains(&key.as_str()))
        {
            return Err(fail("", &format!("unknown field \"{key}\"")));
        }
        let requires_confirm = match map.get("requires_confirm") {
            None => false,
            Some(Value::Bool(flag)) => *flag,
            Some(_) => return Err(fail("requires_confirm", "must be true or false")),
        };
        let description = match map.get("description") {
            None => None,
            Some(Value::String(text)) => Some(text.clone()),
            Some(_) => return Err(fail("description", "must be a string")),
        };
        let mut args = Vec::new();
        if let Some(value) = map.get("args") {
            let entries = value
                .as_object()
                .ok_or_else(|| fail("args", "must be an object"))?;
            for (name, spec) in entries {
                let spec = PropSpec::from_value(spec).map_err(|error| CatalogError {
                    path: format!(
                        "args.{name}{}",
                        if error.path.is_empty() { "" } else { "." }
                    ) + &error.path,
                    message: error.message,
                })?;
                args.push((name.clone(), spec));
            }
        }
        Ok(Self {
            args,
            requires_confirm,
            description,
        })
    }

    pub fn arg(&self, name: &str) -> Option<&PropSpec> {
        catalog::lookup(&self.args, name)
    }
}

pub trait ActionRegistry {
    fn action(&self, name: &str) -> Option<&ActionSpec>;
    fn names(&self) -> Vec<&str>;
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActionTable {
    entries: Vec<(String, ActionSpec)>,
}

impl ActionTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: &str, spec: ActionSpec) -> &mut Self {
        match self
            .entries
            .iter_mut()
            .find(|(existing, _)| existing == name)
        {
            Some((_, slot)) => *slot = spec,
            None => self.entries.push((name.to_string(), spec)),
        }
        self
    }

    pub fn with(mut self, name: &str, spec: ActionSpec) -> Self {
        self.register(name, spec);
        self
    }
}

impl ActionRegistry for ActionTable {
    fn action(&self, name: &str) -> Option<&ActionSpec> {
        self.entries
            .iter()
            .find(|(existing, _)| existing == name)
            .map(|(_, spec)| spec)
    }

    fn names(&self) -> Vec<&str> {
        self.entries.iter().map(|(name, _)| name.as_str()).collect()
    }
}

pub const REF_FAILURE: &str = "$ref could not be resolved.";

pub trait RefResolver {
    fn source_kind(&self, source: &str) -> Option<PropKind>;
    fn resolve(&self, source: &str, key: &str) -> Result<Value, String>;

    fn has_source(&self, source: &str) -> bool {
        self.source_kind(source).is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct RefSource {
    name: String,
    kind: Option<PropKind>,
    entries: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaticRefs {
    sources: Vec<RefSource>,
}

impl StaticRefs {
    pub fn new() -> Self {
        Self::default()
    }

    fn source_mut(&mut self, source: &str) -> &mut RefSource {
        let position = match self.sources.iter().position(|entry| entry.name == source) {
            Some(position) => position,
            None => {
                self.sources.push(RefSource {
                    name: source.to_string(),
                    kind: None,
                    entries: Map::new(),
                });
                self.sources.len() - 1
            }
        };
        &mut self.sources[position]
    }

    pub fn declare(mut self, source: &str, kind: PropKind) -> Self {
        self.source_mut(source).kind = Some(kind);
        self
    }

    pub fn with(mut self, source: &str, key: &str, value: Value) -> Self {
        self.source_mut(source)
            .entries
            .insert(key.to_string(), value);
        self
    }
}

impl RefResolver for StaticRefs {
    fn source_kind(&self, source: &str) -> Option<PropKind> {
        self.sources
            .iter()
            .find(|entry| entry.name == source)
            .and_then(|entry| entry.kind.clone())
    }

    fn resolve(&self, source: &str, key: &str) -> Result<Value, String> {
        self.sources
            .iter()
            .find(|entry| entry.name == source)
            .and_then(|entry| entry.entries.get(key))
            .cloned()
            .ok_or_else(|| format!("no value for \"{key}\""))
    }
}

pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_CHARS
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn split_ref(reference: &str) -> Option<(&str, &str)> {
    let (source, key) = reference.split_once(':')?;
    let mut source_chars = source.chars();
    let source_ok = source.len() <= 64
        && source_chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && source_chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    let key_ok = !key.is_empty()
        && key.chars().count() <= 256
        && !key.chars().any(|c| c.is_whitespace() || c.is_control());
    (source_ok && key_ok).then_some((source, key))
}

fn describe(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "true or false",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

fn expected(kind: &PropKind) -> String {
    match kind {
        PropKind::String { .. } => "text".into(),
        PropKind::Int { .. } => "a whole number".into(),
        PropKind::Number { .. } => "a number".into(),
        PropKind::Bool => "true or false".into(),
        PropKind::Scalar => "text, a number or true/false".into(),
        PropKind::Enum { values } => format!("one of {}", values.join(", ")),
        PropKind::Url { schemes } => format!("an absolute {} URL", schemes.join(" or ")),
        PropKind::Action => "an action object like {\"action\": \"name\", \"args\": {}}".into(),
        PropKind::Path => "a JSON Pointer like \"/form/name\"".into(),
        PropKind::Array { .. } => "a list".into(),
        PropKind::Object { .. } => "an object".into(),
    }
}

fn bounds_text<T: fmt::Display>(min: Option<T>, max: Option<T>) -> String {
    match (min, max) {
        (Some(min), Some(max)) => format!(" from {min} to {max}"),
        (Some(min), None) => format!(" of at least {min}"),
        (None, Some(max)) => format!(" up to {max}"),
        (None, None) => String::new(),
    }
}

fn kind_text(kind: &PropKind, max_string_chars: usize) -> String {
    match kind {
        PropKind::String { min, max } => format!(
            "text of {} to {} characters",
            min.unwrap_or(0),
            max.unwrap_or(max_string_chars as u64)
        ),
        PropKind::Int { min, max } => format!("a whole number{}", bounds_text(*min, *max)),
        PropKind::Number { min, max } => format!("a number{}", bounds_text(*min, *max)),
        other => expected(other),
    }
}

fn contains_range<T: PartialOrd + Copy>(
    inner: (Option<T>, Option<T>),
    outer: (Option<T>, Option<T>),
) -> bool {
    let low_ok = match (inner.0, outer.0) {
        (_, None) => true,
        (Some(inner), Some(outer)) => inner >= outer,
        (None, Some(_)) => false,
    };
    let high_ok = match (inner.1, outer.1) {
        (_, None) => true,
        (Some(inner), Some(outer)) => inner <= outer,
        (None, Some(_)) => false,
    };
    low_ok && high_ok
}

fn widen(range: (Option<i64>, Option<i64>)) -> (Option<f64>, Option<f64>) {
    (
        range.0.map(|low| low as f64),
        range.1.map(|high| high as f64),
    )
}

fn ref_kind_fits(
    declared: &PropKind,
    target: &PropKind,
    allow_http: bool,
    max_string_chars: usize,
) -> bool {
    let text_range = |min: &Option<u64>, max: &Option<u64>| {
        (
            Some(min.unwrap_or(0)),
            Some(max.unwrap_or(max_string_chars as u64)),
        )
    };
    let fits_text = |length: usize, min: &Option<u64>, max: &Option<u64>| {
        let (low, high) = text_range(min, max);
        low.is_none_or(|low| length as u64 >= low) && high.is_none_or(|high| length as u64 <= high)
    };
    match (declared, target) {
        (
            PropKind::String { min, max },
            PropKind::String {
                min: low,
                max: high,
            },
        ) => contains_range(text_range(min, max), text_range(low, high)),
        (PropKind::String { max, .. }, PropKind::Scalar) => {
            max.unwrap_or(max_string_chars as u64) <= max_string_chars as u64
        }
        (
            PropKind::Int { min, max },
            PropKind::Int {
                min: low,
                max: high,
            },
        ) => contains_range((*min, *max), (*low, *high)),
        (
            PropKind::Int { min, max },
            PropKind::Number {
                min: low,
                max: high,
            },
        ) => contains_range(widen((*min, *max)), (*low, *high)),
        (
            PropKind::Number { min, max },
            PropKind::Number {
                min: low,
                max: high,
            },
        ) => contains_range((*min, *max), (*low, *high)),
        (PropKind::Int { .. } | PropKind::Number { .. } | PropKind::Bool, PropKind::Scalar)
        | (PropKind::Bool, PropKind::Bool)
        | (PropKind::Scalar, PropKind::Scalar) => true,
        (PropKind::Enum { values }, PropKind::Enum { values: allowed }) => {
            values.iter().all(|value| allowed.contains(value))
        }
        (PropKind::Enum { values }, PropKind::String { min, max }) => values
            .iter()
            .all(|value| fits_text(value.chars().count(), min, max)),
        (PropKind::Enum { values }, PropKind::Scalar) => values
            .iter()
            .all(|value| value.chars().count() <= max_string_chars),
        (PropKind::Url { schemes }, PropKind::Url { schemes: allowed }) => schemes
            .iter()
            .all(|scheme| allowed.contains(scheme) && (allow_http || scheme != "http")),
        (PropKind::Array { .. }, PropKind::Array { .. })
        | (PropKind::Object { .. }, PropKind::Object { .. }) => declared == target,
        _ => false,
    }
}

fn join_path(base: &str, key: &str) -> String {
    if base.is_empty() {
        key.to_string()
    } else {
        format!("{base}.{key}")
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Checker<'a> {
    pub bindings: bool,
    pub handles: bool,
    pub allow_http: bool,
    pub actions: Option<&'a dyn ActionRegistry>,
    pub refs: Option<&'a dyn RefResolver>,
    pub data: Option<&'a Value>,
    pub max_string_chars: usize,
    pub max_array_len: usize,
}

impl<'a> Checker<'a> {
    fn literal(allow_http: bool) -> Self {
        let limits = Limits::default();
        Self {
            bindings: false,
            handles: true,
            allow_http,
            actions: None,
            refs: None,
            data: None,
            max_string_chars: limits.max_string_chars,
            max_array_len: limits.max_array_len,
        }
    }

    pub(crate) fn check(
        &self,
        spec: &PropSpec,
        value: &Value,
        path: &str,
        out: &mut Vec<(String, String)>,
    ) {
        if let Value::Object(map) = value
            && (map.contains_key("$bind") || map.contains_key("$ref"))
        {
            self.check_binding(spec, map, path, out);
            return;
        }
        let mut fail = |message: String| out.push((path.to_string(), message));
        let mismatch = || {
            format!(
                "expected {}, got {}.",
                expected(&spec.kind),
                describe(value)
            )
        };
        match &spec.kind {
            PropKind::String { min, max } => match value.as_str() {
                Some(text) => {
                    let length = text.chars().count();
                    let max = max.map_or(self.max_string_chars, |max| max as usize);
                    if length > max {
                        fail(format!("text is {length} characters, max is {max}."));
                    } else if let Some(min) = min
                        && (length as u64) < *min
                    {
                        fail(format!("text is {length} characters, min is {min}."));
                    }
                }
                None => fail(mismatch()),
            },
            PropKind::Int { min, max } => {
                let whole = value.as_i64().or_else(|| value.as_u64().map(|_| i64::MAX));
                match whole {
                    Some(number) => {
                        if let Some(min) = min
                            && number < *min
                        {
                            fail(format!("{number} is below the min {min}."));
                        } else if let Some(max) = max
                            && number > *max
                        {
                            fail(format!("{value} is above the max {max}."));
                        }
                    }
                    None => fail(mismatch()),
                }
            }
            PropKind::Number { min, max } => match value.as_f64() {
                Some(number) if number.is_finite() => {
                    if let Some(min) = min
                        && number < *min
                    {
                        fail(format!("{value} is below the min {}.", Value::from(*min)));
                    } else if let Some(max) = max
                        && number > *max
                    {
                        fail(format!("{value} is above the max {}.", Value::from(*max)));
                    }
                }
                _ => fail(mismatch()),
            },
            PropKind::Bool => {
                if !value.is_boolean() {
                    fail(mismatch());
                }
            }
            PropKind::Scalar => match value {
                Value::String(text) => {
                    let length = text.chars().count();
                    if length > self.max_string_chars {
                        fail(format!(
                            "text is {length} characters, max is {}.",
                            self.max_string_chars
                        ));
                    }
                }
                Value::Bool(_) | Value::Number(_) => {}
                _ => fail(mismatch()),
            },
            PropKind::Enum { values } => match value.as_str() {
                Some(text) if values.iter().any(|allowed| allowed == text) => {}
                Some(text) => fail(format!(
                    "\"{}\" is not one of {}.",
                    clip(text),
                    values.join(", ")
                )),
                None => fail(mismatch()),
            },
            PropKind::Url { schemes } => match value.as_str() {
                Some(url) => {
                    let allowed: Vec<&str> = schemes
                        .iter()
                        .map(String::as_str)
                        .filter(|scheme| self.allow_http || *scheme != "http")
                        .collect();
                    if let Err(problem) = sanitize::check_url(url, &allowed) {
                        fail(format!("{problem}."));
                    }
                }
                None => fail(mismatch()),
            },
            PropKind::Action => self.check_action(value, path, out),
            PropKind::Path => match value.as_str() {
                Some(target) if pointer::is_valid_target(target) => {}
                Some(target) => fail(format!(
                    "{}.",
                    pointer::parse(target)
                        .err()
                        .unwrap_or_else(|| PointerError::Syntax(target.to_string()))
                )),
                None => fail(mismatch()),
            },
            PropKind::Array { of, max } => match value.as_array() {
                Some(items) => {
                    let max = max.map_or(self.max_array_len, |max| max as usize);
                    if items.len() > max {
                        fail(format!("list has {} items, max is {max}.", items.len()));
                        return;
                    }
                    for (index, item) in items.iter().enumerate() {
                        self.check(of, item, &format!("{path}[{index}]"), out);
                    }
                }
                None => fail(mismatch()),
            },
            PropKind::Object { fields } => match value.as_object() {
                Some(map) => {
                    for (key, item) in map {
                        match catalog::lookup(fields, key) {
                            Some(field) if is_blank_label(key, field, self.bound(item)) => {
                                out.push((join_path(path, key), "label must not be blank.".into()))
                            }
                            Some(field) => self.check(field, item, &join_path(path, key), out),
                            None => out.push((
                                path.to_string(),
                                format!(
                                    "unknown field \"{}\". Known: {}.",
                                    clip(key),
                                    known_list(fields.iter().map(|(name, _)| name.as_str()))
                                ),
                            )),
                        }
                    }
                    for (name, field) in fields {
                        if field.required && !map.contains_key(name) {
                            out.push((join_path(path, name), "required field is missing.".into()));
                        }
                    }
                }
                None => fail(mismatch()),
            },
        }
    }

    fn bound<'v>(&self, value: &'v Value) -> &'v Value
    where
        'a: 'v,
    {
        value
            .get("$bind")
            .and_then(Value::as_str)
            .and_then(|target| self.data.and_then(|data| pointer::get(data, target)))
            .unwrap_or(value)
    }

    fn check_binding(
        &self,
        spec: &PropSpec,
        map: &Map<String, Value>,
        path: &str,
        out: &mut Vec<(String, String)>,
    ) {
        let mut fail = |message: String| out.push((path.to_string(), message));
        if !self.bindings || !spec.bindable {
            fail("$bind and $ref are not allowed here.".into());
            return;
        }
        if map.len() != 1 {
            fail("$bind or $ref must be the only key in its object.".into());
            return;
        }
        if let Some(target) = map.get("$bind") {
            let Some(target) = target.as_str() else {
                fail("$bind must be a JSON Pointer string like \"/form/name\".".into());
                return;
            };
            if let Err(error) = pointer::parse(target) {
                fail(format!("{error}."));
                return;
            }
            if target.is_empty() {
                fail("$bind must point inside the data model, like \"/form/name\".".into());
                return;
            }
            if let Some(found) = self.data.and_then(|data| pointer::get(data, target)) {
                let literal = Checker {
                    bindings: false,
                    handles: false,
                    ..*self
                };
                let mut nested = Vec::new();
                literal.check(spec, found, path, &mut nested);
                out.extend(nested.into_iter().map(|(at, message)| {
                    (at, format!("data at \"{target}\" does not fit: {message}"))
                }));
            }
            return;
        }
        let Some(reference) = map.get("$ref").and_then(Value::as_str) else {
            fail("$ref must be a string like \"source:key\".".into());
            return;
        };
        match split_ref(reference) {
            None => fail(format!(
                "$ref \"{}\" must look like \"source:key\".",
                clip(reference)
            )),
            Some((source, _)) => match self.refs.and_then(|refs| refs.source_kind(source)) {
                None => fail(format!(
                    "$ref source \"{}\" is not registered.",
                    clip(source)
                )),
                Some(declared)
                    if !ref_kind_fits(
                        &declared,
                        &spec.kind,
                        self.allow_http,
                        self.max_string_chars,
                    ) =>
                {
                    fail(format!(
                        "$ref source \"{}\" gives {}, but this prop needs {}.",
                        clip(source),
                        kind_text(&declared, self.max_string_chars),
                        kind_text(&spec.kind, self.max_string_chars)
                    ))
                }
                Some(_) => {}
            },
        }
    }

    fn check_action(&self, value: &Value, path: &str, out: &mut Vec<(String, String)>) {
        let Some(map) = value.as_object() else {
            out.push((
                path.to_string(),
                format!(
                    "expected {}, got {}.",
                    expected(&PropKind::Action),
                    describe(value)
                ),
            ));
            return;
        };
        if self.handles && map.contains_key("action_id") {
            if view::handle(value).is_none() {
                out.push((path.to_string(), "action handle is malformed.".into()));
            }
            return;
        }
        let mut fail = |at: String, message: String| out.push((at, message));
        for key in map.keys() {
            if !["action", "args", "with"].contains(&key.as_str()) {
                fail(
                    path.to_string(),
                    format!(
                        "unknown field \"{}\" in action. Known: action, args, with.",
                        clip(key)
                    ),
                );
            }
        }
        let Some(name) = map.get("action").and_then(Value::as_str) else {
            fail(
                path.to_string(),
                "action needs a string \"action\" name.".into(),
            );
            return;
        };
        let empty = Map::new();
        let args = match map.get("args") {
            None => &empty,
            Some(Value::Object(args)) => args,
            Some(_) => {
                fail(join_path(path, "args"), "args must be an object.".into());
                &empty
            }
        };
        match map.get("with") {
            None => {}
            Some(Value::Array(paths)) => {
                if paths.len() > MAX_WITH_PATHS {
                    fail(
                        join_path(path, "with"),
                        format!("with lists {} paths, max is {MAX_WITH_PATHS}.", paths.len()),
                    );
                }
                for (index, item) in paths.iter().enumerate() {
                    match item.as_str() {
                        Some(target) if pointer::is_valid_target(target) => {}
                        _ => fail(
                            format!("{path}.with[{index}]"),
                            "expected a JSON Pointer like \"/form/phone\".".into(),
                        ),
                    }
                }
            }
            Some(_) => fail(
                join_path(path, "with"),
                "with must be a list of JSON Pointers.".into(),
            ),
        }
        let Some(registry) = self.actions else {
            return;
        };
        let Some(spec) = registry.action(name) else {
            fail(
                path.to_string(),
                format!(
                    "action \"{}\" is not registered. Known: {}.",
                    clip(name),
                    known_list(registry.names().into_iter())
                ),
            );
            return;
        };
        let literal = Checker {
            bindings: false,
            handles: false,
            data: None,
            ..*self
        };
        let args_path = join_path(path, "args");
        for (key, item) in args {
            match spec.arg(key) {
                Some(arg) => literal.check(arg, item, &join_path(&args_path, key), out),
                None => out.push((
                    join_path(&args_path, key),
                    format!(
                        "unknown argument for \"{name}\". Known: {}.",
                        known_list(spec.args.iter().map(|(arg, _)| arg.as_str()))
                    ),
                )),
            }
        }
        for (key, arg) in &spec.args {
            if arg.required && !args.contains_key(key) {
                out.push((
                    join_path(&args_path, key),
                    "required argument is missing.".into(),
                ));
            }
        }
    }
}

pub fn without_nulls(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(_, item)| !item.is_null())
                .map(|(key, item)| (key.clone(), without_nulls(item)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(without_nulls).collect()),
        other => other.clone(),
    }
}

fn known_list<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let names: Vec<&str> = names.collect();
    if names.is_empty() {
        "none".into()
    } else {
        names.join(", ")
    }
}

pub fn check_literal(spec: &PropSpec, value: &Value) -> Result<(), String> {
    let mut out = Vec::new();
    Checker::literal(true).check(spec, value, "", &mut out);
    match out.into_iter().next() {
        None => Ok(()),
        Some((path, message)) if path.is_empty() => Err(message),
        Some((path, message)) => Err(format!("{path}: {message}")),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Surface {
    pub id: Option<String>,
    pub root: String,
    pub components: Vec<Component>,
    pub data: Map<String, Value>,
}

impl Surface {
    pub fn get(&self, id: &str) -> Option<&Component> {
        self.components.iter().find(|component| component.id == id)
    }

    pub fn data_value(&self) -> Value {
        Value::Object(self.data.clone())
    }

    pub fn fallback(&self, catalogs: &CatalogSet, locale: Locale) -> String {
        view::fallback_text(
            &self.root,
            &self.components,
            &self.data_value(),
            catalogs,
            locale,
        )
    }

    pub fn summary(&self, surface_id: &str, catalogs: &CatalogSet, locale: Locale) -> String {
        crate::summary::summarize(surface_id, self, catalogs, locale)
    }

    pub fn apply(&mut self, update: &Update) -> Result<(), PointerError> {
        let mut data = self.data_value();
        for op in &update.data {
            op.apply(&mut data)?;
        }
        self.data = match data {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        merge_components(&mut self.components, &update.upsert, &update.remove);
        if let Some(root) = &update.root {
            self.root = root.clone();
        }
        Ok(())
    }
}

fn merge_components(components: &mut Vec<Component>, upsert: &[Component], remove: &[String]) {
    components.retain(|component| !remove.contains(&component.id));
    for component in components.iter_mut() {
        component.children.retain(|child| !remove.contains(child));
    }
    for incoming in upsert {
        match components
            .iter_mut()
            .find(|existing| existing.id == incoming.id)
        {
            Some(slot) => *slot = incoming.clone(),
            None => components.push(incoming.clone()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Update {
    pub surface: String,
    pub upsert: Vec<Component>,
    pub remove: Vec<String>,
    pub root: Option<String>,
    pub data: Vec<DataOp>,
}

pub struct Validator<'a> {
    catalogs: &'a CatalogSet,
    actions: &'a dyn ActionRegistry,
    refs: &'a dyn RefResolver,
    limits: Limits,
    allow_http: bool,
}

impl<'a> Validator<'a> {
    pub fn new(
        catalogs: &'a CatalogSet,
        actions: &'a dyn ActionRegistry,
        refs: &'a dyn RefResolver,
    ) -> Self {
        Self {
            catalogs,
            actions,
            refs,
            limits: Limits::default(),
            allow_http: false,
        }
    }

    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    pub fn allow_http(mut self, allow: bool) -> Self {
        self.allow_http = allow;
        self
    }

    pub fn catalogs(&self) -> &CatalogSet {
        self.catalogs
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    fn checker<'b>(&'b self, data: Option<&'b Value>) -> Checker<'b> {
        Checker {
            bindings: true,
            handles: false,
            allow_http: self.allow_http,
            actions: Some(self.actions),
            refs: Some(self.refs),
            data,
            max_string_chars: self.limits.max_string_chars,
            max_array_len: self.limits.max_array_len,
        }
    }

    fn parse_input(&self, input: &str, operation: Operation) -> Result<Value, Problems> {
        let fail = |message: String| Problems {
            operation,
            items: vec![Problem::new(Location::Input, message)],
        };
        limits::prescan(input, &self.limits).map_err(|error| fail(format!("{error}.")))?;
        serde_json::from_str(input).map_err(|error| fail(format!("invalid JSON: {error}.")))
    }

    fn precheck(&self, input: &Value, operation: Operation) -> Result<(), Problems> {
        limits::check_value(input, &self.limits).map_err(|error| Problems {
            operation,
            items: vec![Problem::new(Location::Input, format!("{error}."))],
        })
    }

    pub fn show_ui_str(&self, input: &str) -> Result<Surface, Problems> {
        let value = self.parse_input(input, Operation::Show)?;
        self.show_ui(&value)
    }

    pub fn update_ui_str(&self, input: &str, current: &Surface) -> Result<Update, Problems> {
        let value = self.parse_input(input, Operation::Update)?;
        self.update_ui(&value, current)
    }

    pub fn close_ui_str(&self, input: &str) -> Result<String, Problems> {
        let value = self.parse_input(input, Operation::Close)?;
        self.close_ui(&value)
    }

    pub fn show_ui(&self, input: &Value) -> Result<Surface, Problems> {
        self.precheck(input, Operation::Show)?;
        let input = &without_nulls(input);
        let mut problems = Vec::new();
        let Some(map) = input.as_object() else {
            return Err(Problems {
                operation: Operation::Show,
                items: vec![Problem::new(
                    Location::Input,
                    "expected an object with root and components.",
                )],
            });
        };
        reject_unknown_fields(
            map,
            &["surface", "root", "components", "data"],
            &mut problems,
        );
        let id = optional_id(map.get("surface"), "surface", &mut problems);
        let root = required_string(map.get("root"), "root", &mut problems);
        let data = self.build_data(map.get("data"), &Value::Object(Map::new()), &mut problems);
        let mut components = Vec::new();
        let mut locations = Vec::new();
        match map.get("components") {
            Some(Value::Array(items)) => {
                if items.len() > self.limits.max_components {
                    problems.push(Problem::new(
                        Location::Field("components".into()),
                        format!(
                            "{} components, max is {}.",
                            items.len(),
                            self.limits.max_components
                        ),
                    ));
                }
                if items.is_empty() {
                    problems.push(Problem::new(
                        Location::Field("components".into()),
                        "list at least one component.",
                    ));
                }
                let mut seen = BTreeMap::new();
                for (index, item) in items.iter().enumerate() {
                    if let Some(component) = self.check_component(
                        "components",
                        index,
                        item,
                        Some(&data.1),
                        &mut problems,
                    ) {
                        if let Some(first) = seen.insert(component.id.clone(), index) {
                            problems.push(Problem::new(
                                item_location("components", index, &component.id),
                                format!("duplicate id, first used by components[{first}]."),
                            ));
                            continue;
                        }
                        locations.push(item_location("components", index, &component.id));
                        components.push(component);
                    }
                }
            }
            Some(_) => problems.push(Problem::new(
                Location::Field("components".into()),
                "components must be a list.",
            )),
            None => problems.push(Problem::new(
                Location::Field("components".into()),
                "components is required.",
            )),
        }
        if let Some(root) = &root {
            self.check_graph(root, &components, &locations, false, &mut problems);
        }
        finish(Operation::Show, problems)?;
        Ok(Surface {
            id,
            root: root.unwrap_or_default(),
            components,
            data: data.0,
        })
    }

    pub fn update_ui(&self, input: &Value, current: &Surface) -> Result<Update, Problems> {
        self.precheck(input, Operation::Update)?;
        let input = &without_nulls(input);
        let mut problems = Vec::new();
        let Some(map) = input.as_object() else {
            return Err(Problems {
                operation: Operation::Update,
                items: vec![Problem::new(
                    Location::Input,
                    "expected an object with surface and changes.",
                )],
            });
        };
        reject_unknown_fields(
            map,
            &["surface", "upsert", "remove", "root", "data"],
            &mut problems,
        );
        let surface = required_string(map.get("surface"), "surface", &mut problems);
        if let Some(surface) = &surface
            && !is_valid_id(surface)
        {
            problems.push(Problem::new(
                Location::Field("surface".into()),
                "surface must be 1 to 64 characters from A-Z, a-z, 0-9, _ and -.",
            ));
        }
        let root = match map.get("root") {
            None => None,
            other => required_string(other, "root", &mut problems),
        };
        let (_, data_after) =
            self.build_data(map.get("data"), &current.data_value(), &mut problems);
        let data_ops: Vec<DataOp> = data_entries(map.get("data"));
        let mut upsert = Vec::new();
        let mut upsert_locations = Vec::new();
        match map.get("upsert") {
            None => {}
            Some(Value::Array(items)) => {
                if items.len() > self.limits.max_components {
                    problems.push(Problem::new(
                        Location::Field("upsert".into()),
                        format!(
                            "{} components, max is {}.",
                            items.len(),
                            self.limits.max_components
                        ),
                    ));
                }
                let mut seen = BTreeMap::new();
                for (index, item) in items.iter().enumerate() {
                    if let Some(component) = self.check_component(
                        "upsert",
                        index,
                        item,
                        Some(&data_after),
                        &mut problems,
                    ) {
                        if let Some(first) = seen.insert(component.id.clone(), index) {
                            problems.push(Problem::new(
                                item_location("upsert", index, &component.id),
                                format!("duplicate id, first used by upsert[{first}]."),
                            ));
                            continue;
                        }
                        upsert_locations.push(item_location("upsert", index, &component.id));
                        upsert.push(component);
                    }
                }
            }
            Some(_) => problems.push(Problem::new(
                Location::Field("upsert".into()),
                "upsert must be a list.",
            )),
        }
        let mut remove = Vec::new();
        match map.get("remove") {
            None => {}
            Some(Value::Array(items)) => {
                for (index, item) in items.iter().enumerate() {
                    match item.as_str() {
                        Some(id) if current.get(id).is_some() => remove.push(id.to_string()),
                        Some(id) => problems.push(Problem::new(
                            Location::Entry {
                                list: "remove".into(),
                                index,
                                path: None,
                            },
                            format!("\"{}\" does not exist.", clip(id)),
                        )),
                        None => problems.push(Problem::new(
                            Location::Entry {
                                list: "remove".into(),
                                index,
                                path: None,
                            },
                            "expected a component id.",
                        )),
                    }
                }
            }
            Some(_) => problems.push(Problem::new(
                Location::Field("remove".into()),
                "remove must be a list of ids.",
            )),
        }
        if upsert.is_empty()
            && remove.is_empty()
            && root.is_none()
            && data_ops.is_empty()
            && problems.is_empty()
        {
            problems.push(Problem::new(
                Location::Input,
                "nothing to change: give upsert, remove, root or data.",
            ));
        }
        let mut merged = current.components.clone();
        merge_components(&mut merged, &upsert, &remove);
        let locations: Vec<Location> = merged
            .iter()
            .map(|component| {
                upsert
                    .iter()
                    .position(|candidate| candidate.id == component.id)
                    .and_then(|position| upsert_locations.get(position).cloned())
                    .unwrap_or_else(|| Location::Existing {
                        id: component.id.clone(),
                        prop: None,
                    })
            })
            .collect();
        let merged_root = root.clone().unwrap_or_else(|| current.root.clone());
        self.check_graph(&merged_root, &merged, &locations, false, &mut problems);
        for component in &merged {
            if upsert.iter().any(|candidate| candidate.id == component.id) {
                continue;
            }
            if let Some(definition) = self.catalogs.get(&component.type_name) {
                let base = Location::Existing {
                    id: component.id.clone(),
                    prop: None,
                };
                self.check_props(
                    definition,
                    &component.props,
                    &base,
                    Some(&data_after),
                    &mut problems,
                );
            }
        }
        finish(Operation::Update, problems)?;
        Ok(Update {
            surface: surface.unwrap_or_default(),
            upsert,
            remove,
            root,
            data: data_ops,
        })
    }

    pub fn close_ui(&self, input: &Value) -> Result<String, Problems> {
        self.precheck(input, Operation::Close)?;
        let input = &without_nulls(input);
        let mut problems = Vec::new();
        let Some(map) = input.as_object() else {
            return Err(Problems {
                operation: Operation::Close,
                items: vec![Problem::new(
                    Location::Input,
                    "expected an object with surface.",
                )],
            });
        };
        reject_unknown_fields(map, &["surface"], &mut problems);
        let surface = required_string(map.get("surface"), "surface", &mut problems);
        finish(Operation::Close, problems)?;
        Ok(surface.unwrap_or_default())
    }

    pub fn data(&self, value: &Value) -> Result<Map<String, Value>, Problems> {
        let mut problems = Vec::new();
        let value = without_nulls(value);
        let (map, _) = self.build_data(Some(&value), &Value::Object(Map::new()), &mut problems);
        finish(Operation::Show, problems)?;
        Ok(map)
    }

    pub fn component(
        &self,
        list: &str,
        index: usize,
        value: &Value,
        data: Option<&Value>,
    ) -> Result<Component, Vec<Problem>> {
        let mut problems = Vec::new();
        let value = without_nulls(value);
        let component = self.check_component(list, index, &value, data, &mut problems);
        match component {
            Some(component) if problems.is_empty() => Ok(component),
            _ => Err(problems),
        }
    }

    fn build_data(
        &self,
        value: Option<&Value>,
        base: &Value,
        problems: &mut Vec<Problem>,
    ) -> (Map<String, Value>, Value) {
        let mut data = base.clone();
        match value {
            None => {}
            Some(Value::Array(entries)) => {
                for (index, entry) in entries.iter().enumerate() {
                    let path = entry.get("path").and_then(Value::as_str);
                    let location = Location::Entry {
                        list: "data".into(),
                        index,
                        path: path.map(str::to_string),
                    };
                    let Some(map) = entry.as_object() else {
                        problems.push(Problem::new(
                            location,
                            "expected {\"path\": ..., \"value\": ...}.",
                        ));
                        continue;
                    };
                    if let Some(key) = map.keys().find(|key| *key != "path" && *key != "value") {
                        problems.push(Problem::new(
                            location.clone(),
                            format!("unknown field \"{}\". Known: path, value.", clip(key)),
                        ));
                    }
                    let Some(path) = path else {
                        problems.push(Problem::new(
                            location,
                            "path must be a JSON Pointer string.",
                        ));
                        continue;
                    };
                    if let Err(error) = pointer::parse(path) {
                        problems.push(Problem::new(location, format!("{error}.")));
                        continue;
                    }
                    if path.is_empty() {
                        problems.push(Problem::new(
                            location,
                            "path must point inside the data model, like \"/form/name\".",
                        ));
                        continue;
                    }
                    let item = map.get("value").unwrap_or(&Value::Null);
                    let scalar = PropSpec {
                        kind: PropKind::Scalar,
                        required: true,
                        default: None,
                        bindable: false,
                        description: None,
                    };
                    let mut out = Vec::new();
                    Checker {
                        bindings: false,
                        ..self.checker(None)
                    }
                    .check(&scalar, item, "", &mut out);
                    if let Some((_, message)) = out.into_iter().next() {
                        problems.push(Problem::new(location, format!("value: {message}")));
                        continue;
                    }
                    if let Err(error) = pointer::set(&mut data, path, item.clone()) {
                        problems.push(Problem::new(location, format!("{error}.")));
                    }
                }
            }
            Some(_) => problems.push(Problem::new(
                Location::Field("data".into()),
                "data must be a list of {\"path\": \"/a/b\", \"value\": ...} entries.",
            )),
        }
        let map = match &data {
            Value::Object(map) => map.clone(),
            _ => Map::new(),
        };
        if let Err(error) = limits::check_data_model(&data, &self.limits) {
            problems.push(Problem::new(
                Location::Field("data".into()),
                format!("the data model as sent to the client: {error}."),
            ));
        }
        (map, data)
    }

    fn check_component(
        &self,
        list: &str,
        index: usize,
        value: &Value,
        data: Option<&Value>,
        problems: &mut Vec<Problem>,
    ) -> Option<Component> {
        let base = Location::item(list, index);
        let Some(map) = value.as_object() else {
            problems.push(Problem::new(base, "expected an object with id and type."));
            return None;
        };
        let Some(id) = map.get("id").and_then(Value::as_str) else {
            problems.push(Problem::new(base, "missing \"id\"."));
            return None;
        };
        let base = item_location(list, index, id);
        if !is_valid_id(id) {
            problems.push(Problem::new(
                base.clone(),
                "id must be 1 to 64 characters from A-Z, a-z, 0-9, _ and -.",
            ));
        }
        for key in map.keys() {
            if !COMPONENT_FIELDS.contains(&key.as_str()) {
                problems.push(Problem::new(
                    base.clone(),
                    format!(
                        "unknown field \"{}\". Known: {}.",
                        clip(key),
                        COMPONENT_FIELDS.join(", ")
                    ),
                ));
            }
        }
        let type_name = map.get("type").and_then(Value::as_str).map(str::to_string);
        if type_name.is_none() {
            problems.push(Problem::new(base.clone(), "missing \"type\"."));
        }
        let definition = if let Some(type_name) = &type_name {
            let found = self.catalogs.get(type_name);
            if found.is_none() {
                problems.push(Problem::new(
                    base.clone(),
                    format!(
                        "type \"{}\" is not in the catalog. Known: {}.",
                        clip(type_name),
                        known_list(self.catalogs.type_names().into_iter())
                    ),
                ));
            }
            found
        } else {
            None
        };
        let props = match map.get("props") {
            None => Map::new(),
            Some(Value::Object(props)) => props.clone(),
            Some(_) => {
                problems.push(Problem::new(base.clone(), "props must be an object."));
                Map::new()
            }
        };
        if let Some(definition) = definition {
            self.check_props(definition, &props, &base, data, problems);
        }
        let children = match map.get("children") {
            None => Vec::new(),
            Some(Value::Array(items)) => {
                let ids: Vec<String> = items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect();
                if ids.len() != items.len() {
                    problems.push(Problem::new(
                        base.clone(),
                        "children must be a list of ids.",
                    ));
                }
                ids
            }
            Some(_) => {
                problems.push(Problem::new(
                    base.clone(),
                    "children must be a list of ids.",
                ));
                Vec::new()
            }
        };
        let text_field = |key: &str, problems: &mut Vec<Problem>| match map.get(key) {
            None => None,
            Some(Value::String(text)) => Some(text.clone()),
            Some(_) => {
                problems.push(Problem::new(
                    base.clone(),
                    format!("{key} must be a string."),
                ));
                None
            }
        };
        let alt = text_field("alt", problems);
        let say = text_field("say", problems);
        let voice = map
            .get("voice")
            .and_then(|voice| check_voice(voice, &base, problems));
        Some(Component {
            id: id.to_string(),
            type_name: type_name.unwrap_or_default(),
            props,
            children,
            alt,
            say,
            voice,
        })
    }

    fn check_props(
        &self,
        definition: &TypeDef,
        props: &Map<String, Value>,
        base: &Location,
        data: Option<&Value>,
        problems: &mut Vec<Problem>,
    ) {
        let checker = self.checker(data);
        for (name, value) in props {
            match definition.prop(name) {
                Some(spec) => {
                    let mut out = Vec::new();
                    checker.check(spec, value, name, &mut out);
                    problems.extend(
                        out.into_iter()
                            .map(|(path, message)| Problem::new(base.with_prop(&path), message)),
                    );
                }
                None => problems.push(Problem::new(
                    base.with_prop(&clip(name)),
                    format!(
                        "unknown prop for {}. Known: {}.",
                        definition.qualified,
                        known_list(definition.prop_names().into_iter())
                    ),
                )),
            }
        }
        for (name, spec) in &definition.props {
            if spec.required && !props.contains_key(name) {
                problems.push(Problem::new(
                    base.with_prop(name),
                    "required prop is missing.",
                ));
            }
        }
    }

    fn check_graph(
        &self,
        root: &str,
        components: &[Component],
        locations: &[Location],
        allow_orphans: bool,
        problems: &mut Vec<Problem>,
    ) {
        let at = |position: usize| -> Location {
            locations.get(position).cloned().unwrap_or(Location::Input)
        };
        if components.len() > self.limits.max_components {
            problems.push(Problem::new(
                Location::Field("components".into()),
                format!(
                    "the surface would have {} components, max is {}.",
                    components.len(),
                    self.limits.max_components
                ),
            ));
            return;
        }
        let root_position = view::position_of(components, root);
        if root_position.is_none() {
            problems.push(Problem::new(
                Location::Field("root".into()),
                format!("\"{}\" is not in components.", clip(root)),
            ));
        }
        let mut parent: Vec<Option<usize>> = vec![None; components.len()];
        for (position, component) in components.iter().enumerate() {
            if component.children.is_empty() {
                continue;
            }
            let definition = self.catalogs.get(&component.type_name);
            if let Some(definition) = definition
                && !definition.has_children()
            {
                problems.push(Problem::new(
                    at(position),
                    format!("{} cannot have children.", definition.qualified),
                ));
                continue;
            }
            for child_id in &component.children {
                let Some(child) = view::position_of(components, child_id) else {
                    problems.push(Problem::new(
                        at(position),
                        format!(
                            "children references \"{}\" which does not exist.",
                            clip(child_id)
                        ),
                    ));
                    continue;
                };
                if child == position {
                    problems.push(Problem::new(at(position), "lists itself as a child."));
                    continue;
                }
                if Some(child) == root_position {
                    problems.push(Problem::new(
                        at(position),
                        format!("children references the root \"{}\".", clip(root)),
                    ));
                    continue;
                }
                match parent[child] {
                    Some(existing) if existing == position => problems.push(Problem::new(
                        at(position),
                        format!("children lists \"{}\" twice.", clip(child_id)),
                    )),
                    Some(existing) => problems.push(Problem::new(
                        at(position),
                        format!(
                            "children references \"{}\" which already has parent \"{}\".",
                            clip(child_id),
                            clip(&components[existing].id)
                        ),
                    )),
                    None => {
                        parent[child] = Some(position);
                        if let (Some(definition), Some(child_definition)) =
                            (definition, self.catalogs.get(&components[child].type_name))
                            && !definition.allows_child(&child_definition.qualified)
                        {
                            let allowed = match &definition.children {
                                catalog::ChildRule::Only(names) => names.join(", "),
                                _ => String::new(),
                            };
                            problems.push(Problem::new(
                                at(position),
                                format!(
                                    "child \"{}\" has type {}, allowed: {allowed}.",
                                    clip(child_id),
                                    child_definition.qualified
                                ),
                            ));
                        }
                    }
                }
            }
        }
        let mut in_cycle = vec![false; components.len()];
        for start in 0..components.len() {
            if in_cycle[start] {
                continue;
            }
            let mut chain = vec![start];
            let mut current = start;
            while let Some(next) = parent[current] {
                if next == start {
                    for &member in &chain {
                        in_cycle[member] = true;
                    }
                    let mut names: Vec<&str> = chain
                        .iter()
                        .rev()
                        .map(|&member| components[member].id.as_str())
                        .collect();
                    names.push(&components[*chain.last().unwrap_or(&start)].id);
                    problems.push(Problem::new(
                        at(start),
                        format!("children make a cycle: {}.", names.join(" > ")),
                    ));
                    break;
                }
                if chain.len() > components.len() || chain.contains(&next) {
                    break;
                }
                chain.push(next);
                current = next;
            }
        }
        let mut depth = vec![0usize; components.len()];
        if let Some(root_position) = root_position {
            depth[root_position] = 1;
            let mut queue = vec![root_position];
            while let Some(current) = queue.pop() {
                for child_id in &components[current].children {
                    if let Some(child) = view::position_of(components, child_id)
                        && parent[child] == Some(current)
                        && depth[child] == 0
                    {
                        depth[child] = depth[current] + 1;
                        if depth[child] == self.limits.max_tree_depth + 1 {
                            problems.push(Problem::new(
                                at(child),
                                format!(
                                    "is {} levels deep, max is {}.",
                                    depth[child], self.limits.max_tree_depth
                                ),
                            ));
                        }
                        queue.push(child);
                    }
                }
            }
        }
        if allow_orphans || root_position.is_none() {
            return;
        }
        for position in 0..components.len() {
            if depth[position] == 0 && !in_cycle[position] {
                problems.push(Problem::new(
                    at(position),
                    format!("is not reachable from root \"{}\".", clip(root)),
                ));
            }
        }
    }
}

fn item_location(list: &str, index: usize, id: &str) -> Location {
    Location::Item {
        list: list.to_string(),
        index,
        id: Some(id.to_string()),
        prop: None,
    }
}

fn finish(operation: Operation, items: Vec<Problem>) -> Result<(), Problems> {
    if items.is_empty() {
        Ok(())
    } else {
        Err(Problems { operation, items })
    }
}

fn reject_unknown_fields(map: &Map<String, Value>, known: &[&str], problems: &mut Vec<Problem>) {
    for key in map.keys() {
        if !known.contains(&key.as_str()) {
            problems.push(Problem::new(
                Location::Input,
                format!(
                    "unknown field \"{}\". Known: {}.",
                    clip(key),
                    known.join(", ")
                ),
            ));
        }
    }
}

fn required_string(
    value: Option<&Value>,
    name: &str,
    problems: &mut Vec<Problem>,
) -> Option<String> {
    match value {
        Some(Value::String(text)) => Some(text.clone()),
        Some(_) => {
            problems.push(Problem::new(
                Location::Field(name.into()),
                format!("{name} must be a string."),
            ));
            None
        }
        None => {
            problems.push(Problem::new(
                Location::Field(name.into()),
                format!("{name} is required."),
            ));
            None
        }
    }
}

fn optional_id(value: Option<&Value>, name: &str, problems: &mut Vec<Problem>) -> Option<String> {
    let id = match value {
        None => return None,
        Some(_) => required_string(value, name, problems)?,
    };
    if !is_valid_id(&id) {
        problems.push(Problem::new(
            Location::Field(name.into()),
            format!("{name} must be 1 to 64 characters from A-Z, a-z, 0-9, _ and -."),
        ));
    }
    Some(id)
}

fn data_entries(value: Option<&Value>) -> Vec<DataOp> {
    value
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    Some(DataOp::Set {
                        path: entry.get("path")?.as_str()?.to_string(),
                        value: entry.get("value")?.clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn data_from_entries(entries: &[(String, Value)]) -> Result<Map<String, Value>, PointerError> {
    let mut data = Value::Object(Map::new());
    for (path, value) in entries {
        pointer::set(&mut data, path, value.clone())?;
    }
    Ok(match data {
        Value::Object(map) => map,
        _ => Map::new(),
    })
}

fn check_voice(value: &Value, base: &Location, problems: &mut Vec<Problem>) -> Option<VoiceHint> {
    let fail = |problems: &mut Vec<Problem>, message: &str| {
        problems.push(Problem::new(base.with_prop("voice"), message.to_string()));
    };
    let Some(map) = value.as_object() else {
        fail(
            problems,
            "voice must be an object like {\"ordinal\": 2, \"synonyms\": [\"gas\"]}.",
        );
        return None;
    };
    let mut hint = VoiceHint::default();
    for (key, item) in map {
        match key.as_str() {
            "ordinal" => match item.as_u64() {
                Some(ordinal @ 1..=1000) => hint.ordinal = u32::try_from(ordinal).ok(),
                _ => fail(
                    problems,
                    "voice.ordinal must be a whole number from 1 to 1000.",
                ),
            },
            "synonyms" => match item.as_array() {
                Some(words)
                    if words.len() <= MAX_SYNONYMS
                        && words
                            .iter()
                            .all(|word| word.as_str().is_some_and(|w| w.chars().count() <= 80)) =>
                {
                    hint.synonyms = words
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect();
                }
                _ => fail(
                    problems,
                    "voice.synonyms must be up to 12 strings of up to 80 characters.",
                ),
            },
            other => fail(
                problems,
                &format!(
                    "unknown field \"{}\" in voice. Known: ordinal, synonyms.",
                    clip(other)
                ),
            ),
        }
    }
    Some(hint)
}

#[derive(Debug, Clone, PartialEq)]
pub struct SealedAction {
    pub action_id: String,
    pub component: String,
    pub prop: String,
    pub action: String,
    pub args: Map<String, Value>,
    pub with: Vec<String>,
    pub requires_confirm: bool,
}

pub fn seal_actions<F: FnMut() -> String>(
    components: &mut [Component],
    catalogs: &CatalogSet,
    actions: &dyn ActionRegistry,
    mut next_id: F,
) -> Vec<SealedAction> {
    let mut sealed = Vec::new();
    for component in components.iter_mut() {
        let Some(definition) = catalogs.get(&component.type_name) else {
            continue;
        };
        let id = component.id.clone();
        for (name, value) in component.props.iter_mut() {
            if let Some(spec) = definition.prop(name) {
                seal_value(spec, value, name, &id, actions, &mut next_id, &mut sealed);
            }
        }
    }
    sealed
}

fn seal_value(
    spec: &PropSpec,
    value: &mut Value,
    path: &str,
    component: &str,
    actions: &dyn ActionRegistry,
    next_id: &mut dyn FnMut() -> String,
    sealed: &mut Vec<SealedAction>,
) {
    match (&spec.kind, value) {
        (PropKind::Action, value) => {
            let Some(name) = value
                .get("action")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return;
            };
            let args = value
                .get("args")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let with = view::handle_paths(value);
            let requires_confirm = actions
                .action(&name)
                .is_none_or(|action| action.requires_confirm);
            let action_id = next_id();
            let mut handle = Map::new();
            handle.insert("action_id".into(), Value::from(action_id.clone()));
            handle.insert("confirm".into(), Value::from(requires_confirm));
            if !with.is_empty() {
                handle.insert("with".into(), Value::from(with.clone()));
            }
            *value = Value::Object(handle);
            sealed.push(SealedAction {
                action_id,
                component: component.to_string(),
                prop: path.to_string(),
                action: name,
                args,
                with,
                requires_confirm,
            });
        }
        (PropKind::Array { of, .. }, Value::Array(items)) => {
            for (index, item) in items.iter_mut().enumerate() {
                seal_value(
                    of,
                    item,
                    &format!("{path}[{index}]"),
                    component,
                    actions,
                    next_id,
                    sealed,
                );
            }
        }
        (PropKind::Object { fields }, Value::Object(map)) => {
            for (key, item) in map.iter_mut() {
                if let Some(field) = catalog::lookup(fields, key) {
                    seal_value(
                        field,
                        item,
                        &join_path(path, key),
                        component,
                        actions,
                        next_id,
                        sealed,
                    );
                }
            }
        }
        _ => {}
    }
}

fn ref_value(value: &Value) -> Option<&str> {
    value
        .as_object()
        .filter(|map| map.len() == 1)
        .and_then(|map| map.get("$ref"))
        .and_then(Value::as_str)
}

impl Validator<'_> {
    pub fn resolve_refs(
        &self,
        components: &[Component],
        operation: Operation,
    ) -> Result<Vec<Component>, Problems> {
        let list = if operation == Operation::Update {
            "upsert"
        } else {
            "components"
        };
        let mut problems = Vec::new();
        let mut out = components.to_vec();
        for (index, component) in out.iter_mut().enumerate() {
            let Some(definition) = self.catalogs.get(&component.type_name) else {
                continue;
            };
            let base = item_location(list, index, &component.id);
            for (name, value) in component.props.iter_mut() {
                if let Some(spec) = definition.prop(name) {
                    let mut failures = Vec::new();
                    self.resolve_value(spec, value, name, &mut failures);
                    problems.extend(
                        failures
                            .into_iter()
                            .map(|path| Problem::new(base.with_prop(&path), REF_FAILURE)),
                    );
                }
            }
        }
        finish(operation, problems)?;
        Ok(out)
    }

    fn resolved(&self, spec: &PropSpec, reference: &str) -> Option<Value> {
        let (source, key) = split_ref(reference)?;
        let declared = self.refs.source_kind(source)?;
        if !ref_kind_fits(
            &declared,
            &spec.kind,
            self.allow_http,
            self.limits.max_string_chars,
        ) {
            return None;
        }
        let value = self.refs.resolve(source, key).ok()?;
        let checker = Checker {
            bindings: false,
            handles: false,
            data: None,
            ..self.checker(None)
        };
        let declared_spec = PropSpec {
            kind: declared,
            ..spec.clone()
        };
        let mut out = Vec::new();
        checker.check(&declared_spec, &value, "", &mut out);
        checker.check(spec, &value, "", &mut out);
        out.is_empty().then_some(value)
    }

    fn resolve_value(
        &self,
        spec: &PropSpec,
        value: &mut Value,
        path: &str,
        failures: &mut Vec<String>,
    ) {
        if let Some(reference) = ref_value(value).map(str::to_string) {
            match self.resolved(spec, &reference) {
                Some(resolved) => *value = resolved,
                None => failures.push(path.to_string()),
            }
            return;
        }
        match (&spec.kind, value) {
            (PropKind::Array { of, .. }, Value::Array(items)) => {
                for (index, item) in items.iter_mut().enumerate() {
                    self.resolve_value(of, item, &format!("{path}[{index}]"), failures);
                }
            }
            (PropKind::Object { fields }, Value::Object(map)) => {
                for (key, item) in map.iter_mut() {
                    if let Some(field) = catalog::lookup(fields, key) {
                        let target = join_path(path, key);
                        let before = failures.len();
                        self.resolve_value(field, item, &target, failures);
                        if failures.len() == before && is_blank_label(key, field, item) {
                            failures.push(target);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn downgrade_for_client(
    surface: &Surface,
    full: &CatalogSet,
    client: &CatalogSet,
    locale: Locale,
) -> Surface {
    let mut out = surface.clone();
    let data = surface.data_value();
    let mut hidden: Vec<String> = Vec::new();
    for component in &surface.components {
        if client.get(&component.type_name).is_some() {
            continue;
        }
        let mut pending = component.children.clone();
        while let Some(id) = pending.pop() {
            if hidden.contains(&id) {
                continue;
            }
            if let Some(child) = surface.get(&id) {
                pending.extend(child.children.iter().cloned());
            }
            hidden.push(id);
        }
    }
    out.components
        .retain(|component| !hidden.contains(&component.id));
    for component in out.components.iter_mut() {
        if client.get(&component.type_name).is_some() {
            continue;
        }
        let mut subtree = surface.clone();
        subtree.root = component.id.clone();
        let text = view::fallback_text(&subtree.root, &subtree.components, &data, full, locale);
        let mut props = Map::new();
        props.insert("text".into(), Value::from(text.clone()));
        props.insert("style".into(), Value::from("body"));
        *component = Component {
            id: component.id.clone(),
            type_name: "Text".into(),
            props,
            children: Vec::new(),
            alt: Some(text),
            say: component.say.clone(),
            voice: component.voice.clone(),
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fits::fits;
    use serde_json::json;

    fn every_spec(spec: &PropSpec, out: &mut Vec<PropSpec>) {
        out.push(spec.clone());
        match &spec.kind {
            PropKind::Array { of, .. } => every_spec(of, out),
            PropKind::Object { fields } => {
                for (_, field) in fields {
                    every_spec(field, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn fits_agrees_with_check_literal() {
        let mut specs = Vec::new();
        for definition in CatalogSet::builtin().types() {
            for (_, spec) in &definition.props {
                every_spec(spec, &mut specs);
            }
        }
        for extra in [
            json!({ "kind": "int", "min": -2, "max": 5 }),
            json!({ "kind": "number", "min": -1.5, "max": 2.5 }),
            json!({ "kind": "string", "min": 2, "max": 3 }),
            json!({ "kind": "url", "schemes": ["https", "http", "tel"] }),
            json!({ "kind": "array", "max": 2, "of": { "kind": "bool" } }),
            json!({ "kind": "array", "of": { "kind": "action" } }),
            json!({ "kind": "object", "fields": { "a": { "kind": "int", "required": true }, "b": { "kind": "path" } } }),
        ] {
            specs.push(PropSpec::from_value(&extra).unwrap());
        }
        let long = "x".repeat(4097);
        let values = [
            json!(null),
            json!(true),
            json!(0),
            json!(-3),
            json!(4),
            json!(9),
            json!(u64::MAX),
            json!(1.5),
            json!(-7.25),
            json!(""),
            json!("a"),
            json!("abc"),
            json!("abcd"),
            json!(long),
            json!("primary"),
            json!("bar"),
            json!("https://example.com/a.png"),
            json!("http://example.com"),
            json!("javascript:alert(1)"),
            json!("tel:+998901234567"),
            json!("/form/phone"),
            json!("form"),
            json!([]),
            json!([true, false]),
            json!([true, false, true]),
            json!([{ "id": "a", "label": "A" }]),
            json!([{ "id": "a" }]),
            json!([{ "id": "a", "label": " " }]),
            json!([{ "x": "Jan", "y": 1 }, { "x": 2, "y": 3.5, "series": "s" }]),
            json!([{ "x": "Jan", "y": "1" }]),
            json!({ "$bind": "/a" }),
            json!({ "$ref": "bill:1" }),
            json!({ "a": 1 }),
            json!({ "a": 1, "b": "/x" }),
            json!({ "a": 1, "c": 2 }),
            json!({ "b": "/x" }),
            json!({ "action_id": "act_1", "confirm": true }),
            json!({ "action_id": "" }),
            json!({ "action": "pay", "args": { "bill": "b1" }, "with": ["/form/phone"] }),
            json!({ "action": "pay", "args": 3 }),
            json!({ "action": "pay", "with": ["nope"] }),
            json!({ "action": "pay", "extra": 1 }),
            json!({ "args": {} }),
            json!([{ "action": "pay" }, { "action_id": "a" }]),
        ];
        for spec in &specs {
            for value in &values {
                assert_eq!(
                    fits(spec, value, true),
                    check_literal(spec, value).is_ok(),
                    "{:?} with {value}",
                    spec.kind
                );
            }
        }
    }

    #[test]
    fn problems_render_for_the_model() {
        let problems = Problems {
            operation: Operation::Show,
            items: vec![
                Problem::new(
                    item_location("components", 3, "pay"),
                    "type \"PayButton\" is not in the catalog. Known: Button.",
                ),
                Problem::new(
                    Location::item("components", 1).with_prop("label"),
                    "required prop is missing.",
                ),
            ],
        };
        assert_eq!(
            problems.to_string(),
            "2 problems, nothing was shown:\n1. components[3] id \"pay\": type \"PayButton\" is not in the catalog. Known: Button.\n2. components[1] prop \"label\": required prop is missing."
        );
        let many = Problems {
            operation: Operation::Update,
            items: (0..25)
                .map(|n| Problem::new(Location::Field(format!("f{n}")), "bad."))
                .collect(),
        };
        let text = many.to_string();
        assert!(text.starts_with("25 problems, nothing was changed:"));
        assert!(text.ends_with("... and 5 more."));
        let one = Problems {
            operation: Operation::Close,
            items: vec![Problem::new(Location::Input, "x.")],
        };
        assert_eq!(
            one.to_string(),
            "1 problem, nothing was closed:\n1. input: x."
        );
    }

    #[test]
    fn ids_and_refs() {
        assert!(is_valid_id("pay_bill-2"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("a b"));
        assert!(!is_valid_id(&"a".repeat(65)));
        assert_eq!(split_ref("account:card_1"), Some(("account", "card_1")));
        assert_eq!(split_ref("Account:x"), None);
        assert_eq!(split_ref("account:"), None);
        assert_eq!(split_ref("account:a b"), None);
        assert_eq!(split_ref("nocolon"), None);
    }

    #[test]
    fn literal_checks() {
        let spec = |value: Value| PropSpec::from_value(&value).unwrap();
        assert!(
            check_literal(
                &spec(json!({ "kind": "int", "min": 1, "max": 4 })),
                &json!(4)
            )
            .is_ok()
        );
        assert!(check_literal(&spec(json!({ "kind": "int" })), &json!(2.5)).is_err());
        assert!(check_literal(&spec(json!({ "kind": "number", "max": 1.5 })), &json!(2)).is_err());
        assert!(check_literal(&spec(json!({ "kind": "scalar" })), &json!(null)).is_err());
        assert!(check_literal(&spec(json!({ "kind": "string", "min": 2 })), &json!("a")).is_err());
        assert!(
            check_literal(
                &spec(json!({ "kind": "action" })),
                &json!({ "action_id": "a1" })
            )
            .is_ok()
        );
        assert!(check_literal(&spec(json!({ "kind": "path" })), &json!("/a")).is_ok());
        assert!(check_literal(&spec(json!({ "kind": "path" })), &json!("a")).is_err());
        assert!(
            check_literal(
                &spec(json!({ "kind": "string" })),
                &json!({ "$bind": "/a" })
            )
            .is_err()
        );
    }

    #[test]
    fn action_specs_parse() {
        let action = ActionSpec::from_json(
            r#"{"args":{"bill":{"kind":"string","required":true}},"requires_confirm":true}"#,
        )
        .unwrap();
        assert!(action.requires_confirm);
        assert!(action.arg("bill").is_some_and(|spec| spec.required));
        assert!(ActionSpec::from_json(r#"{"args":{"x":{"kind":"nope"}}}"#).is_err());
        assert!(ActionSpec::from_json(r#"{"confirm":true}"#).is_err());
        let table = ActionTable::new()
            .with("a", ActionSpec::new(false))
            .with("a", ActionSpec::new(true));
        assert_eq!(table.names(), vec!["a"]);
        assert!(table.action("a").is_some_and(|spec| spec.requires_confirm));
    }
}
