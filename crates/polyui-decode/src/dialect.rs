use crate::schema::{SCHEMA_LISTS, SCHEMA_MAPS, SCHEMA_VALUES};
use serde_json::{Map, Value};
use std::fmt;

const UNIONS: [&str; 2] = ["anyOf", "oneOf"];

const POSITIONAL: [&str; 3] = ["prefixItems", "items", "additionalItems"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SchemaDialect {
    #[default]
    Standard,
    NoSameTypeUnions,
}

impl SchemaDialect {
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::NoSameTypeUnions => "no-same-type-unions",
        }
    }

    pub fn apply(self, schema: &Value) -> Value {
        match self {
            Self::Standard => schema.clone(),
            Self::NoSameTypeUnions => merge_same_type_unions(schema),
        }
    }
}

impl fmt::Display for SchemaDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

impl Family {
    fn of_name(name: &str) -> Option<Self> {
        match name {
            "null" => Some(Self::Null),
            "boolean" => Some(Self::Boolean),
            "integer" | "number" => Some(Self::Number),
            "string" => Some(Self::String),
            "array" => Some(Self::Array),
            "object" => Some(Self::Object),
            _ => None,
        }
    }

    fn of_value(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(_) => Self::Boolean,
            Value::Number(_) => Self::Number,
            Value::String(_) => Self::String,
            Value::Array(_) => Self::Array,
            Value::Object(_) => Self::Object,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
        }
    }
}

enum Piece {
    Anything,
    Typed(Family, Value),
    Other(Value),
}

#[derive(Clone, Copy)]
enum Bound {
    Least,
    Most,
}

pub fn merge_same_type_unions(schema: &Value) -> Value {
    let Value::Object(map) = schema else {
        return schema.clone();
    };
    let mut out = Map::new();
    let mut branches = Vec::new();
    let mut union = false;
    for (key, value) in map {
        let keyword = key.as_str();
        if UNIONS.contains(&keyword) {
            union = true;
            branches.extend(
                value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(merge_same_type_unions),
            );
            continue;
        }
        let merged = if SCHEMA_LISTS.contains(&keyword) {
            match value {
                Value::Array(items) => {
                    Value::Array(items.iter().map(merge_same_type_unions).collect())
                }
                other => other.clone(),
            }
        } else if SCHEMA_MAPS.contains(&keyword) {
            match value {
                Value::Object(entries) => Value::Object(
                    entries
                        .iter()
                        .map(|(name, entry)| (name.clone(), merge_same_type_unions(entry)))
                        .collect(),
                ),
                other => other.clone(),
            }
        } else if SCHEMA_VALUES.contains(&keyword) {
            merge_same_type_unions(value)
        } else {
            value.clone()
        };
        out.insert(key.clone(), merged);
    }
    if !union {
        return Value::Object(out);
    }
    let joined = join(branches);
    if out.is_empty() {
        return joined;
    }
    let alternatives = match joined {
        Value::Object(mut single) if single.len() == 1 && single.contains_key("anyOf") => {
            single.remove("anyOf").unwrap_or_default()
        }
        other => Value::Array(vec![other]),
    };
    out.insert("anyOf".into(), alternatives);
    Value::Object(out)
}

fn join(branches: Vec<Value>) -> Value {
    let mut groups: Vec<(Family, Vec<Value>)> = Vec::new();
    let mut others: Vec<Value> = Vec::new();
    for piece in branches.into_iter().flat_map(pieces) {
        match piece {
            Piece::Anything => return Value::Object(Map::new()),
            Piece::Typed(family, schema) => {
                match groups.iter_mut().find(|(found, _)| *found == family) {
                    Some((_, schemas)) => {
                        if !schemas.contains(&schema) {
                            schemas.push(schema);
                        }
                    }
                    None => groups.push((family, vec![schema])),
                }
            }
            Piece::Other(schema) => {
                if !others.contains(&schema) {
                    others.push(schema);
                }
            }
        }
    }
    let mut joined: Vec<Value> = groups
        .into_iter()
        .map(|(family, schemas)| combine(family, schemas))
        .collect();
    joined.extend(others);
    match joined.len() {
        0 => Value::Bool(false),
        1 => joined.remove(0),
        _ => {
            let mut union = Map::new();
            union.insert("anyOf".into(), Value::Array(joined));
            Value::Object(union)
        }
    }
}

