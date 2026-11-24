use crate::catalog::{CatalogSet, PropKind, TypeDef};
use crate::pointer;
use crate::protocol::Component;
use crate::text::{self, Locale};
use crate::validate::check_literal;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefMode {
    Missing,
    Marked,
}

pub(crate) const REF_MARK: &str = "[ref]";

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Resolved {
    pub props: Map<String, Value>,
    pub values: Map<String, Value>,
    pub missing_required: bool,
}

pub(crate) fn bind(value: &Value, data: &Value, mode: RefMode) -> Option<Value> {
    match value {
        Value::Object(map) => {
            if map.len() == 1 {
                if let Some(path) = map.get("$bind") {
                    return path
                        .as_str()
                        .and_then(|path| pointer::get(data, path))
                        .cloned();
                }
                if map.contains_key("$ref") {
                    return match mode {
                        RefMode::Marked => Some(Value::from(REF_MARK)),
                        RefMode::Missing => None,
                    };
                }
            }
            Some(Value::Object(
                map.iter()
                    .filter_map(|(key, item)| {
                        bind(item, data, mode).map(|item| (key.clone(), item))
                    })
                    .collect(),
            ))
        }
        Value::Array(items) => Some(Value::Array(
            items
                .iter()
                .filter_map(|item| bind(item, data, mode))
                .collect(),
        )),
        other => Some(other.clone()),
    }
}

pub(crate) fn handle(value: &Value) -> Option<Value> {
    let map = value.as_object()?;
    let action_id = map.get("action_id")?.as_str()?;
    if action_id.is_empty() {
        return None;
    }
    let confirm = map.get("confirm").and_then(Value::as_bool).unwrap_or(false);
    let mut out = Map::new();
    out.insert("action_id".into(), Value::from(action_id));
    out.insert("confirm".into(), Value::from(confirm));
    Some(Value::Object(out))
}

pub(crate) fn handle_paths(value: &Value) -> Vec<String> {
    value
        .get("with")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|path| pointer::is_valid_target(path))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn resolve(
    component: &Component,
    definition: &TypeDef,
    data: &Value,
    mode: RefMode,
) -> Resolved {
    let mut resolved = Resolved::default();
    for (name, spec) in &definition.props {
        let raw = component.props.get(name);
        let value = match (&spec.kind, raw) {
            (_, None) => None,
            (PropKind::Action, Some(raw)) => handle(raw),
            (PropKind::Path, Some(raw)) => raw
                .as_str()
                .filter(|path| pointer::is_valid_target(path))
                .map(|path| {
                    resolved.values.insert(
                        name.clone(),
                        pointer::get(data, path).cloned().unwrap_or(Value::Null),
                    );
                    Value::from(path)
                }),
            (_, Some(raw)) => bind(raw, data, mode).filter(|value| {
                (mode == RefMode::Marked && value.as_str() == Some(REF_MARK))
                    || check_literal(spec, value).is_ok()
            }),
        };
        match value.or_else(|| spec.default.clone()) {
            Some(value) => {
                resolved.props.insert(name.clone(), value);
            }
            None if spec.required => resolved.missing_required = true,
            None => {}
        }
    }
    resolved
}

