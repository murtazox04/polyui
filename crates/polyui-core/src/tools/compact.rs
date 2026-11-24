use crate::catalog::{ChildRule, PropKind, PropSpec, TypeDef};
use crate::validate::ActionRegistry;
use serde_json::{Value, json};
use std::fmt::Display;

const SHOW_UI: &str = "Show UI: a flat list of components, each {id, type, props, children}. root is the id of the top component. A parent lists the ids of its child components in its own children, next to props and never inside props, and every component must be reachable from root that way. Returns what is on screen, or numbered problems to fix and retry.";
const UPDATE_UI: &str = "Change an open surface: upsert adds or replaces components by id, remove drops ids, root sets the top id, data sets values. Types as in show_ui.";
const LEGEND: &str = "Types: Type(prop, ...). ! required. A bare name is text, name<=N text of at most N chars. a|b one of. [X] list. path: data pointer like /form/name. [children]: the type may have children.";
const BINDING: &str = "A prop that is not an action or path may be {\"$bind\":\"/path\"} (data model) or {\"$ref\":\"source:key\"} (host value).";
const ACTION: &str = "Action: {\"action\":\"name\",\"args\":{...},\"with\":[\"/path\"]}, with lists paths sent when it fires. Actions:";
const NOT_BINDABLE: &str = " (no $bind or $ref)";
const SHOWN_LIMIT: u64 = 100;

pub struct TypeLine {
    pub qualified: String,
    pub line: String,
}

struct Signatures<'a> {
    actions: &'a dyn ActionRegistry,
}

fn bounds(min: Option<impl Display>, max: Option<impl Display>) -> String {
    match (min, max) {
        (None, None) => String::new(),
        (None, Some(max)) => format!("<={max}"),
        (Some(min), None) => format!(">={min}"),
        (Some(min), Some(max)) => format!(" {min}..{max}"),
    }
}

fn shown(limit: Option<u64>) -> Option<u64> {
    limit.filter(|value| *value <= SHOWN_LIMIT)
}

impl Signatures<'_> {
    fn kind(&self, spec: &PropSpec) -> Option<String> {
        let text = match &spec.kind {
            PropKind::String { min, max } => format!(":text{}", bounds(*min, shown(*max))),
            PropKind::Int { min, max } => format!(": int{}", bounds(*min, *max)),
            PropKind::Number { min, max } => format!(": number{}", bounds(*min, *max)),
            PropKind::Bool => ": bool".to_string(),
            PropKind::Scalar => ": text|number|bool".to_string(),
            PropKind::Enum { values } => format!(": {}", values.join("|")),
            PropKind::Url { schemes } => format!(": {} url", schemes.join("|")),
            PropKind::Action if self.actions.names().is_empty() => return None,
            PropKind::Action => ": action".to_string(),
            PropKind::Path => ": path".to_string(),
            PropKind::Array { of, max } => {
                let item = self.kind(of)?;
                let item = item.strip_prefix(':').unwrap_or(&item).trim_start();
                format!(": [{item}]{}", bounds(None::<u64>, shown(*max)))
            }
            PropKind::Object { fields } => format!(": {{{}}}", self.fields(fields, true)?),
        };
        Some(text)
    }

    fn field(&self, name: &str, spec: &PropSpec, bindable_marks: bool) -> Option<String> {
        let kind = self.kind(spec)?;
        let kind = match kind.strip_prefix(":text") {
            Some(limit) => limit.to_string(),
            None => kind,
        };
        let required = if spec.required { "!" } else { "" };
        let fixed = matches!(spec.kind, PropKind::Action | PropKind::Path);
        let note = if bindable_marks && !spec.bindable && !fixed {
            NOT_BINDABLE
        } else {
            ""
        };
        Some(format!("{name}{kind}{required}{note}"))
    }

    fn fields(&self, fields: &[(String, PropSpec)], bindable_marks: bool) -> Option<String> {
        let mut parts = Vec::new();
        for (name, spec) in fields {
            match self.field(name, spec, bindable_marks) {
                Some(part) => parts.push(part),
                None if spec.required => return None,
                None => {}
            }
        }
        Some(parts.join(", "))
    }

    fn type_line(&self, definition: &TypeDef) -> Option<TypeLine> {
        let props = self.fields(&definition.props, true)?;
        let children = match &definition.children {
            ChildRule::None => String::new(),
            ChildRule::Any => " [children]".to_string(),
            ChildRule::Only(names) => format!(" [children: {}]", names.join("|")),
        };
        let mut line = format!("{}({props}){children}", definition.qualified);
        if !definition.description.is_empty() {
            line.push_str(" - ");
            line.push_str(&definition.description);
        }
        Some(TypeLine {
            qualified: definition.qualified.clone(),
            line,
        })
    }

    fn action_lines(&self) -> Vec<String> {
        self.actions
            .names()
            .into_iter()
            .filter_map(|name| {
                let spec = self.actions.action(name)?;
                let args = self.fields(&spec.args, false)?;
                Some(format!("{name}({args})"))
            })
            .collect()
    }
}

