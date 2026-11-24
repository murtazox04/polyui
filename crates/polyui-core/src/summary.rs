use crate::catalog::{CatalogSet, PropKind};
use crate::protocol::Component;
use crate::text::{self, Locale};
use crate::validate::Surface;
use crate::view::{self, RefMode, TextKind, Tree};
use serde_json::Value;

const MAX_LABEL_CHARS: usize = 60;
const MAX_VALUE_CHARS: usize = 40;

fn shorten(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(limit).collect();
        out.push_str("...");
        out
    }
}

fn is_action(value: &Value) -> bool {
    value.get("action").is_some() || value.get("action_id").is_some()
}

fn node_line(component: &Component, child_count: usize, context: &Context<'_>) -> String {
    let definition = context.catalogs.get(&component.type_name);
    let mut line = component.type_name.clone();
    if child_count >= 2 {
        line.push_str(&format!("({child_count})"));
    }
    let Some(definition) = definition else {
        return line;
    };
    let resolved = view::resolve(component, definition, context.data, RefMode::Marked);
    let values = view::template_values(definition, &resolved);
    let alt = view::component_text(
        component,
        Some(definition),
        &values,
        &[],
        TextKind::Alt,
        context.locale,
    );
    let label = text::label_from_alt(&alt);
    if !label.is_empty() {
        line.push_str(&format!(" \"{}\"", shorten(&label, MAX_LABEL_CHARS)));
    }
    let acts = definition.props.iter().any(|(name, spec)| {
        spec.kind == PropKind::Action && component.props.get(name).is_some_and(is_action)
    });
    if acts {
        line.push_str(" (act)");
    }
    for (name, spec) in &definition.props {
        if spec.kind != PropKind::Path {
            continue;
        }
        if let Some(current) = resolved.values.get(name).filter(|value| !value.is_null()) {
            line.push_str(&format!(
                " = {}",
                shorten(&current.to_string(), MAX_VALUE_CHARS)
            ));
        }
    }
    line
}

struct Context<'a> {
    components: &'a [Component],
    tree: Tree,
    catalogs: &'a CatalogSet,
    data: &'a Value,
    locale: Locale,
}

fn describe(position: usize, context: &Context<'_>) -> String {
    let children = &context.tree.children[position];
    let mut line = node_line(&context.components[position], children.len(), context);
    match children.as_slice() {
        [] => {}
        [only] => {
            line.push_str(" > ");
            line.push_str(&describe(*only, context));
        }
        many => {
            let parts: Vec<String> = many.iter().map(|&child| describe(child, context)).collect();
            line.push_str(" > [");
            line.push_str(&parts.join(", "));
            line.push(']');
        }
    }
    line
}

pub fn summarize(
    surface_id: &str,
    surface: &Surface,
    catalogs: &CatalogSet,
    locale: Locale,
) -> String {
    let data = surface.data_value();
    let context = Context {
        components: &surface.components,
        tree: view::build_tree(Some(&surface.root), &surface.components),
        catalogs,
        data: &data,
        locale,
    };
    let root = view::index_of(&surface.components)
        .get(surface.root.as_str())
        .copied();
    match root {
        Some(root) => format!("surface {surface_id} open: {}", describe(root, &context)),
        None => format!("surface {surface_id} open: (empty)"),
    }
}

pub fn closed(surface_id: &str) -> String {
    format!("surface {surface_id} closed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summary_shows_structure_refs_actions_and_values() {
        let components: Vec<Component> = serde_json::from_value(json!([
            { "id": "card", "type": "Card", "props": { "title": "Your bills" }, "children": ["list"] },
            { "id": "list", "type": "List", "children": ["gas", "power", "phone"] },
            { "id": "gas", "type": "Button", "props": { "label": "Gas 29 322 so'm", "on_press": { "action_id": "a1" } } },
            { "id": "power", "type": "Button", "props": { "label": { "$ref": "bill:power" }, "on_press": { "action": "pay" } } },
            { "id": "phone", "type": "Input", "props": { "label": "Phone", "value": "/phone" } }
        ]))
        .unwrap();
        let surface = Surface {
            id: None,
            root: "card".into(),
            components,
            data: json!({ "phone": "+998" })
                .as_object()
                .cloned()
                .unwrap_or_default(),
        };
        assert_eq!(
            summarize("s1", &surface, &CatalogSet::builtin(), Locale::En),
            "surface s1 open: Card \"Your bills\" > List(3) > [Button \"Gas 29 322 so'm\" (act), Button \"[ref]\" (act), Input \"Phone\" = \"+998\"]"
        );
        assert_eq!(closed("s1"), "surface s1 closed");
        let empty = Surface::default();
        assert_eq!(
            summarize("s2", &empty, &CatalogSet::builtin(), Locale::En),
            "surface s2 open: (empty)"
        );
        assert_eq!(shorten("abcdef", 3), "abc...");
    }
}
