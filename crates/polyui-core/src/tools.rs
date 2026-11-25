use crate::catalog::{CatalogSet, ChildRule, PropKind, PropSpec, TypeDef};
use crate::limits::Limits;
use crate::validate::{ActionRegistry, MAX_ID_CHARS};
use serde_json::{Map, Value, json};

mod compact;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToolProfile {
    #[default]
    Full,
    Compact,
}

impl ToolProfile {
    pub fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Compact => "compact",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "full" => Some(Self::Full),
            "compact" => Some(Self::Compact),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToolOptions {
    pub strict: bool,
    pub profile: ToolProfile,
}

const SHOW_UI_DESCRIPTION: &str = "Show UI to the user as a flat list of components. Children are referenced by id. A prop that is not an action or a path can also be {\"$bind\": \"/path\"} to read the data model or {\"$ref\": \"source:key\"} for a host value. Returns a summary of what is on screen, or a numbered list of problems to fix and retry.";
const UPDATE_UI_DESCRIPTION: &str = "Change a surface that is open: add or replace components, remove ids, change the root, or set data values.";
const CLOSE_UI_DESCRIPTION: &str = "Close a surface.";

struct Builder<'a> {
    actions: &'a dyn ActionRegistry,
    strict: bool,
    limits: Limits,
}

const STRICT_NOTE: &str =
    " Every field must be present: write null for a field you do not set and [] for an empty list.";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StrictCost {
    pub tools: usize,
    pub optional: usize,
    pub unions: usize,
}

pub const STRICT_LIMITS: StrictCost = StrictCost {
    tools: 20,
    optional: 24,
    unions: 16,
};

impl StrictCost {
    pub fn of_tool(tool: &Value) -> Self {
        let mut cost = Self {
            tools: 1,
            ..Self::default()
        };
        if let Some(schema) = tool.get("input_schema") {
            cost.visit(schema);
        }
        cost
    }

    fn visit(&mut self, schema: &Value) {
        let Value::Object(map) = schema else {
            return;
        };
        if map.contains_key("anyOf") || map.get("type").is_some_and(Value::is_array) {
            self.unions += 1;
        }
        if let Some(Value::Object(properties)) = map.get("properties") {
            let required: Vec<&str> = map
                .get("required")
                .and_then(Value::as_array)
                .map(|names| names.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            self.optional += properties
                .keys()
                .filter(|name| !required.contains(&name.as_str()))
                .count();
            properties
                .values()
                .for_each(|property| self.visit(property));
        }
        if let Some(items) = map.get("items") {
            self.visit(items);
        }
        for key in ["anyOf", "allOf"] {
            if let Some(Value::Array(branches)) = map.get(key) {
                branches.iter().for_each(|branch| self.visit(branch));
            }
        }
        for key in ["$defs", "definitions"] {
            if let Some(Value::Object(definitions)) = map.get(key) {
                definitions
                    .values()
                    .for_each(|definition| self.visit(definition));
            }
        }
    }

    pub fn plus(self, other: Self) -> Self {
        Self {
            tools: self.tools + other.tools,
            optional: self.optional + other.optional,
            unions: self.unions + other.unions,
        }
    }

    pub fn within(self, limits: Self) -> bool {
        self.tools <= limits.tools
            && self.optional <= limits.optional
            && self.unions <= limits.unions
    }
}

fn is_strict(tool: &Value) -> bool {
    tool.get("strict") == Some(&Value::Bool(true))
}

pub fn limit_strict(tools: &mut [Value]) {
    let mut total = StrictCost::default();
    for tool in tools.iter_mut() {
        if !is_strict(tool) {
            continue;
        }
        let next = total.plus(StrictCost::of_tool(tool));
        if next.within(STRICT_LIMITS) {
            total = next;
        } else if let Value::Object(map) = tool {
            map.remove("strict");
        }
    }
}

fn nullable(schema: Value) -> Value {
    let Value::Object(mut map) = schema else {
        return schema;
    };
    if let Some(Value::Array(values)) = map.get_mut("enum") {
        values.push(Value::Null);
        map.remove("type");
        return Value::Object(map);
    }
    if let Some(Value::Array(branches)) = map.get_mut("anyOf") {
        branches.push(json!({ "type": "null" }));
        return Value::Object(map);
    }
    if map.get("type") == Some(&Value::from("boolean")) {
        map.remove("type");
        map.insert("enum".into(), json!([true, false, null]));
        return Value::Object(map);
    }
    let description = map.remove("description");
    let mut wrapped = Map::new();
    wrapped.insert(
        "anyOf".into(),
        Value::Array(vec![Value::Object(map), json!({ "type": "null" })]),
    );
    if let Some(description) = description {
        wrapped.insert("description".into(), description);
    }
    Value::Object(wrapped)
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

fn single_key_schema(key: &str) -> Value {
    json!({
        "type": "object",
        "properties": { key: { "type": "string" } },
        "required": [key],
        "additionalProperties": false
    })
}

impl Builder<'_> {
    fn object(
        &self,
        properties: Map<String, Value>,
        required: Vec<String>,
        lists: &[&str],
    ) -> Value {
        if !self.strict {
            return closed_object(properties, required);
        }
        let mut all = Map::new();
        for (name, schema) in properties {
            let schema = if required.contains(&name) || lists.contains(&name.as_str()) {
                schema
            } else {
                nullable(schema)
            };
            all.insert(name, schema);
        }
        let names = all.keys().cloned().collect();
        closed_object(all, names)
    }

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
                self.object(properties, required, &[]),
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
        let mut with = json!({ "type": "array", "items": { "type": "string" }, "description": "Data paths whose values are sent when the action fires. [] for none." });
        self.limit(
            &mut with,
            "maxItems",
            Some(Value::from(crate::validate::MAX_WITH_PATHS)),
        );
        let mut properties = Map::new();
        properties.insert("action".into(), json!({ "type": "string", "enum": names }));
        properties.insert("args".into(), args);
        properties.insert("with".into(), with);
        let required = if self.strict {
            vec!["action".into(), "args".into()]
        } else {
            vec!["action".into()]
        };
        Some(self.object(properties, required, &["with"]))
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
        Some(self.object(properties, required, &[]))
    }

