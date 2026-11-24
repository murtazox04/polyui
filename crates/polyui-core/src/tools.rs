use crate::catalog::{CatalogSet, ChildRule, PropKind, PropSpec, TypeDef};
use crate::limits::Limits;
use crate::validate::{ActionRegistry, MAX_ID_CHARS};
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToolOptions {
    pub strict: bool,
}

const SHOW_UI_DESCRIPTION: &str = "Show UI to the user as a flat list of components. Children are referenced by id. A prop that is not an action or a path can also be {\"$bind\": \"/path\"} to read the data model or {\"$ref\": \"source:key\"} for a host value. Returns a summary of what is on screen, or a numbered list of problems to fix and retry.";
const UPDATE_UI_DESCRIPTION: &str = "Change a surface that is open: add or replace components, remove ids, change the root, or set data values.";
const CLOSE_UI_DESCRIPTION: &str = "Close a surface.";

struct Builder<'a> {
    actions: &'a dyn ActionRegistry,
    strict: bool,
    limits: Limits,
}

fn closed_object(properties: Map<String, Value>, required: Vec<String>) -> Value {
    let mut schema = Map::new();
    schema.insert("type".into(), Value::from("object"));
    schema.insert("properties".into(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".into(), Value::from(required));
    }
    schema.insert("additionalProperties".into(), Value::Bool(false));
    Value::Object(schema)
}

fn describe(mut schema: Value, description: Option<&str>) -> Value {
    if let (Some(text), Value::Object(map)) = (description, &mut schema)
        && !text.is_empty()
    {
        map.insert("description".into(), Value::from(text));
    }
    schema
}

fn scalar_schema() -> Value {
    json!({ "anyOf": [ { "type": "string" }, { "type": "number" }, { "type": "boolean" } ] })
}

fn binding_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "$bind": { "type": "string" },
            "$ref": { "type": "string" }
        },
        "additionalProperties": false
    })
}

