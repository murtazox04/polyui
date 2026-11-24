use crate::support::{Case, case, cases};
use polyui_core::validate::{StaticRefs, Validator};
use polyui_decode::{
    Checker, Layout, SchemaDialect, ToolGrammar, merge_same_type_unions, render, same_type_unions,
    schema_order,
};
use serde_json::{Value, json};

fn calls(grammar: &ToolGrammar) -> Value {
    let branches: Vec<Value> = grammar
        .tools()
        .iter()
        .filter_map(|tool| grammar.call_schema(&tool.name))
        .collect();
    json!({ "anyOf": branches })
}

fn accepts(schema: &Value, instance: &Value) -> bool {
    let checker = Checker::byte_level();
    let mut matcher = checker
        .json_schema(schema)
        .expect("llguidance compiles the schema");
    let text = render(&schema_order(instance, schema), Layout::Spaced);
    checker
        .feed(&mut matcher, &text)
        .expect("the matcher does not fail")
        .accepted()
}

#[test]
fn merged_schemas_have_no_same_type_union_anywhere() {
    for case in cases() {
        let grammar = case.grammar();
        let before = calls(&grammar);
        let after = merge_same_type_unions(&before);
        assert!(!same_type_unions(&before).is_empty(), "{}", case.file);
        assert_eq!(
            same_type_unions(&after),
            Vec::<String>::new(),
            "{}",
            case.file
        );
        assert_eq!(merge_same_type_unions(&after), after, "{}", case.file);
        for tool in grammar.tools() {
            let merged = SchemaDialect::NoSameTypeUnions.apply(&tool.input_schema);
            assert!(
                same_type_unions(&merged).is_empty(),
                "{} {}",
                case.file,
                tool.name
            );
        }
    }
}

#[test]
fn the_standard_dialect_changes_nothing() {
    let grammar = case("ok_bills_card.json").grammar();
    let schema = calls(&grammar);
    assert_eq!(SchemaDialect::Standard.apply(&schema), schema);
    assert_eq!(
        grammar
            .clone()
            .with_dialect(SchemaDialect::Standard)
            .tools(),
        grammar.tools()
    );
}

#[test]
fn discriminators_become_one_enum_and_components_one_closed_object() {
    let grammar = case("ok_bills_card.json").grammar();
    let merged = merge_same_type_unions(&calls(&grammar));
    assert_eq!(merged["type"], "object");
    assert_eq!(
        merged["properties"]["name"],
        json!({ "type": "string", "enum": ["show_ui", "update_ui", "close_ui"] })
    );
    assert_eq!(merged["required"], json!(["name", "arguments"]));
    assert_eq!(merged["additionalProperties"], false);
    let component = &merged["properties"]["arguments"]["properties"]["components"]["items"];
    assert_eq!(component["type"], "object");
    assert_eq!(component["required"], json!(["id", "type"]));
    assert_eq!(component["additionalProperties"], false);
    let types = component["properties"]["type"]["enum"]
        .as_array()
        .expect("the type discriminator is one enum");
    for name in ["Text", "Card", "Button", "chart:Chart"] {
        assert!(types.contains(&json!(name)), "{name}");
    }
    assert_eq!(component["properties"]["props"]["type"], "object");
    assert_eq!(
        component["properties"]["props"]["additionalProperties"],
        false
    );
}

#[test]
fn strings_become_one_enum_or_a_plain_string() {
    let merged = merge_same_type_unions(&json!({ "anyOf": [
        { "const": "a" },
        { "type": "string", "enum": ["b", "c"] },
        { "type": "integer", "maximum": 3 },
        { "type": "number", "maximum": 9.5 }
    ] }));
    assert_eq!(
        merged,
        json!({ "anyOf": [
            { "type": "string", "enum": ["a", "b", "c"] },
            { "type": "number", "maximum": 9.5 }
        ] })
    );
    let merged = merge_same_type_unions(&json!({ "anyOf": [
        { "type": "string", "maxLength": 8 },
        { "type": "string", "maxLength": 40, "minLength": 1 }
    ] }));
    assert_eq!(merged, json!({ "type": "string", "maxLength": 40 }));
    let merged = merge_same_type_unions(&json!({ "anyOf": [
        { "type": "string", "maxLength": 8 },
        { "type": "string", "enum": ["x"] }
    ] }));
    assert_eq!(merged, json!({ "type": "string" }));
}