    fn prop_schema(&self, spec: &PropSpec) -> Option<Value> {
        let literal = self.literal_schema(spec)?;
        let schema = match (spec.bindable, self.strict) {
            (false, _) => literal,
            (true, false) => json!({ "anyOf": [ literal, binding_schema() ] }),
            (true, true) => json!({ "anyOf": [
                literal,
                single_key_schema("$bind"),
                single_key_schema("$ref")
            ] }),
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
        if self.strict || definition.props.iter().any(|(_, spec)| spec.required) {
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
            self.object(properties, required, &["children"]),
            Some(&description),
        ))
    }

    fn components_schema(&self, definitions: &[&TypeDef]) -> Value {
        let branches: Vec<Value> = definitions
            .iter()
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
        let description = if self.strict {
            format!("{description}{STRICT_NOTE}")
        } else {
            description.to_string()
        };
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

fn offered<'a>(catalogs: &'a CatalogSet, types: &[String]) -> Vec<&'a TypeDef> {
    catalogs
        .types()
        .filter(|definition| types.is_empty() || types.contains(&definition.qualified))
        .collect()
}

pub fn unknown_types(catalogs: &CatalogSet, types: &[String]) -> Vec<String> {
    types
        .iter()
        .filter(|name| catalogs.get(name).is_none())
        .cloned()
        .collect()
}

pub fn show_ui_tool(
    catalogs: &CatalogSet,
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    show_ui(&offered(catalogs, &[]), actions, options)
}

fn show_ui(definitions: &[&TypeDef], actions: &dyn ActionRegistry, options: ToolOptions) -> Value {
    if options.profile == ToolProfile::Compact {
        return compact::show_ui_tool(definitions, actions);
    }
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
    properties.insert("components".into(), builder.components_schema(definitions));
    properties.insert("data".into(), builder.data_schema());
    builder.tool(
        "show_ui",
        SHOW_UI_DESCRIPTION,
        builder.object(
            properties,
            vec!["root".into(), "components".into()],
            &["data"],
        ),
    )
}

pub fn update_ui_tool(
    catalogs: &CatalogSet,
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    update_ui(&offered(catalogs, &[]), actions, options)
}

fn update_ui(
    definitions: &[&TypeDef],
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    if options.profile == ToolProfile::Compact {
        return compact::update_ui_tool(definitions, actions);
    }
    let builder = builder(actions, options);
    let mut properties = Map::new();
    properties.insert(
        "surface".into(),
        builder.surface_schema("Id of the open surface."),
    );
    properties.insert("upsert".into(), builder.components_schema(definitions));
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
        builder.object(
            properties,
            vec!["surface".into()],
            &["upsert", "remove", "data"],
        ),
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
        builder.object(properties, vec!["surface".into()], &[]),
    )
}

fn every_tool(
    definitions: &[&TypeDef],
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Vec<Value> {
    vec![
        show_ui(definitions, actions, options),
        update_ui(definitions, actions, options),
        close_ui_tool(actions, options),
    ]
}

pub fn strict_tool_definitions(catalogs: &CatalogSet, actions: &dyn ActionRegistry) -> Value {
    Value::Array(every_tool(
        &offered(catalogs, &[]),
        actions,
        ToolOptions {
            strict: true,
            profile: ToolProfile::Full,
        },
    ))
}

pub fn tool_definitions(
    catalogs: &CatalogSet,
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    definitions(&offered(catalogs, &[]), actions, options)
}

fn definitions(
    definitions: &[&TypeDef],
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    let loose = every_tool(
        definitions,
        actions,
        ToolOptions {
            strict: false,
            ..options
        },
    );
    if !options.strict {
        return Value::Array(loose);
    }
    let strict = every_tool(definitions, actions, options);
    let mut order: Vec<usize> = (0..strict.len())
        .filter(|&index| is_strict(&strict[index]))
        .collect();
    order.sort_by_key(|&index| {
        let cost = StrictCost::of_tool(&strict[index]);
        (cost.unions + cost.optional, index)
    });
    let mut total = StrictCost::default();
    let mut chosen = loose;
    for index in order {
        let next = total.plus(StrictCost::of_tool(&strict[index]));
        if next.within(STRICT_LIMITS) {
            total = next;
            chosen[index] = strict[index].clone();
        }
    }
    Value::Array(chosen)
}

pub fn for_client(
    catalogs: &CatalogSet,
    declared: &[String],
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    tool_definitions(&catalogs.restrict(declared), actions, options)
}

pub fn for_client_types(
    catalogs: &CatalogSet,
    declared: &[String],
    types: &[String],
    actions: &dyn ActionRegistry,
    options: ToolOptions,
) -> Value {
    let restricted = catalogs.restrict(declared);
    definitions(&offered(&restricted, types), actions, options)
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
        let loose = builder(&actions, ToolOptions::default());
        let strict = builder(
            &actions,
            ToolOptions {
                strict: true,
                ..ToolOptions::default()
            },
        );
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
        let close = close_ui_tool(
            &actions,
            ToolOptions {
                strict: true,
                ..ToolOptions::default()
            },
        );
        assert_eq!(close["strict"], json!(true));
        assert_eq!(close["input_schema"]["required"], json!(["surface"]));
    }
}
