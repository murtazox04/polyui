use serde_json::{Map, Value};
use std::fmt;

pub const SUPPORTED_KEYWORDS: [&str; 27] = [
    "anyOf",
    "oneOf",
    "allOf",
    "$ref",
    "const",
    "enum",
    "type",
    "items",
    "additionalItems",
    "prefixItems",
    "minItems",
    "maxItems",
    "properties",
    "additionalProperties",
    "patternProperties",
    "required",
    "minProperties",
    "maxProperties",
    "minLength",
    "maxLength",
    "pattern",
    "format",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
];

pub const ANNOTATIONS: [&str; 15] = [
    "$anchor",
    "$defs",
    "definitions",
    "$schema",
    "$id",
    "id",
    "$comment",
    "title",
    "description",
    "default",
    "readOnly",
    "writeOnly",
    "examples",
    "contentMediaType",
    "contentEncoding",
];

pub const UNSUPPORTED_KEYWORDS: [&str; 18] = [
    "not",
    "if",
    "then",
    "else",
    "uniqueItems",
    "contains",
    "minContains",
    "maxContains",
    "propertyNames",
    "dependencies",
    "dependentRequired",
    "dependentSchemas",
    "unevaluatedItems",
    "unevaluatedProperties",
    "$recursiveRef",
    "$recursiveAnchor",
    "$dynamicRef",
    "$dynamicAnchor",
];

pub const FORMATS: [&str; 11] = [
    "date-time",
    "time",
    "date",
    "duration",
    "email",
    "hostname",
    "ipv4",
    "ipv6",
    "uuid",
    "uri",
    "unknown",
];

const DROPPED_ANNOTATIONS: [&str; 5] = ["description", "title", "examples", "default", "$comment"];

const SCHEMA_MAPS: [&str; 5] = [
    "properties",
    "patternProperties",
    "$defs",
    "definitions",
    "dependentSchemas",
];

const SCHEMA_LISTS: [&str; 4] = ["anyOf", "oneOf", "allOf", "prefixItems"];

const SCHEMA_VALUES: [&str; 3] = ["items", "additionalItems", "additionalProperties"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Dropped { path: String, keyword: String },
    OneOfToAnyOf { path: String },
    UnknownFormat { path: String, format: String },
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dropped { path, keyword } => write!(
                f,
                "{path}: dropped \"{keyword}\", llguidance does not support it; the server still checks it"
            ),
            Self::OneOfToAnyOf { path } => write!(
                f,
                "{path}: oneOf became anyOf; the server still checks that exactly one branch fits"
            ),
            Self::UnknownFormat { path, format } => write!(
                f,
                "{path}: dropped format \"{format}\", llguidance does not know it"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    RemoteRef { path: String, target: String },
    NotASchema { path: String },
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RemoteRef { path, target } => write!(
                f,
                "{path}: $ref \"{target}\" points outside the schema, llguidance only follows local refs"
            ),
            Self::NotASchema { path } => write!(f, "{path}: expected a schema object or boolean"),
        }
    }
}

impl std::error::Error for SchemaError {}

#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub schema: Value,
    pub changes: Vec<Change>,
}

pub fn prepare(schema: &Value) -> Result<Prepared, SchemaError> {
    let mut changes = Vec::new();
    let schema = rewrite(schema, "#", &mut changes)?;
    Ok(Prepared { schema, changes })
}

fn rewrite(schema: &Value, path: &str, changes: &mut Vec<Change>) -> Result<Value, SchemaError> {
    let map = match schema {
        Value::Bool(_) => return Ok(schema.clone()),
        Value::Object(map) => map,
        _ => {
            return Err(SchemaError::NotASchema {
                path: path.to_string(),
            });
        }
    };
    let mut out = Map::new();
    for (key, value) in map {
        let here = format!("{path}/{key}");
        match key.as_str() {
            keyword if DROPPED_ANNOTATIONS.contains(&keyword) => {}
            keyword if UNSUPPORTED_KEYWORDS.contains(&keyword) => changes.push(Change::Dropped {
                path: path.to_string(),
                keyword: keyword.to_string(),
            }),
            "$ref" => {
                let target = value.as_str().unwrap_or_default();
                if !target.starts_with('#') {
                    return Err(SchemaError::RemoteRef {
                        path: path.to_string(),
                        target: target.to_string(),
                    });
                }
                out.insert(key.clone(), value.clone());
            }
            "format" => match value.as_str() {
                Some(format) if FORMATS.contains(&format) => {
                    out.insert(key.clone(), value.clone());
                }
                _ => changes.push(Change::UnknownFormat {
                    path: path.to_string(),
                    format: value.as_str().unwrap_or_default().to_string(),
                }),
            },
            "oneOf" => {
                changes.push(Change::OneOfToAnyOf {
                    path: path.to_string(),
                });
                out.insert("anyOf".into(), rewrite_list(value, &here, changes)?);
            }
            keyword if SCHEMA_LISTS.contains(&keyword) => {
                out.insert(key.clone(), rewrite_list(value, &here, changes)?);
            }
            keyword if SCHEMA_MAPS.contains(&keyword) => {
                out.insert(key.clone(), rewrite_map(value, &here, changes)?);
            }
            keyword if SCHEMA_VALUES.contains(&keyword) => {
                out.insert(key.clone(), rewrite(value, &here, changes)?);
            }
            _ => {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    Ok(Value::Object(out))
}

fn rewrite_list(
    value: &Value,
    path: &str,
    changes: &mut Vec<Change>,
) -> Result<Value, SchemaError> {
    let Value::Array(items) = value else {
        return Err(SchemaError::NotASchema {
            path: path.to_string(),
        });
    };
    items
        .iter()
        .enumerate()
        .map(|(index, item)| rewrite(item, &format!("{path}/{index}"), changes))
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn rewrite_map(value: &Value, path: &str, changes: &mut Vec<Change>) -> Result<Value, SchemaError> {
    let Value::Object(entries) = value else {
        return Err(SchemaError::NotASchema {
            path: path.to_string(),
        });
    };
    let mut out = Map::new();
    for (name, schema) in entries {
        out.insert(
            name.clone(),
            rewrite(schema, &format!("{path}/{name}"), changes)?,
        );
    }
    Ok(Value::Object(out))
}

pub fn keywords(schema: &Value) -> Vec<String> {
    let mut found = Vec::new();
    collect_keywords(schema, &mut found);
    found.sort();
    found.dedup();
    found
}

fn collect_keywords(schema: &Value, found: &mut Vec<String>) {
    let Value::Object(map) = schema else {
        return;
    };
    for (key, value) in map {
        found.push(key.clone());
        if SCHEMA_LISTS.contains(&key.as_str()) {
            for item in value.as_array().into_iter().flatten() {
                collect_keywords(item, found);
            }
        } else if SCHEMA_MAPS.contains(&key.as_str()) {
            for item in value
                .as_object()
                .into_iter()
                .flatten()
                .map(|(_, item)| item)
            {
                collect_keywords(item, found);
            }
        } else if SCHEMA_VALUES.contains(&key.as_str()) {
            collect_keywords(value, found);
        }
    }
}

pub fn is_supported_keyword(keyword: &str) -> bool {
    SUPPORTED_KEYWORDS.contains(&keyword) || ANNOTATIONS.contains(&keyword)
}