fn type_lines(definitions: &[&TypeDef], actions: &dyn ActionRegistry) -> Vec<TypeLine> {
    let signatures = Signatures { actions };
    definitions
        .iter()
        .filter_map(|definition| signatures.type_line(definition))
        .collect()
}

fn guide(types: &[TypeLine], actions: &dyn ActionRegistry) -> String {
    let signatures = Signatures { actions };
    let mut lines = vec![SHOW_UI.to_string(), LEGEND.to_string(), BINDING.to_string()];
    let action_lines = signatures.action_lines();
    if !action_lines.is_empty() {
        lines.push(ACTION.to_string());
        lines.extend(action_lines);
    }
    lines.extend(types.iter().map(|entry| entry.line.clone()));
    lines.join("\n")
}

fn closed(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn component_list(types: &[TypeLine]) -> Value {
    let names: Vec<&str> = types.iter().map(|entry| entry.qualified.as_str()).collect();
    json!({
        "type": "array",
        "items": closed(
            json!({
                "id": { "type": "string" },
                "type": { "type": "string", "enum": names },
                "props": { "type": "object" },
                "children": { "type": "array", "items": { "type": "string" } }
            }),
            &["id", "type"],
        )
    })
}

fn data_list() -> Value {
    json!({
        "type": "array",
        "items": closed(
            json!({
                "path": { "type": "string" },
                "value": { "type": ["string", "number", "boolean"] }
            }),
            &["path", "value"],
        )
    })
}

fn tool(name: &str, description: String, input_schema: Value) -> Value {
    json!({ "name": name, "description": description, "input_schema": input_schema })
}

pub fn show_ui_tool(definitions: &[&TypeDef], actions: &dyn ActionRegistry) -> Value {
    let types = type_lines(definitions, actions);
    let schema = closed(
        json!({
            "surface": { "type": "string" },
            "root": { "type": "string" },
            "components": component_list(&types),
            "data": data_list()
        }),
        &["root", "components"],
    );
    tool("show_ui", guide(&types, actions), schema)
}

pub fn update_ui_tool(definitions: &[&TypeDef], actions: &dyn ActionRegistry) -> Value {
    let types = type_lines(definitions, actions);
    let schema = closed(
        json!({
            "surface": { "type": "string" },
            "upsert": component_list(&types),
            "remove": { "type": "array", "items": { "type": "string" } },
            "root": { "type": "string" },
            "data": data_list()
        }),
        &["surface"],
    );
    tool("update_ui", UPDATE_UI.to_string(), schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::{ActionSpec, ActionTable};

    fn spec(value: Value) -> PropSpec {
        PropSpec::from_value(&value).unwrap()
    }

    #[test]
    fn props_are_written_tersely() {
        let actions = ActionTable::new().with("go", ActionSpec::new(false));
        let signatures = Signatures { actions: &actions };
        let cases = [
            (
                json!({ "kind": "string", "max": 80, "required": true }),
                "p<=80!",
            ),
            (json!({ "kind": "string", "max": 4000 }), "p"),
            (json!({ "kind": "int", "min": 1, "max": 4 }), "p: int 1..4"),
            (json!({ "kind": "number", "min": 0 }), "p: number>=0"),
            (json!({ "kind": "enum", "values": ["a", "b"] }), "p: a|b"),
            (json!({ "kind": "url" }), "p: https url"),
            (json!({ "kind": "scalar" }), "p: text|number|bool"),
            (
                json!({ "kind": "array", "max": 12, "of": { "kind": "object", "fields": { "id": { "kind": "string", "required": true } } } }),
                "p: [{id!}]<=12",
            ),
            (
                json!({ "kind": "array", "max": 1000, "of": { "kind": "string", "max": 8 } }),
                "p: [text<=8]",
            ),
            (json!({ "kind": "action", "required": true }), "p: action!"),
            (json!({ "kind": "path" }), "p: path"),
        ];
        for (value, expected) in cases {
            assert_eq!(
                signatures.field("p", &spec(value), true).as_deref(),
                Some(expected)
            );
        }
        let none = ActionTable::new();
        let without = Signatures { actions: &none };
        assert_eq!(without.kind(&spec(json!({ "kind": "action" }))), None);
        assert_eq!(
            without.fields(
                &[
                    ("a".into(), spec(json!({ "kind": "action" }))),
                    (
                        "b".into(),
                        spec(json!({ "kind": "bool", "bindable": false }))
                    )
                ],
                true
            ),
            Some("b: bool (no $bind or $ref)".to_string())
        );
        assert_eq!(
            without.fields(
                &[(
                    "a".into(),
                    spec(json!({ "kind": "action", "required": true }))
                )],
                true
            ),
            None
        );
    }
}