#[test]
fn objects_keep_the_union_of_properties_and_the_shared_required() {
    let merged = merge_same_type_unions(&json!({ "oneOf": [
        { "type": "object", "properties": { "kind": { "const": "a" }, "x": { "type": "integer" } }, "required": ["kind", "x"], "additionalProperties": false },
        { "type": "object", "properties": { "kind": { "const": "b" }, "x": { "type": "string" }, "y": { "type": "boolean" } }, "required": ["kind", "y"], "additionalProperties": false },
        { "type": ["string", "null"] }
    ] }));
    assert_eq!(
        merged,
        json!({ "anyOf": [
            { "type": "object", "properties": {
                "kind": { "type": "string", "enum": ["a", "b"] },
                "x": { "anyOf": [{ "type": "integer" }, { "type": "string" }] },
                "y": { "type": "boolean" }
            }, "required": ["kind"], "additionalProperties": false },
            { "type": "string" },
            { "type": "null" }
        ] })
    );
    let open = merge_same_type_unions(&json!({ "anyOf": [
        { "type": "object", "properties": { "a": { "type": "string" } }, "additionalProperties": false },
        { "type": "object" }
    ] }));
    assert_eq!(open, json!({ "type": "object", "properties": { "a": {} } }));
    assert!(accepts(&open, &json!({ "a": 1, "b": 2 })));
}

#[test]
fn every_valid_fixture_matches_the_merged_schema() {
    let valid: Vec<Case> = cases().into_iter().filter(Case::ok).collect();
    assert_eq!(valid.len(), 6, "the conformance set has six valid cases");
    for case in &valid {
        let input = &case.value["input"];
        let call = json!({ "name": case.tool(), "arguments": input });
        assert!(
            accepts(&merge_same_type_unions(&calls(&case.grammar())), &call),
            "{}",
            case.file
        );
        let grammar = case.grammar().with_dialect(SchemaDialect::NoSameTypeUnions);
        let schema = grammar
            .input_schema(case.tool())
            .expect("the fixture names a tool in the grammar");
        assert!(accepts(schema, input), "{}", case.file);
    }
}

#[test]
fn the_server_still_rejects_what_the_merged_schema_lets_through() {
    let case = case("ok_bills_card.json");
    let catalogs = case.catalogs();
    let actions = case.actions();
    let refs = StaticRefs::new();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let grammar = case.grammar();
    let standard = grammar
        .input_schema("show_ui")
        .expect("show_ui is in the grammar")
        .clone();
    let merged = SchemaDialect::NoSameTypeUnions.apply(&standard);
    let base = json!({ "surface": "s1", "root": "card", "components": [
        { "id": "card", "type": "Card", "props": { "title": "Your bills" }, "children": ["gas"] },
        { "id": "gas", "type": "Button", "props": { "label": "Gas", "on_press": { "action": "pay_bill", "args": { "bill": "b_17" } } } }
    ] });
    assert!(accepts(&standard, &base));
    assert!(accepts(&merged, &base));
    assert!(validator.show_ui(&base).is_ok());

    let mut button_prop_on_a_card = base.clone();
    button_prop_on_a_card["components"][0]["props"]["on_press"] =
        base["components"][1]["props"]["on_press"].clone();
    let mut text_without_its_text = base.clone();
    text_without_its_text["components"][0]["children"] = json!(["gas", "note"]);
    text_without_its_text["components"]
        .as_array_mut()
        .expect("components is a list")
        .push(json!({ "id": "note", "type": "Text" }));
    let mut divider_with_a_title = base.clone();
    divider_with_a_title["components"][0]["children"] = json!(["gas", "line"]);
    divider_with_a_title["components"]
        .as_array_mut()
        .expect("components is a list")
        .push(json!({ "id": "line", "type": "Divider", "props": { "title": "Bills" } }));
    let mut stat_value_too_long = base.clone();
    stat_value_too_long["components"][0]["children"] = json!(["gas", "total"]);
    stat_value_too_long["components"]
        .as_array_mut()
        .expect("components is a list")
        .push(json!({ "id": "total", "type": "Stat", "props": { "label": "Total", "value": "9".repeat(41) } }));

    for (name, input) in [
        ("a Button prop on a Card", button_prop_on_a_card),
        ("a Text without its text", text_without_its_text),
        ("a Divider with a title", divider_with_a_title),
        ("a Stat value over 40 characters", stat_value_too_long),
    ] {
        assert!(
            accepts(&merged, &input),
            "{name}: the merged schema lets it through"
        );
        assert!(
            !accepts(&standard, &input),
            "{name}: the standard schema refuses it"
        );
        let problems = validator
            .show_ui(&input)
            .expect_err("the server validator refuses it");
        assert!(!problems.items.is_empty(), "{name}");
    }
}
