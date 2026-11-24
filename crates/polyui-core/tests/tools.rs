use polyui_core::tools::{ToolOptions, for_client, tool_definitions};
use polyui_core::{ActionSpec, ActionTable, CatalogSet};
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
    for strict in [false, true] {
        let tools = tool_definitions(&CatalogSet::builtin(), &actions(), ToolOptions { strict });
        let mut found = Vec::new();
        for tool in tools.as_array().unwrap() {
            assert!(tool["name"].is_string() && tool["description"].is_string());
            assert_eq!(tool["strict"].as_bool().unwrap_or(false), strict);
            visit_schema(
                &tool["input_schema"],
                tool["name"].as_str().unwrap(),
                &mut found,
                strict,
            );
        }
        assert!(found.is_empty(), "strict={strict}: {found:#?}");
    }
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
    let strict = tool_definitions(
        &CatalogSet::builtin(),
        &actions(),
        ToolOptions { strict: true },
    );
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