fn pieces(schema: Value) -> Vec<Piece> {
    let map = match schema {
        Value::Bool(true) => return vec![Piece::Anything],
        Value::Bool(false) => return Vec::new(),
        Value::Object(map) => map,
        other => return vec![Piece::Other(other)],
    };
    if map.is_empty() {
        return vec![Piece::Anything];
    }
    if map.len() == 1
        && let Some(Value::Array(inner)) = map.get("anyOf")
    {
        return inner.clone().into_iter().flat_map(pieces).collect();
    }
    match map.get("type") {
        Some(Value::String(name)) => match Family::of_name(name) {
            Some(family) => vec![Piece::Typed(family, Value::Object(map))],
            None => vec![Piece::Other(Value::Object(map))],
        },
        Some(Value::Array(names)) => {
            let names: Vec<String> = names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            names
                .iter()
                .filter_map(|name| {
                    let Some(family) = Family::of_name(name) else {
                        return Some(Piece::Other(Value::Object(map.clone())));
                    };
                    let mut part = narrowed(&map, family)?;
                    part.insert("type".into(), Value::from(name.as_str()));
                    Some(Piece::Typed(family, Value::Object(part)))
                })
                .collect()
        }
        Some(_) => vec![Piece::Other(Value::Object(map))],
        None => by_values(map),
    }
}

fn narrowed(map: &Map<String, Value>, family: Family) -> Option<Map<String, Value>> {
    if let Some(constant) = map.get("const")
        && Family::of_value(constant) != family
    {
        return None;
    }
    let mut part = map.clone();
    if let Some(Value::Array(values)) = map.get("enum") {
        let kept: Vec<Value> = values
            .iter()
            .filter(|value| Family::of_value(value) == family)
            .cloned()
            .collect();
        if kept.is_empty() {
            return None;
        }
        part.insert("enum".into(), Value::Array(kept));
    }
    Some(part)
}

fn by_values(map: Map<String, Value>) -> Vec<Piece> {
    if let Some(constant) = map.get("const") {
        let family = Family::of_value(constant);
        return vec![Piece::Typed(family, Value::Object(map))];
    }
    let Some(Value::Array(values)) = map.get("enum") else {
        return vec![Piece::Other(Value::Object(map))];
    };
    let mut families: Vec<Family> = Vec::new();
    for value in values {
        let family = Family::of_value(value);
        if !families.contains(&family) {
            families.push(family);
        }
    }
    families
        .into_iter()
        .filter_map(|family| {
            narrowed(&map, family).map(|part| Piece::Typed(family, Value::Object(part)))
        })
        .collect()
}

fn combine(family: Family, mut schemas: Vec<Value>) -> Value {
    if schemas.len() == 1 {
        return schemas.remove(0);
    }
    if let Some(values) = enumerated(&schemas) {
        let integers = values.iter().all(|value| value.is_i64() || value.is_u64());
        let mut out = Map::new();
        out.insert("type".into(), Value::from(type_name(family, integers)));
        out.insert("enum".into(), Value::Array(values));
        return Value::Object(out);
    }
    match family {
        Family::Object => objects(&schemas),
        Family::Array => arrays(&schemas),
        Family::String => scalars(
            &schemas,
            family,
            &[("minLength", Bound::Least), ("maxLength", Bound::Most)],
        ),
        Family::Number => scalars(
            &schemas,
            family,
            &[
                ("minimum", Bound::Least),
                ("maximum", Bound::Most),
                ("exclusiveMinimum", Bound::Least),
                ("exclusiveMaximum", Bound::Most),
            ],
        ),
        Family::Boolean | Family::Null => scalars(&schemas, family, &[]),
    }
}

fn type_name(family: Family, integers: bool) -> &'static str {
    if family == Family::Number && integers {
        "integer"
    } else {
        family.name()
    }
}

fn all_integer(schemas: &[Value]) -> bool {
    schemas
        .iter()
        .all(|schema| schema.get("type").and_then(Value::as_str) == Some("integer"))
}

