use crate::support::case;
use polyui_decode::{CallFormat, Checker, Fed, Layout, Marker, Reply, ToolGrammar};
use serde_json::json;

fn grammar() -> ToolGrammar {
    case("ok_bills_card.json").grammar()
}

fn feed(grammar: &ToolGrammar, text: &str) -> Fed {
    let checker = Checker::byte_level();
    let mut matcher = checker.lark(&grammar.lark()).expect("grammar compiles");
    checker
        .feed(&mut matcher, text)
        .expect("matcher does not fail")
}

fn close_call(grammar: &ToolGrammar) -> String {
    grammar.render_call("close_ui", &json!({ "surface": "s1" }))
}

#[test]
fn plain_text_is_allowed_when_the_format_allows_it() {
    let grammar = grammar();
    for text in [
        "Your gas bill is 29 322 so'm.",
        "Привет! Gaz to'lovi: 29 322 so'm.\n\nAnything else?",
        "<b>bold</b> and <tool> are fine",
    ] {
        assert!(feed(&grammar, text).accepted(), "{text}");
    }
    assert!(
        !feed(&grammar, "").accepting,
        "an empty reply is not a reply"
    );
}

#[test]
fn text_cannot_contain_the_call_marker() {
    let grammar = grammar();
    let text = "Sure, here it is <tool_call> with junk";
    let at = feed(&grammar, text)
        .rejected_at()
        .expect("the marker inside text is rejected");
    assert_eq!(&text[..at], "Sure, here it is <tool_cal");
    let leading = format!(" {}", close_call(&grammar));
    assert_eq!(feed(&grammar, &leading).rejected_at(), Some(10));
}

#[test]
fn text_cannot_spell_a_near_miss_of_the_marker() {
    let grammar = grammar();
    for text in [
        "<tool_calls>\n{\"name\": \"show_ui\"}\n</tool_calls>",
        "<tool_call name=\"show_ui\">",
        "Here you go:\n<tool_call\n{}",
    ] {
        let at = feed(&grammar, text)
            .rejected_at()
            .unwrap_or_else(|| panic!("{text} must not pass as plain text"));
        let marker = text.find("<tool_call").expect("every case spells the stem");
        assert!(at <= marker + "<tool_call".len(), "{text} rejected at {at}");
    }
    assert!(feed(&grammar, "Use the tool_call helper or <tool> tags.").accepted());
}

#[test]
fn exactly_one_call_and_nothing_after_it() {
    let grammar = grammar();
    let call = close_call(&grammar);
    assert!(feed(&grammar, &call).accepted());
    let trailing = format!("{call}\nDone.");
    assert_eq!(feed(&grammar, &trailing).rejected_at(), Some(call.len()));
    let twice = format!("{call}{call}");
    assert_eq!(feed(&grammar, &twice).rejected_at(), Some(call.len()));
}

#[test]
fn tool_only_format_refuses_text() {
    let grammar = grammar().with_format(CallFormat::hermes().tool_only());
    assert_eq!(feed(&grammar, "Hello").rejected_at(), Some(0));
    assert!(feed(&grammar, &close_call(&grammar)).accepted());
}

#[test]
fn layouts_fix_the_whitespace() {
    let spaced = grammar();
    let compact = grammar().with_layout(Layout::Compact);
    let flexible = grammar().with_layout(Layout::Flexible);
    let spaced_call = close_call(&spaced);
    let compact_call = close_call(&compact);
    assert!(compact_call.contains("{\"name\":\"close_ui\",\"arguments\":{\"surface\":\"s1\"}}"));
    assert!(feed(&spaced, &spaced_call).accepted());
    assert!(feed(&compact, &compact_call).accepted());
    assert!(feed(&compact, &spaced_call).rejected_at().is_some());
    assert!(feed(&spaced, &compact_call).rejected_at().is_some());
    let loose = "<tool_call>\n{ \"name\" :\n \"close_ui\",  \"arguments\": {\"surface\": \"s1\"} }\n</tool_call>";
    assert!(feed(&flexible, &spaced_call).accepted());
    assert!(feed(&flexible, &compact_call).accepted());
    assert!(feed(&flexible, loose).accepted());
    assert!(feed(&spaced, loose).rejected_at().is_some());
}

#[test]
fn special_token_markers_need_the_token_not_its_spelling() {
    let checker = Checker::byte_level();
    let format = CallFormat {
        open: Marker::Token("<|tool|>".into()),
        close: Some(Marker::Token("<|/tool|>".into())),
        ..CallFormat::hermes()
    };
    let grammar = grammar().with_format(format);
    let lark = grammar.lark();
    assert!(
        lark.contains("call: <|tool|> \"\\n\" tool \"\\n\" <|/tool|>"),
        "{lark}"
    );
    let env = checker.tok_env().clone();
    let trie = env.tok_trie();
    let open = trie.get_special_token("<|tool|>").expect("open token");
    let close = trie.get_special_token("<|/tool|>").expect("close token");
    let spelled = close_call(&grammar);
    let body = spelled
        .strip_prefix("<|tool|>")
        .and_then(|rest| rest.strip_suffix("<|/tool|>"))
        .expect("render_call writes the spelling");
    let mut tokens = vec![open];
    tokens.extend(env.tokenize_bytes(body.as_bytes()));
    tokens.push(close);
    let mut matcher = checker.lark(&lark).expect("compiles");
    assert!(
        checker
            .feed_tokens(&mut matcher, &tokens)
            .expect("feeds")
            .accepted()
    );
    let mut matcher = checker.lark(&lark).expect("compiles");
    let fed = checker.feed(&mut matcher, &spelled).expect("feeds");
    assert_eq!(fed.rejected_at(), Some("<|too".len()));
    let unknown = checker
        .lark(
            &case("ok_bills_card.json")
                .grammar()
                .with_format(CallFormat::hermes_tokens())
                .lark(),
        )
        .err()
        .expect("<tool_call> is not a special token of the byte tokenizer");
    assert!(unknown.contains("unknown special token"), "{unknown}");
}

#[test]
fn replies_parse_back_into_calls_or_text() {
    let grammar = grammar();
    let input = case("ok_bills_card.json").value["input"].clone();
    match grammar.parse_reply(&grammar.render_call("show_ui", &input)) {
        Reply::Call { name, arguments } => {
            assert_eq!(name, "show_ui");
            assert_eq!(arguments, input);
        }
        other => panic!("expected a call, got {other:?}"),
    }
    assert_eq!(
        grammar.parse_reply("  It is 29 322 so'm.\n"),
        Reply::Text("It is 29 322 so'm.".into())
    );
    assert!(matches!(
        grammar.parse_reply("<tool_call>\n{\"name\": \"show_ui\", \"arguments\": {\n</tool_call>"),
        Reply::Invalid(_)
    ));
    assert_eq!(
        grammar
            .parse_reply("<tool_call>\n{\"name\": \"drop_db\", \"arguments\": {}}\n</tool_call>"),
        Reply::Invalid("unknown tool \"drop_db\"".into())
    );
}

#[test]
fn the_tools_prompt_lists_every_tool_and_the_call_shape() {
    let prompt = grammar().tools_prompt();
    for name in ["show_ui", "update_ui", "close_ui"] {
        assert!(prompt.contains(&format!("\"name\": \"{name}\"")), "{name}");
    }
    assert!(prompt.contains(
        "<tool_call>\n{\"name\": <tool name>, \"arguments\": <arguments object>}\n</tool_call>"
    ));
    assert!(!prompt.contains('\u{2014}'));
}