impl Builder<'_> {
    fn limit(&self, schema: &mut Value, key: &str, value: Option<Value>) {
        if self.strict {
            return;
        }
        if let (Some(value), Value::Object(map)) = (value, schema) {
            map.insert(key.into(), value);
        }
    }

    fn action_names(&self) -> Vec<&str> {
        self.actions.names()
    }

    fn action_schema(&self) -> Option<Value> {
        let names = self.action_names();
        if names.is_empty() {
            return None;
        }
        let mut branches: Vec<Value> = Vec::new();
        for name in &names {
            let Some(spec) = self.actions.action(name) else {
                continue;
            };
            let mut properties = Map::new();
            let mut required = Vec::new();
            for (arg, arg_spec) in &spec.args {
                if let Some(schema) = self.literal_schema(arg_spec) {
                    properties.insert(
                        arg.clone(),
                        describe(schema, arg_spec.description.as_deref()),
                    );
                    if arg_spec.required {
                        required.push(arg.clone());
                    }
                }
            }
            let branch = describe(
                closed_object(properties, required),
                Some(&format!("Arguments for {name}.")),
            );
            if !branches.contains(&branch) {
                branches.push(branch);
            }
        }
        let args = if branches.len() == 1 {
            branches.remove(0)
        } else {
            json!({ "anyOf": branches })
        };
        let mut with = json!({ "type": "array", "items": { "type": "string" }, "description": "Data paths whose values are sent when the action fires." });
        self.limit(
            &mut with,
            "maxItems",
            Some(Value::from(crate::validate::MAX_WITH_PATHS)),
        );
        let mut properties = Map::new();
        properties.insert("action".into(), json!({ "type": "string", "enum": names }));
        properties.insert("args".into(), args);
        properties.insert("with".into(), with);
        Some(closed_object(properties, vec!["action".into()]))
    }

    fn literal_schema(&self, spec: &PropSpec) -> Option<Value> {
        let schema = match &spec.kind {
            PropKind::String { min, max } => {
                let mut schema = json!({ "type": "string" });
                let max = max.unwrap_or(self.limits.max_string_chars as u64);
                self.limit(&mut schema, "maxLength", Some(Value::from(max)));
                self.limit(&mut schema, "minLength", min.map(Value::from));
                schema
            }
            PropKind::Int { min, max } => {
                let mut schema = json!({ "type": "integer" });
                self.limit(&mut schema, "minimum", min.map(Value::from));
                self.limit(&mut schema, "maximum", max.map(Value::from));
                schema
            }
            PropKind::Number { min, max } => {
                let mut schema = json!({ "type": "number" });
                self.limit(&mut schema, "minimum", min.map(Value::from));
                self.limit(&mut schema, "maximum", max.map(Value::from));
                schema
            }
            PropKind::Bool => json!({ "type": "boolean" }),
            PropKind::Scalar => scalar_schema(),
            PropKind::Enum { values } => json!({ "type": "string", "enum": values }),
            PropKind::Url { schemes } => json!({
                "type": "string",
                "description": format!("Absolute URL ({}).", schemes.join(", "))
            }),
            PropKind::Action => self.action_schema()?,
            PropKind::Path => {
                json!({ "type": "string", "description": "JSON Pointer into the data model, like /form/name." })
            }
            PropKind::Array { of, max } => {
                let mut schema = json!({ "type": "array", "items": self.prop_schema(of)? });
                let max = max.unwrap_or(self.limits.max_array_len as u64);
                self.limit(&mut schema, "maxItems", Some(Value::from(max)));
                schema
            }
            PropKind::Object { fields } => self.fields_schema(fields)?,
        };
        Some(schema)
    }

    fn fields_schema(&self, fields: &[(String, PropSpec)]) -> Option<Value> {
        let mut properties = Map::new();
        let mut required = Vec::new();
        for (name, spec) in fields {
            match self.prop_schema(spec) {
                Some(schema) => {
                    properties.insert(name.clone(), schema);
                    if spec.required {
                        required.push(name.clone());
                    }
                }
                None if spec.required => return None,
                None => {}
            }
        }
        Some(closed_object(properties, required))
    }

    fn prop_schema(&self, spec: &PropSpec) -> Option<Value> {
        let literal = self.literal_schema(spec)?;
        let schema = if spec.bindable {
            json!({ "anyOf": [ literal, binding_schema() ] })
        } else {
            literal
        };
        Some(describe(schema, spec.description.as_deref()))
    }

    fn component_schema(&self, definition: &TypeDef) -> Option<Value> {
        let props = self.fields_schema(&definition.props)?;
        let mut id =
            json!({ "type": "string", "description": "Unique id: letters, digits, _ and -." });
        self.limit(&mut id, "minLength", Some(Value::from(1)));
        self.limit(&mut id, "maxLength", Some(Value::from(MAX_ID_CHARS)));
        let mut properties = Map::new();
        properties.insert("id".into(), id);
        properties.insert(
            "type".into(),
            json!({ "type": "string", "enum": [definition.qualified] }),
        );
        properties.insert("props".into(), props);
        let mut required = vec!["id".to_string(), "type".to_string()];
        if definition.props.iter().any(|(_, spec)| spec.required) {
            required.push("props".into());
        }
        let children_description = match &definition.children {
            ChildRule::None => None,
            ChildRule::Any => Some("Ids of child components, in order.".to_string()),
            ChildRule::Only(names) => Some(format!(
                "Ids of child components, in order. Allowed types: {}.",
                names.join(", ")
            )),
        };
        if let Some(text) = children_description {
            let mut children =
                json!({ "type": "array", "items": { "type": "string" }, "description": text });
            self.limit(
                &mut children,
                "maxItems",
                Some(Value::from(self.limits.max_components)),
            );
            properties.insert("children".into(), children);
        }
        let description = format!("{}: {}", definition.qualified, definition.description);
        Some(describe(
            closed_object(properties, required),
            Some(&description),
        ))
    }

    fn components_schema(&self, catalogs: &CatalogSet) -> Value {
        let branches: Vec<Value> = catalogs
            .types()
            .filter_map(|definition| self.component_schema(definition))
            .collect();
        let mut schema = json!({ "type": "array", "items": { "anyOf": branches } });
        self.limit(
            &mut schema,
            "maxItems",
            Some(Value::from(self.limits.max_components)),
        );
        schema
    }

    fn data_schema(&self) -> Value {
        let mut properties = Map::new();
        properties.insert(
            "path".into(),
            json!({ "type": "string", "description": "JSON Pointer, like /bills/0/amount." }),
        );
        properties.insert("value".into(), scalar_schema());
        let mut schema = json!({
            "type": "array",
            "description": "Values for the data model. Each entry sets one value at a path; missing objects are created.",
            "items": closed_object(properties, vec!["path".into(), "value".into()])
        });
        self.limit(
            &mut schema,
            "maxItems",
            Some(Value::from(self.limits.max_array_len)),
        );
        schema
    }

    fn surface_schema(&self, description: &str) -> Value {
        let mut schema = json!({ "type": "string", "description": description });
        self.limit(&mut schema, "maxLength", Some(Value::from(MAX_ID_CHARS)));
        schema
    }

    fn tool(&self, name: &str, description: &str, input_schema: Value) -> Value {
        let mut tool = Map::new();
        tool.insert("name".into(), Value::from(name));
        tool.insert("description".into(), Value::from(description));
        tool.insert("input_schema".into(), input_schema);
        if self.strict {
            tool.insert("strict".into(), Value::Bool(true));
        }
        Value::Object(tool)
    }
}

