use crate::Value;
use crate::catalog::{self, PropKind, PropSpec};
use crate::limits::Limits;
use crate::pointer;
use crate::sanitize;
use crate::view;

pub const MAX_WITH_PATHS: usize = 32;
const LABEL_FIELD: &str = "label";

fn fits_action(value: &Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    if map.contains_key("action_id") {
        return view::handle(value).is_some();
    }
    let known_keys = map
        .keys()
        .all(|key| ["action", "args", "with"].contains(&key.as_str()));
    let args_ok = map.get("args").is_none_or(Value::is_object);
    let with_ok = map.get("with").is_none_or(|with| {
        with.as_array().is_some_and(|paths| {
            paths.len() <= MAX_WITH_PATHS
                && paths
                    .iter()
                    .all(|path| path.as_str().is_some_and(pointer::is_valid_target))
        })
    });
    known_keys && map.get("action").is_some_and(Value::is_string) && args_ok && with_ok
}

pub(crate) fn is_blank_label(key: &str, field: &PropSpec, value: &Value) -> bool {
    key == LABEL_FIELD
        && matches!(field.kind, PropKind::String { .. })
        && value.as_str().is_some_and(|text| text.trim().is_empty())
}

fn fits_within(spec: &PropSpec, value: &Value, limits: &Limits, allow_http: bool) -> bool {
    if let Value::Object(map) = value
        && (map.contains_key("$bind") || map.contains_key("$ref"))
    {
        return false;
    }
    let length_ok = |text: &str, max: usize| text.chars().count() <= max;
    match &spec.kind {
        PropKind::String { min, max } => value.as_str().is_some_and(|text| {
            let length = text.chars().count();
            length <= max.map_or(limits.max_string_chars, |max| max as usize)
                && min.is_none_or(|min| length as u64 >= min)
        }),
        PropKind::Int { min, max } => value
            .as_i64()
            .or_else(|| value.as_u64().map(|_| i64::MAX))
            .is_some_and(|number| {
                min.is_none_or(|min| number >= min) && max.is_none_or(|max| number <= max)
            }),
        PropKind::Number { min, max } => value.as_f64().is_some_and(|number| {
            number.is_finite()
                && min.is_none_or(|min| number >= min)
                && max.is_none_or(|max| number <= max)
        }),
        PropKind::Bool => value.is_boolean(),
        PropKind::Scalar => match value {
            Value::String(text) => length_ok(text, limits.max_string_chars),
            Value::Bool(_) | Value::Number(_) => true,
            _ => false,
        },
        PropKind::Enum { values } => value
            .as_str()
            .is_some_and(|text| values.iter().any(|allowed| allowed == text)),
        PropKind::Url { schemes } => value.as_str().is_some_and(|url| {
            let allowed: Vec<&str> = schemes
                .iter()
                .map(String::as_str)
                .filter(|scheme| allow_http || *scheme != "http")
                .collect();
            sanitize::is_safe_url(url, &allowed)
        }),
        PropKind::Action => fits_action(value),
        PropKind::Path => value.as_str().is_some_and(pointer::is_valid_target),
        PropKind::Array { of, max } => value.as_array().is_some_and(|items| {
            items.len() <= max.map_or(limits.max_array_len, |max| max as usize)
                && items
                    .iter()
                    .all(|item| fits_within(of, item, limits, allow_http))
        }),
        PropKind::Object { fields } => value.as_object().is_some_and(|map| {
            map.iter().all(|(key, item)| {
                catalog::lookup(fields, key).is_some_and(|field| {
                    !is_blank_label(key, field, item)
                        && fits_within(field, item, limits, allow_http)
                })
            }) && fields
                .iter()
                .all(|(name, field)| !field.required || map.contains_key(name))
        }),
    }
}

pub(crate) fn fits(spec: &PropSpec, value: &Value, allow_http: bool) -> bool {
    fits_within(spec, value, &Limits::default(), allow_http)
}