pub(crate) fn template_values(definition: &TypeDef, resolved: &Resolved) -> Map<String, Value> {
    let mut out = Map::new();
    for (name, value) in &resolved.props {
        match definition.prop(name).map(|spec| &spec.kind) {
            Some(PropKind::Action) => {}
            Some(PropKind::Path) => {
                out.insert(
                    name.clone(),
                    resolved.values.get(name).cloned().unwrap_or(Value::Null),
                );
            }
            _ => {
                out.insert(name.clone(), value.clone());
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextKind {
    Alt,
    Say,
}

pub(crate) fn component_text(
    component: &Component,
    definition: Option<&TypeDef>,
    values: &Map<String, Value>,
    children: &[String],
    kind: TextKind,
    locale: Locale,
) -> String {
    let fixed = match kind {
        TextKind::Alt => component.alt.as_deref(),
        TextKind::Say => component.say.as_deref().or(component.alt.as_deref()),
    };
    if let Some(fixed) = fixed {
        return text::tidy(fixed);
    }
    match definition {
        Some(definition) => {
            let template = match kind {
                TextKind::Alt => &definition.alt,
                TextKind::Say => &definition.say,
            };
            text::render(template, values, children, locale)
        }
        None => String::new(),
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Tree {
    pub order: Vec<usize>,
    pub children: Vec<Vec<usize>>,
    pub reachable: Vec<bool>,
}

pub(crate) fn index_of(components: &[Component]) -> BTreeMap<&str, usize> {
    let mut index = BTreeMap::new();
    for (position, component) in components.iter().enumerate() {
        index.entry(component.id.as_str()).or_insert(position);
    }
    index
}

pub(crate) fn build_tree(root: Option<&str>, components: &[Component]) -> Tree {
    let index = index_of(components);
    let mut tree = Tree {
        order: Vec::with_capacity(components.len()),
        children: vec![Vec::new(); components.len()],
        reachable: vec![false; components.len()],
    };
    let mut claimed = vec![false; components.len()];
    let root_index = root.and_then(|root| index.get(root).copied());
    let starts = root_index
        .into_iter()
        .chain(0..components.len())
        .collect::<Vec<_>>();
    for start in starts {
        if claimed.get(start).copied().unwrap_or(true) {
            continue;
        }
        let from_root = Some(start) == root_index;
        claimed[start] = true;
        let mut stack = vec![start];
        while let Some(current) = stack.pop() {
            tree.order.push(current);
            tree.reachable[current] = from_root;
            let mut kids = Vec::new();
            for child_id in &components[current].children {
                if let Some(&child) = index.get(child_id.as_str())
                    && !claimed[child]
                {
                    claimed[child] = true;
                    kids.push(child);
                }
            }
            stack.extend(kids.iter().rev());
            tree.children[current] = kids;
        }
    }
    tree
}

pub(crate) fn fallback_text(
    root: &str,
    components: &[Component],
    data: &Value,
    catalogs: &CatalogSet,
    locale: Locale,
) -> String {
    let tree = build_tree(Some(root), components);
    let mut texts: Vec<String> = vec![String::new(); components.len()];
    for &position in tree.order.iter().rev() {
        if !tree.reachable[position] {
            continue;
        }
        let component = &components[position];
        let definition = catalogs.get(&component.type_name);
        let values = definition
            .map(|definition| {
                template_values(
                    definition,
                    &resolve(component, definition, data, RefMode::Missing),
                )
            })
            .unwrap_or_default();
        let children: Vec<String> = tree.children[position]
            .iter()
            .map(|&child| std::mem::take(&mut texts[child]))
            .collect();
        texts[position] = component_text(
            component,
            definition,
            &values,
            &children,
            TextKind::Alt,
            locale,
        );
    }
    index_of(components)
        .get(root)
        .map(|&position| std::mem::take(&mut texts[position]))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn component(id: &str, children: &[&str]) -> Component {
        Component {
            id: id.into(),
            type_name: "Card".into(),
            children: children.iter().map(|child| child.to_string()).collect(),
            ..Component::default()
        }
    }

    #[test]
    fn tree_claims_each_child_once_and_cuts_cycles() {
        let components = vec![
            component("root", &["a", "b"]),
            component("a", &["b", "root"]),
            component("b", &["a"]),
            component("orphan", &["ghost"]),
        ];
        let tree = build_tree(Some("root"), &components);
        assert_eq!(tree.order, vec![0, 1, 2, 3]);
        assert_eq!(tree.children[0], vec![1, 2]);
        assert!(tree.children[1].is_empty());
        assert!(tree.children[2].is_empty());
        assert_eq!(tree.reachable, vec![true, true, true, false]);
    }

    #[test]
    fn bindings_and_handles() {
        let data = json!({ "a": { "b": 2 } });
        assert_eq!(
            bind(&json!({ "$bind": "/a/b" }), &data, RefMode::Missing),
            Some(json!(2))
        );
        assert_eq!(
            bind(&json!({ "$bind": "/zz" }), &data, RefMode::Missing),
            None
        );
        assert_eq!(
            bind(&json!({ "$ref": "x:y" }), &data, RefMode::Missing),
            None
        );
        assert_eq!(
            bind(&json!({ "$ref": "x:y" }), &data, RefMode::Marked),
            Some(json!(REF_MARK))
        );
        assert_eq!(
            bind(
                &json!([{ "label": { "$bind": "/a/b" } }, { "$bind": "/no" }]),
                &data,
                RefMode::Missing
            ),
            Some(json!([{ "label": 2 }]))
        );
        assert_eq!(
            handle(&json!({ "action_id": "a1", "with": ["/x"], "args": { "secret": 1 } })),
            Some(json!({ "action_id": "a1", "confirm": false }))
        );
        assert_eq!(handle(&json!({ "action": "pay" })), None);
        assert_eq!(
            handle_paths(&json!({ "with": ["/x", "bad", 3] })),
            vec!["/x"]
        );
    }
}
