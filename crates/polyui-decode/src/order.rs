use serde_json::{Map, Value};

pub fn schema_order(value: &Value, schema: &Value) -> Value {
    let Some(schema) = branch_for(value, schema) else {
        return value.clone();
    };
    match value {
        Value::Object(fields) => {
            let properties = schema.get("properties").and_then(Value::as_object);
            let mut ordered = Map::new();
            for (name, property) in properties.into_iter().flatten() {
                if let Some(field) = fields.get(name) {
                    ordered.insert(name.clone(), schema_order(field, property));
                }
            }
            for (name, field) in fields {
                if !ordered.contains_key(name) {
                    ordered.insert(name.clone(), field.clone());
                }
            }
            Value::Object(ordered)
        }
        Value::Array(items) => match schema.get("items") {
            Some(item_schema) => Value::Array(
                items
                    .iter()
                    .map(|item| schema_order(item, item_schema))
                    .collect(),
            ),
            None => value.clone(),
        },
        _ => value.clone(),
    }
}

fn branch_for<'a>(value: &Value, schema: &'a Value) -> Option<&'a Value> {
    match schema.get("anyOf").and_then(Value::as_array) {
        Some(branches) => branches
            .iter()
            .find(|branch| fits(value, branch))
            .or_else(|| branches.iter().find(|branch| same_shape(value, branch)))
            .and_then(|branch| branch_for(value, branch)),
        None => Some(schema),
    }
}

fn same_shape(value: &Value, schema: &Value) -> bool {
    if let Some(branches) = schema.get("anyOf").and_then(Value::as_array) {
        return branches.iter().any(|branch| same_shape(value, branch));
    }
    if let Some(kind) = schema.get("type")
        && !type_fits(value, kind)
    {
        return false;
    }
    let (Value::Object(fields), Some(properties)) =
        (value, schema.get("properties").and_then(Value::as_object))
    else {
        return true;
    };
    properties.iter().all(|(name, property)| {
        let fixed = property.get("const").or_else(|| {
            property
                .get("enum")
                .and_then(Value::as_array)
                .filter(|values| values.len() == 1)
                .and_then(|values| values.first())
        });
        match (fixed, fields.get(name)) {
            (Some(fixed), Some(field)) => fixed == field,
            _ => true,
        }
    })
}

fn fits(value: &Value, schema: &Value) -> bool {
    if let Some(branches) = schema.get("anyOf").and_then(Value::as_array) {
        return branches.iter().any(|branch| fits(value, branch));
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        return false;
    }
    if let Some(constant) = schema.get("const")
        && constant != value
    {
        return false;
    }
    if let Some(kind) = schema.get("type")
        && !type_fits(value, kind)
    {
        return false;
    }
    match value {
        Value::Object(fields) => {
            let properties = schema.get("properties").and_then(Value::as_object);
            fields.iter().all(|(name, field)| {
                match properties.and_then(|properties| properties.get(name)) {
                    Some(property) => fits(field, property),
                    None => schema.get("additionalProperties") != Some(&Value::Bool(false)),
                }
            })
        }
        Value::Array(items) => match schema.get("items") {
            Some(item_schema) => items.iter().all(|item| fits(item, item_schema)),
            None => true,
        },
        _ => true,
    }
}

fn type_fits(value: &Value, kind: &Value) -> bool {
    match kind {
        Value::String(name) => single_type_fits(value, name),
        Value::Array(names) => names
            .iter()
            .filter_map(Value::as_str)
            .any(|name| single_type_fits(value, name)),
        _ => true,
    }
}

fn single_type_fits(value: &Value, name: &str) -> bool {
    match name {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    }
}
