use crate::support::case;
use polyui_core::tools::strict_tool_definitions;
use polyui_core::{ActionTable, CatalogSet};
use polyui_decode::schema::{is_supported_keyword, keywords};
use polyui_decode::{CallFormat, Change, Checker, Layout, SchemaError, ToolGrammar, prepare};
use serde_json::{Value, json};

fn strict(schema: &Value) -> Value {
    let mut schema = schema.clone();
    if let Value::Object(map) = &mut schema {
        map.insert("x-guidance".into(), json!({ "lenient": false }));
    }
    schema
}

fn grammars() -> Vec<(String, ToolGrammar)> {
    let bills = case("ok_bills_card.json");
    let extension = case("url_http_allowed_by_catalog.json");
    let none = ActionTable::new();
    let core = vec!["core@1".to_string()];
    let both = vec!["core@1".to_string(), "chart@1".to_string()];
    vec![
        (
            "all catalogs, no actions".into(),
            ToolGrammar::new(&CatalogSet::builtin(), &none).expect("builds"),
        ),
        (
            "all catalogs, bills actions".into(),
            ToolGrammar::new(&CatalogSet::builtin(), &bills.actions()).expect("builds"),
        ),
        (
            "core@1 client, bills actions".into(),
            ToolGrammar::for_client(&CatalogSet::builtin(), &core, &bills.actions())
                .expect("builds"),
        ),
        (
            "core@1 and chart@1 client, bills actions".into(),
            ToolGrammar::for_client(&CatalogSet::builtin(), &both, &bills.actions())
                .expect("builds"),
        ),
        (
            "with an extension catalog".into(),
            ToolGrammar::new(&extension.catalogs(), &bills.actions()).expect("builds"),
        ),
        (
            "strict tool schemas".into(),
            ToolGrammar::from_tools(&strict_tool_definitions(
                &CatalogSet::builtin(),
                &bills.actions(),
            ))
            .expect("builds"),
        ),
    ]
}

#[test]
fn every_generated_schema_compiles_without_lenient_mode() {
    let checker = Checker::byte_level();
    let mut compiled = 0;
    for (label, grammar) in grammars() {
        assert!(
            grammar.changes().is_empty(),
            "{label}: polyui-core schemas need no rewrite, got {:?}",
            grammar.changes()
        );
        for tool in grammar.tools() {
            let unsupported: Vec<String> = keywords(&tool.definition)
                .into_iter()
                .filter(|keyword| !is_supported_keyword(keyword))
                .collect();
            assert!(
                unsupported.is_empty(),
                "{label} {}: {unsupported:?}",
                tool.name
            );
            for schema in [&tool.definition, &tool.input_schema] {
                let mut matcher = checker
                    .json_schema(&strict(schema))
                    .unwrap_or_else(|error| panic!("{label} {}: {error}", tool.name));
                assert!(
                    matcher.grammar_warnings().is_empty(),
                    "{label} {}: {:?}",
                    tool.name,
                    matcher.grammar_warnings()
                );
                compiled += 1;
            }
            let call = grammar.call_schema(&tool.name).expect("tool is there");
            checker
                .json_schema(&call)
                .unwrap_or_else(|error| panic!("{label} {} call: {error}", tool.name));
            compiled += 1;
        }
        for format in [CallFormat::hermes(), CallFormat::hermes().tool_only()] {
            for layout in [Layout::Spaced, Layout::Compact, Layout::Flexible] {
                let lark = grammar
                    .clone()
                    .with_format(format.clone())
                    .with_layout(layout)
                    .lark();
                assert!(lark.starts_with("%llguidance {}\n"));
                let mut matcher = checker
                    .lark(&lark)
                    .unwrap_or_else(|error| panic!("{label} {layout:?}: {error}"));
                assert!(matcher.grammar_warnings().is_empty());
                compiled += 1;
            }
        }
    }
    println!("schemas and grammars compiled: {compiled}");
    assert_eq!(compiled, 6 * (3 * 3 + 6));
}