fn builder(actions: &dyn ActionRegistry, options: ToolOptions) -> Builder<'_> {
    Builder {
        actions,
        strict: options.strict,
        limits: Limits::default(),
    }
}

pub fn show_ui_tool(
    catalogs: &CatalogSet,
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    let builder = builder(actions, options);
    let mut properties = Map::new();
    properties.insert(
        "surface".into(),
        builder.surface_schema("Optional id for the new surface."),
    );
    properties.insert(
        "root".into(),
        json!({ "type": "string", "description": "Id of the top component." }),
    );
    properties.insert("components".into(), builder.components_schema(catalogs));
    properties.insert("data".into(), builder.data_schema());
    builder.tool(
        "show_ui",
        SHOW_UI_DESCRIPTION,
        closed_object(properties, vec!["root".into(), "components".into()]),
    )
}

pub fn update_ui_tool(
    catalogs: &CatalogSet,
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    let builder = builder(actions, options);
    let mut properties = Map::new();
    properties.insert(
        "surface".into(),
        builder.surface_schema("Id of the open surface."),
    );
    properties.insert("upsert".into(), builder.components_schema(catalogs));
    properties.insert(
        "remove".into(),
        json!({ "type": "array", "items": { "type": "string" }, "description": "Ids of components to remove." }),
    );
    properties.insert(
        "root".into(),
        json!({ "type": "string", "description": "New top component id." }),
    );
    properties.insert("data".into(), builder.data_schema());
    builder.tool(
        "update_ui",
        UPDATE_UI_DESCRIPTION,
        closed_object(properties, vec!["surface".into()]),
    )
}

pub fn close_ui_tool(actions: &dyn ActionRegistry, options: ToolOptions) -> Value {
    let builder = builder(actions, options);
    let mut properties = Map::new();
    properties.insert(
        "surface".into(),
        builder.surface_schema("Id of the surface to close."),
    );
    builder.tool(
        "close_ui",
        CLOSE_UI_DESCRIPTION,
        closed_object(properties, vec!["surface".into()]),
    )
}

pub fn tool_definitions(
    catalogs: &CatalogSet,
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    Value::Array(vec![
        show_ui_tool(catalogs, actions, options),
        update_ui_tool(catalogs, actions, options),
        close_ui_tool(actions, options),
    ])
}

pub fn for_client(
    catalogs: &CatalogSet,
    declared: &[String],
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    tool_definitions(&catalogs.restrict(declared), actions, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::ActionTable;

    #[test]
    fn objects_are_closed_and_strict_drops_limits() {
        let object = closed_object(Map::new(), Vec::new());
        assert_eq!(
            object,
            json!({ "type": "object", "properties": {}, "additionalProperties": false })
        );
        let actions = ActionTable::new();
        let loose = builder(&actions, ToolOptions { strict: false });
        let strict = builder(&actions, ToolOptions { strict: true });
        let spec = PropSpec::from_value(&json!({ "kind": "int", "min": 1, "max": 3 })).unwrap();
        assert_eq!(
            loose.literal_schema(&spec),
            Some(json!({ "type": "integer", "minimum": 1, "maximum": 3 }))
        );
        assert_eq!(
            strict.literal_schema(&spec),
            Some(json!({ "type": "integer" }))
        );
        let action = PropSpec::from_value(&json!({ "kind": "action" })).unwrap();
        assert_eq!(strict.prop_schema(&action), None);
        let close = close_ui_tool(&actions, ToolOptions { strict: true });
        assert_eq!(close["strict"], json!(true));
        assert_eq!(close["input_schema"]["required"], json!(["surface"]));
    }
}
