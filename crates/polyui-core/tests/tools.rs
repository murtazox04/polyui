#![cfg(feature = "tools")]

use polyui_core::tools::{
    STRICT_LIMITS, StrictCost, ToolOptions, for_client, limit_strict, strict_tool_definitions,
    tool_definitions,
};
use polyui_core::{ActionSpec, ActionTable, CatalogSet, StaticRefs, Validator};
use serde_json::{Value, json};

const FORBIDDEN: [&str; 13] = [
    "not",
    "if",
    "then",
    "else",
    "uniqueItems",
    "contains",
    "propertyNames",
    "patternProperties",
    "$ref",
    "$defs",
    "definitions",
    "oneOf",
    "format",
];
const STRICT_FORBIDDEN: [&str; 8] = [
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "multipleOf",
    "minItems",
    "maxItems",
    "pattern",
];

fn actions() -> ActionTable {
    ActionTable::new()
        .with(
            "pay_bill",
            ActionSpec::from_value(&json!({ "args": { "bill": { "kind": "string", "required": true, "max": 32 } }, "requires_confirm": true })).unwrap(),
        )
        .with("pick_store", ActionSpec::new(false))
}

fn visit_schema(schema: &Value, path: &str, found: &mut Vec<String>, strict: bool) {
    let Value::Object(map) = schema else {
        if let Value::Array(items) = schema {
            for (index, item) in items.iter().enumerate() {
                visit_schema(item, &format!("{path}[{index}]"), found, strict);
            }
        }
        return;
    };
    for (key, value) in map {
        if FORBIDDEN.contains(&key.as_str()) || (strict && STRICT_FORBIDDEN.contains(&key.as_str()))
        {
            found.push(format!("{path}.{key}"));
        }
        match key.as_str() {
            "properties" => {
                for (name, property) in value.as_object().into_iter().flatten() {
                    visit_schema(
                        property,
                        &format!("{path}.properties.{name}"),
                        found,
                        strict,
                    );
                }
            }
            "enum" | "required" | "description" | "type" => {}
            _ => visit_schema(value, &format!("{path}.{key}"), found, strict),
        }
    }
    if map.get("type") == Some(&json!("object"))
        && map.get("additionalProperties") != Some(&json!(false))
    {
        found.push(format!("{path} is an open object"));
    }
}

#[test]
fn schemas_use_only_supported_keywords_and_closed_objects() {
    let catalogs = CatalogSet::builtin();
    for tools in [
        tool_definitions(&catalogs, &actions(), ToolOptions { strict: false }),
        tool_definitions(&catalogs, &actions(), ToolOptions { strict: true }),
        strict_tool_definitions(&catalogs, &actions()),
    ] {
        let mut found = Vec::new();
        for tool in tools.as_array().unwrap() {
            assert!(tool["name"].is_string() && tool["description"].is_string());
            let strict = tool["strict"] == true;
            visit_schema(
                &tool["input_schema"],
                tool["name"].as_str().unwrap(),
                &mut found,
                strict,
            );
        }
        assert!(found.is_empty(), "{found:#?}");
    }
    let loose = tool_definitions(&catalogs, &actions(), ToolOptions { strict: false });
    assert!(
        loose
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool.get("strict").is_none())
    );
}