fn awkward_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tags": { "type": "array", "items": { "type": "string" }, "uniqueItems": true, "contains": { "const": "a" } },
            "kind": { "oneOf": [ { "const": "a" }, { "const": "b" } ] },
            "colour": { "type": "string", "format": "color" },
            "count": { "type": "integer", "not": { "const": 3 } }
        },
        "if": { "properties": { "kind": { "const": "a" } } },
        "then": { "required": ["count"] },
        "propertyNames": { "pattern": "^[a-z]+$" },
        "additionalProperties": false
    })
}

#[test]
fn unsupported_keywords_are_rewritten_so_the_schema_compiles() {
    let checker = Checker::byte_level();
    let raw = awkward_schema();
    let error = checker
        .json_schema(&strict(&raw))
        .err()
        .expect("llguidance refuses the raw schema");
    assert!(
        error.contains("Unimplemented keys") || error.contains("Unknown format"),
        "{error}"
    );
    let prepared = prepare(&raw).expect("no remote refs");
    let dropped: Vec<String> = prepared
        .changes
        .iter()
        .map(|change| match change {
            Change::Dropped { path, keyword } => format!("{path} {keyword}"),
            Change::OneOfToAnyOf { path } => format!("{path} oneOf"),
            Change::UnknownFormat { path, format } => format!("{path} format {format}"),
        })
        .collect();
    assert_eq!(
        dropped,
        vec![
            "#/properties/tags uniqueItems",
            "#/properties/tags contains",
            "#/properties/kind oneOf",
            "#/properties/colour format color",
            "#/properties/count not",
            "# if",
            "# then",
            "# propertyNames",
        ]
    );
    assert!(
        keywords(&prepared.schema)
            .iter()
            .all(|keyword| is_supported_keyword(keyword))
    );
    let mut matcher = checker
        .json_schema(&strict(&prepared.schema))
        .expect("the rewritten schema compiles");
    assert!(matcher.grammar_warnings().is_empty());
    let fed = checker
        .feed(&mut matcher, r#"{"tags": ["x"], "kind": "c"}"#)
        .expect("feeds");
    assert_eq!(fed.rejected_at(), Some(25));
    let mut matcher = checker
        .json_schema(&strict(&prepared.schema))
        .expect("compiles");
    assert!(
        checker
            .feed(
                &mut matcher,
                r#"{"tags": ["x", "x"], "kind": "a", "count": 3}"#
            )
            .expect("feeds")
            .accepted(),
        "the dropped keywords are left to the server"
    );
}

#[test]
fn a_ref_outside_the_schema_is_an_error() {
    let schema = json!({ "type": "object", "properties": { "a": { "$ref": "https://example.com/a.json" } } });
    assert_eq!(
        prepare(&schema),
        Err(SchemaError::RemoteRef {
            path: "#/properties/a".into(),
            target: "https://example.com/a.json".into()
        })
    );
    let local = json!({ "$defs": { "n": { "type": "integer" } }, "type": "object", "properties": { "a": { "$ref": "#/$defs/n" } } });
    let prepared = prepare(&local).expect("local refs are kept");
    assert!(prepared.changes.is_empty());
    assert!(Checker::byte_level().json_schema(&prepared.schema).is_ok());
}

#[test]
fn descriptions_are_left_out_of_the_grammar_but_kept_for_the_prompt() {
    let grammar = ToolGrammar::new(
        &CatalogSet::builtin(),
        &case("ok_bills_card.json").actions(),
    )
    .expect("builds");
    let show = grammar.tool("show_ui").expect("show_ui is there");
    assert!(show.definition.to_string().contains("\"description\""));
    assert!(!show.input_schema.to_string().contains("\"description\""));
    assert!(
        grammar
            .tools_prompt()
            .contains("Unique id: letters, digits, _ and -.")
    );
    assert!(grammar.lark().len() < show.definition.to_string().len() * 3);
}
