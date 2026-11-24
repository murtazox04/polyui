use crate::support::{Case, case, cases};
use polyui_decode::{Checker, Fed, ToolGrammar, render};
use serde_json::json;

const REJECTED_BY_GRAMMAR: [(&str, &str); 16] = [
    ("action_args_bad.json", "17, \"amount\""),
    ("action_not_registered.json", "transfer_all\""),
    ("bad_component_fields.json", ", \"colour\""),
    ("bind_not_allowed.json", "$bind\": \"/act\""),
    ("catalog_restricted.json", "chart:Chart\""),
    ("chart_rows_checked.json", "\"12\""),
    ("children_rules.json", ", \"children\": [\"x\"]"),
    ("data_entries_bad.json", "{\"nested\": true}"),
    ("depth_33.json", "[[[[[[[[[["),
    ("invalid_json.json", "}\n</tool_call>"),
    ("missing_required_prop.json", "}}]}}"),
    ("scene_rejected.json", ", \"scene\""),
    ("string_too_long.json", "x\"}}"),
    ("unknown_prop.json", "colour\""),
    ("unknown_type.json", "PayButton\""),
    ("wrong_kinds.json", "5}, \"children\""),
];

const LEFT_TO_THE_SERVER: [&str; 15] = [
    "bad_pointers.json",
    "bound_data_checked.json",
    "cycle.json",
    "depth_13.json",
    "duplicate_id.json",
    "missing_child.json",
    "orphan.json",
    "ref_unregistered.json",
    "root_missing.json",
    "self_child_and_root_child.json",
    "two_parents.json",
    "update_bad.json",
    "url_http_default.json",
    "url_http_needs_scheme.json",
    "url_javascript.json",
];

fn feed(checker: &Checker, grammar: &ToolGrammar, text: &str) -> Fed {
    let mut matcher = checker.lark(&grammar.lark()).expect("grammar compiles");
    checker
        .feed(&mut matcher, text)
        .expect("matcher does not fail")
}

fn run(checker: &Checker, case: &Case) -> (String, Fed) {
    let grammar = case.grammar();
    let text = case.call_text(&grammar);
    let fed = feed(checker, &grammar, &text);
    (text, fed)
}

#[test]
fn every_valid_fixture_is_accepted() {
    let checker = Checker::byte_level();
    let valid: Vec<Case> = cases().into_iter().filter(Case::ok).collect();
    assert_eq!(valid.len(), 6, "the conformance set has six valid cases");
    for case in &valid {
        let (text, fed) = run(&checker, case);
        assert!(
            fed.accepted(),
            "{} was not accepted: {fed:?}\n{text}",
            case.file
        );
    }
    let tools: Vec<&str> = valid.iter().map(Case::tool).collect();
    assert!(
        tools.contains(&"show_ui") && tools.contains(&"update_ui") && tools.contains(&"close_ui")
    );
}

#[test]
fn invalid_fixtures_are_rejected_at_the_offending_token() {
    let checker = Checker::byte_level();
    for (file, rest) in REJECTED_BY_GRAMMAR {
        let case = case(file);
        assert!(!case.ok());
        let (text, fed) = run(&checker, &case);
        let at = fed
            .rejected_at()
            .unwrap_or_else(|| panic!("{file} was not rejected: {fed:?}\n{text}"));
        assert!(
            text[at..].starts_with(rest),
            "{file} was rejected at byte {at}, before {:?}, expected before {rest:?}",
            &text[at..(at + 30).min(text.len())]
        );
    }
}

#[test]
fn the_grammar_cannot_check_graph_refs_urls_or_pointers() {
    let checker = Checker::byte_level();
    for file in LEFT_TO_THE_SERVER {
        let case = case(file);
        assert!(!case.ok(), "{file} is a fixture the server rejects");
        let (text, fed) = run(&checker, &case);
        assert!(
            fed.accepted(),
            "{file} is accepted by the grammar and only the server rejects it: {fed:?}\n{text}"
        );
    }
}

#[test]
fn every_fixture_is_classified() {
    let all = cases();
    let valid = all.iter().filter(|case| case.ok()).count();
    assert_eq!(
        valid + REJECTED_BY_GRAMMAR.len() + LEFT_TO_THE_SERVER.len(),
        all.len()
    );
    for case in all.iter().filter(|case| !case.ok()) {
        let listed = REJECTED_BY_GRAMMAR
            .iter()
            .any(|(file, _)| *file == case.file)
            || LEFT_TO_THE_SERVER.contains(&case.file.as_str());
        assert!(listed, "{} is not classified", case.file);
    }
}