fn enumerated(schemas: &[Value]) -> Option<Vec<Value>> {
    let mut values: Vec<Value> = Vec::new();
    for schema in schemas {
        let listed = match (schema.get("const"), schema.get("enum")) {
            (Some(constant), _) => vec![constant.clone()],
            (None, Some(Value::Array(listed))) => listed.clone(),
            _ => return None,
        };
        for value in listed {
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
    Some(values)
}

fn loosest(schemas: &[Value], key: &str, bound: Bound) -> Option<Value> {
    let mut best: Option<(f64, &Value)> = None;
    for schema in schemas {
        let value = schema.get(key)?;
        let number = value.as_f64()?;
        let wider = match (best, bound) {
            (None, _) => true,
            (Some((current, _)), Bound::Least) => number < current,
            (Some((current, _)), Bound::Most) => number > current,
        };
        if wider {
            best = Some((number, value));
        }
    }
    best.map(|(_, value)| value.clone())
}

fn shared(schemas: &[Value], handled: &[&str]) -> Vec<(String, Value)> {
    let Some(Value::Object(first)) = schemas.first() else {
        return Vec::new();
    };
    first
        .iter()
        .filter(|(key, _)| !handled.contains(&key.as_str()))
        .filter(|(key, value)| {
            schemas
                .iter()
                .all(|schema| schema.get(key.as_str()) == Some(*value))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn with_bounds(out: &mut Map<String, Value>, schemas: &[Value], bounds: &[(&str, Bound)]) {
    for (key, bound) in bounds {
        if let Some(value) = loosest(schemas, key, *bound) {
            out.insert((*key).to_string(), value);
        }
    }
}

fn scalars(schemas: &[Value], family: Family, bounds: &[(&str, Bound)]) -> Value {
    let mut out = Map::new();
    out.insert(
        "type".into(),
        Value::from(type_name(family, all_integer(schemas))),
    );
    with_bounds(&mut out, schemas, bounds);
    let mut handled = vec!["type", "enum", "const"];
    handled.extend(bounds.iter().map(|(key, _)| *key));
    out.extend(shared(schemas, &handled));
    Value::Object(out)
}

fn properties(schema: &Value) -> Option<&Map<String, Value>> {
    schema.get("properties").and_then(Value::as_object)
}

fn extra(schema: &Value) -> Option<Value> {
    match schema.get("additionalProperties") {
        Some(Value::Bool(false)) => None,
        Some(other) => Some(other.clone()),
        None => Some(Value::Bool(true)),
    }
}

fn objects(schemas: &[Value]) -> Value {
    let patterned = schemas
        .iter()
        .any(|schema| schema.get("patternProperties").is_some());
    let mut names: Vec<&String> = Vec::new();
    for name in schemas.iter().filter_map(properties).flat_map(Map::keys) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let mut merged = Map::new();
    for name in names {
        let parts: Vec<Value> = schemas
            .iter()
            .filter_map(
                |schema| match properties(schema).and_then(|found| found.get(name)) {
                    Some(property) => Some(property.clone()),
                    None if patterned => Some(Value::Bool(true)),
                    None => extra(schema),
                },
            )
            .collect();
        merged.insert(name.clone(), join(parts));
    }
    let required: Vec<Value> = schemas
        .first()
        .and_then(|schema| schema.get("required"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|name| {
            schemas.iter().all(|schema| {
                schema
                    .get("required")
                    .and_then(Value::as_array)
                    .is_some_and(|listed| listed.contains(name))
            })
        })
        .cloned()
        .collect();
    let mut out = Map::new();
    out.insert("type".into(), Value::from("object"));
    if !merged.is_empty() {
        out.insert("properties".into(), Value::Object(merged));
    }
    if !required.is_empty() {
        out.insert("required".into(), Value::Array(required));
    }
    if !patterned {
        let extras: Vec<Value> = schemas.iter().filter_map(extra).collect();
        if extras.is_empty() {
            out.insert("additionalProperties".into(), Value::Bool(false));
        } else {
            let joined = join(extras);
            if joined != Value::Object(Map::new()) {
                out.insert("additionalProperties".into(), joined);
            }
        }
    }
    with_bounds(
        &mut out,
        schemas,
        &[
            ("minProperties", Bound::Least),
            ("maxProperties", Bound::Most),
        ],
    );
    out.extend(shared(
        schemas,
        &[
            "type",
            "properties",
            "required",
            "additionalProperties",
            "patternProperties",
            "minProperties",
            "maxProperties",
            "enum",
            "const",
        ],
    ));
    Value::Object(out)
}

fn arrays(schemas: &[Value]) -> Value {
    let positional = schemas.iter().any(|schema| {
        schema.get("prefixItems").is_some() || schema.get("items").is_some_and(Value::is_array)
    });
    let mut out = Map::new();
    out.insert("type".into(), Value::from("array"));
    if !positional {
        let items = join(
            schemas
                .iter()
                .map(|schema| schema.get("items").cloned().unwrap_or(Value::Bool(true)))
                .collect(),
        );
        if items != Value::Object(Map::new()) {
            out.insert("items".into(), items);
        }
    }
    with_bounds(
        &mut out,
        schemas,
        &[("minItems", Bound::Least), ("maxItems", Bound::Most)],
    );
    let mut handled = vec!["type", "minItems", "maxItems", "enum", "const"];
    if !positional {
        handled.extend(POSITIONAL);
    }
    out.extend(shared(schemas, &handled));
    Value::Object(out)
}

pub fn same_type_unions(schema: &Value) -> Vec<String> {
    let mut found = Vec::new();
    find_same_type_unions(schema, "#", &mut found);
    found
}

fn find_same_type_unions(schema: &Value, path: &str, found: &mut Vec<String>) {
    let Value::Object(map) = schema else {
        return;
    };
    for (key, value) in map {
        let keyword = key.as_str();
        let here = format!("{path}/{key}");
        if UNIONS.contains(&keyword)
            && let Some(branches) = value.as_array()
            && overlapping(branches)
        {
            found.push(here.clone());
        }
        if keyword == "type"
            && let Some(names) = value.as_array()
            && overlapping(
                &names
                    .iter()
                    .map(|name| {
                        let mut single = Map::new();
                        single.insert("type".into(), name.clone());
                        Value::Object(single)
                    })
                    .collect::<Vec<_>>(),
            )
        {
            found.push(here.clone());
        }
        if SCHEMA_LISTS.contains(&keyword) {
            for (index, item) in value.as_array().into_iter().flatten().enumerate() {
                find_same_type_unions(item, &format!("{here}/{index}"), found);
            }
        } else if SCHEMA_MAPS.contains(&keyword) {
            for (name, item) in value.as_object().into_iter().flatten() {
                find_same_type_unions(item, &format!("{here}/{name}"), found);
            }
        } else if SCHEMA_VALUES.contains(&keyword) {
            find_same_type_unions(value, &here, found);
        }
    }
}

fn families(schema: &Value) -> Option<Vec<Family>> {
    let map = match schema {
        Value::Bool(true) => return None,
        Value::Bool(false) => return Some(Vec::new()),
        Value::Object(map) => map,
        _ => return None,
    };
    let mut found = match (map.get("type"), map.get("const"), map.get("enum")) {
        (Some(Value::String(name)), _, _) => vec![Family::of_name(name)?],
        (Some(Value::Array(names)), _, _) => names
            .iter()
            .map(|name| name.as_str().and_then(Family::of_name))
            .collect::<Option<Vec<_>>>()?,
        (Some(_), _, _) => return None,
        (None, Some(constant), _) => vec![Family::of_value(constant)],
        (None, None, Some(Value::Array(values))) => values.iter().map(Family::of_value).collect(),
        (None, None, _) => match map.get("anyOf") {
            Some(Value::Array(branches)) if map.len() == 1 => branches
                .iter()
                .map(families)
                .collect::<Option<Vec<_>>>()?
                .concat(),
            _ => return None,
        },
    };
    found.dedup();
    Some(found)
}

fn overlapping(branches: &[Value]) -> bool {
    let mut seen: Vec<Family> = Vec::new();
    for branch in branches {
        let Some(mut list) = families(branch) else {
            return branches.len() > 1;
        };
        list.sort_by_key(|family| family.name());
        list.dedup();
        if list.iter().any(|family| seen.contains(family)) {
            return true;
        }
        seen.extend(list);
    }
    false
}