fn every_property_required(schema: &Value, path: &str, found: &mut Vec<String>) {
    match schema {
        Value::Object(map) => {
            if let Some(Value::Object(properties)) = map.get("properties") {
                let required = map
                    .get("required")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for name in properties.keys() {
                    if !required.contains(&json!(name)) {
                        found.push(format!("{path}.{name}"));
                    }
                }
                for (name, property) in properties {
                    every_property_required(property, &format!("{path}.{name}"), found);
                }
            }
            for key in ["items", "anyOf"] {
                if let Some(inner) = map.get(key) {
                    every_property_required(inner, &format!("{path}.{key}"), found);
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                every_property_required(item, &format!("{path}[{index}]"), found);
            }
        }
        _ => {}
    }
}

#[test]
fn strict_tools_fit_the_api_limits() {
    let catalogs = CatalogSet::builtin();
    let strict = strict_tool_definitions(&catalogs, &actions());
    let mut optional = Vec::new();
    for tool in strict.as_array().unwrap() {
        assert_eq!(tool["strict"], json!(true));
        every_property_required(
            &tool["input_schema"],
            tool["name"].as_str().unwrap(),
            &mut optional,
        );
    }
    assert!(optional.is_empty(), "{optional:#?}");
    let chosen = tool_definitions(&catalogs, &actions(), ToolOptions { strict: true });
    let cost = chosen
        .as_array()
        .unwrap()
        .iter()
        .filter(|tool| tool["strict"] == true)
        .map(StrictCost::of_tool)
        .fold(StrictCost::default(), StrictCost::plus);
    assert!(cost.within(STRICT_LIMITS), "{cost:?}");
    assert_eq!(chosen[2]["strict"], json!(true));
    assert_eq!(chosen[0]["name"], json!("show_ui"));
    assert_eq!(chosen[1]["name"], json!("update_ui"));

    let mut tools = vec![
        chosen[2].clone(),
        json!({ "name": "huge", "strict": true, "input_schema": strict[0]["input_schema"].clone() }),
        json!({ "name": "small", "strict": true, "input_schema": { "type": "object", "properties": { "a": { "type": "string" } }, "required": ["a"], "additionalProperties": false } }),
    ];
    limit_strict(&mut tools);
    assert_eq!(tools[0]["strict"], json!(true));
    assert_eq!(tools[1].get("strict"), None);
    assert_eq!(tools[2]["strict"], json!(true));
}

#[test]
fn null_means_missing_for_strict_input() {
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let refs = StaticRefs::new();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let surface = validator
        .show_ui(&json!({
            "surface": null,
            "root": "card",
            "components": [
                { "id": "card", "type": "Card", "props": { "title": "Bills", "subtitle": null, "tone": null }, "children": ["pay"] },
                { "id": "pay", "type": "Button", "props": { "label": "Pay", "tone": null, "disabled": null, "on_press": { "action": "pay_bill", "args": { "bill": "b_1" }, "with": [] } }, "children": [] }
            ],
            "data": []
        }))
        .unwrap();
    assert_eq!(surface.id, None);
    assert!(!surface.components[0].props.contains_key("subtitle"));
    let update = validator
        .update_ui(
            &json!({ "surface": "s1", "upsert": [], "remove": [], "root": null, "data": [ { "path": "/a", "value": 1 } ] }),
            &surface,
        )
        .unwrap();
    assert_eq!(update.root, None);
    let missing = validator
        .show_ui(&json!({ "root": "t", "components": [ { "id": "t", "type": "Text", "props": { "text": null } } ], "data": [] }))
        .unwrap_err();
    assert!(
        missing.to_string().contains("required prop is missing"),
        "{missing}"
    );
}

#[test]
fn components_are_an_any_of_with_fixed_types() {
    let tools = tool_definitions(&CatalogSet::builtin(), &actions(), ToolOptions::default());
    let show = &tools[0];
    assert_eq!(show["name"], json!("show_ui"));
    let branches = show["input_schema"]["properties"]["components"]["items"]["anyOf"]
        .as_array()
        .unwrap();
    let types: Vec<&str> = branches
        .iter()
        .map(|branch| {
            let fixed = branch["properties"]["type"]["enum"].as_array().unwrap();
            assert_eq!(fixed.len(), 1);
            fixed[0].as_str().unwrap()
        })
        .collect();
    assert_eq!(types.len(), 14);
    assert!(types.contains(&"Button") && types.contains(&"chart:Chart"));
    let button = branches
        .iter()
        .find(|branch| branch["properties"]["type"]["enum"][0] == "Button")
        .unwrap();
    assert_eq!(button["required"], json!(["id", "type", "props"]));
    assert!(button["properties"].get("children").is_none());
    let on_press = &button["properties"]["props"]["properties"]["on_press"];
    assert_eq!(
        on_press["properties"]["action"]["enum"],
        json!(["pay_bill", "pick_store"])
    );
    assert_eq!(
        on_press["properties"]["args"]["anyOf"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        button["properties"]["props"]["properties"]["label"]["anyOf"][0]["maxLength"],
        json!(80)
    );
    let card = branches
        .iter()
        .find(|branch| branch["properties"]["type"]["enum"][0] == "Card")
        .unwrap();
    assert!(card["properties"]["children"].is_object());
    assert_eq!(card["required"], json!(["id", "type"]));
    let data = &show["input_schema"]["properties"]["data"]["items"];
    assert_eq!(data["required"], json!(["path", "value"]));
    assert_eq!(
        show["input_schema"]["required"],
        json!(["root", "components"])
    );
    assert_eq!(tools[1]["input_schema"]["required"], json!(["surface"]));
    assert_eq!(tools[2]["name"], json!("close_ui"));
}

#[test]
fn strict_mode_drops_limits_only() {
    let loose = tool_definitions(
        &CatalogSet::builtin(),
        &actions(),
        ToolOptions { strict: false },
    );
    let strict = strict_tool_definitions(&CatalogSet::builtin(), &actions());
    assert!(loose.to_string().contains("maxLength"));
    assert!(!strict.to_string().contains("maxLength"));
    assert_eq!(
        loose[0]["input_schema"]["properties"]["components"]["items"]["anyOf"]
            .as_array()
            .map(Vec::len),
        strict[0]["input_schema"]["properties"]["components"]["items"]["anyOf"]
            .as_array()
            .map(Vec::len)
    );
}

#[test]
fn only_declared_catalogs_and_reachable_actions() {
    let core_only = for_client(
        &CatalogSet::builtin(),
        &["core@1".to_string()],
        &actions(),
        ToolOptions::default(),
    );
    let text = core_only.to_string();
    assert!(!text.contains("chart:"));
    let no_actions = tool_definitions(
        &CatalogSet::builtin(),
        &ActionTable::new(),
        ToolOptions::default(),
    );
    let branches = no_actions[0]["input_schema"]["properties"]["components"]["items"]["anyOf"]
        .as_array()
        .unwrap();
    let types: Vec<&str> = branches
        .iter()
        .map(|branch| branch["properties"]["type"]["enum"][0].as_str().unwrap())
        .collect();
    assert!(!types.contains(&"Button"));
    assert!(!types.contains(&"ConfirmSheet"));
    assert!(types.contains(&"ChoiceGroup"));
    let choice = branches
        .iter()
        .find(|branch| branch["properties"]["type"]["enum"][0] == "ChoiceGroup")
        .unwrap();
    assert!(
        choice["properties"]["props"]["properties"]
            .get("on_change")
            .is_none()
    );
}