fn bills_card() -> (Case, ToolGrammar) {
    let case = case("ok_bills_card.json");
    let grammar = case.grammar();
    (case, grammar)
}

#[test]
fn a_wrong_enum_value_is_rejected_at_its_first_wrong_byte() {
    let checker = Checker::byte_level();
    let (case, grammar) = bills_card();
    let mut input = case.value["input"].clone();
    input["components"][2]["props"]["tone"] = json!("loud");
    let text = grammar.render_call("show_ui", &input);
    let fed = feed(&checker, &grammar, &text);
    let at = fed.rejected_at().expect("wrong tone is rejected");
    assert!(text[..at].ends_with("\"tone\": \""), "{}", &text[..at]);
    assert!(text[at..].starts_with("loud\""));

    let mut input = case.value["input"].clone();
    input["components"][2]["props"]["tone"] = json!("primaryx");
    let text = grammar.render_call("show_ui", &input);
    let fed = feed(&checker, &grammar, &text);
    let at = fed
        .rejected_at()
        .expect("an enum value with extra letters is rejected");
    assert!(text[at..].starts_with("x\""), "{}", &text[at..]);
}

#[test]
fn a_missing_required_prop_is_rejected_where_the_object_closes() {
    let checker = Checker::byte_level();
    let (case, grammar) = bills_card();
    let mut input = case.value["input"].clone();
    input["components"][3]["props"]
        .as_object_mut()
        .expect("props is an object")
        .remove("on_press");
    let text = grammar.render_call("show_ui", &input);
    let fed = feed(&checker, &grammar, &text);
    let at = fed
        .rejected_at()
        .expect("a button without on_press is rejected");
    assert!(
        text[..at].ends_with("{\"$ref\": \"bill:power\"}"),
        "{}",
        &text[..at]
    );
    assert!(text[at..].starts_with('}'));
}

#[test]
fn keys_must_come_in_schema_order() {
    let checker = Checker::byte_level();
    let (case, grammar) = bills_card();
    let input = &case.value["input"];
    let as_written = format!(
        "<tool_call>\n{}\n</tool_call>",
        render(
            &json!({ "name": "show_ui", "arguments": input }),
            grammar.layout()
        )
    );
    let fed = feed(&checker, &grammar, &as_written);
    let at = fed
        .rejected_at()
        .expect("the fixture lists tone after on_press, the schema lists it before");
    assert!(
        as_written[at..].starts_with("tone\""),
        "{}",
        &as_written[at..]
    );
    assert!(feed(&checker, &grammar, &grammar.render_call("show_ui", input)).accepted());
}

#[test]
fn reordering_only_moves_keys() {
    let mut reordered = 0;
    for case in cases().iter().filter(|case| case.raw_text().is_none()) {
        let grammar = case.grammar();
        let input = &case.value["input"];
        let ordered = grammar.schema_order(case.tool(), input);
        assert_eq!(&ordered, input, "{}", case.file);
        if render(&ordered, grammar.layout()) != render(input, grammar.layout()) {
            reordered += 1;
        }
    }
    assert!(reordered > 0);
    println!("fixtures whose keys were put in schema order: {reordered}");
}

#[test]
fn update_and_close_calls_go_through_the_same_grammar() {
    let checker = Checker::byte_level();
    let (_, grammar) = bills_card();
    let close = grammar.render_call("close_ui", &json!({ "surface": "s1" }));
    assert!(feed(&checker, &grammar, &close).accepted());
    let unknown = close.replace("close_ui", "close_all");
    let fed = feed(&checker, &grammar, &unknown);
    let at = fed.rejected_at().expect("an unknown tool name is rejected");
    assert!(unknown[at..].starts_with("all\""), "{}", &unknown[at..]);
    let only_show = grammar
        .clone()
        .only(&["show_ui"])
        .expect("show_ui is there");
    let fed = feed(&checker, &only_show, &close);
    let at = fed
        .rejected_at()
        .expect("close_ui is not in a show_ui grammar");
    assert!(close[at..].starts_with("close_ui"), "{}", &close[at..]);
    let update = grammar.render_call("update_ui", &json!({ "surface": "s1", "remove": ["gas"] }));
    assert!(feed(&checker, &grammar, &update).accepted());
    let no_surface = grammar.render_call("update_ui", &json!({ "remove": ["gas"] }));
    let fed = feed(&checker, &grammar, &no_surface);
    assert!(fed.rejected_at().is_some(), "update_ui needs a surface");
    assert_eq!(
        grammar.parse_reply(&update),
        polyui_decode::Reply::Call {
            name: "update_ui".into(),
            arguments: json!({ "surface": "s1", "remove": ["gas"] })
        }
    );
}
