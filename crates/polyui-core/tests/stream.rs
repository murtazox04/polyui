#![cfg(feature = "stream")]

use polyui_core::stream::{ShowUiStream, StreamEvent};
use polyui_core::{ActionSpec, ActionTable, CatalogSet, StaticRefs, Validator};
use serde_json::json;

fn input() -> String {
    json!({
        "surface": "s1",
        "root": "card",
        "components": [
            { "id": "card", "type": "Card", "props": { "title": "Bills \"2026\"" }, "children": ["gas", "bad", "pay"] },
            { "id": "gas", "type": "Stat", "props": { "label": "Gas", "value": { "$bind": "/gas" } } },
            { "id": "bad", "type": "Badge", "props": { "text": "x", "tone": "purple" } },
            { "id": "pay", "type": "Button", "props": { "label": "Pay {all}", "on_press": { "action": "pay_bill", "args": { "bill": "b_1" } } } }
        ],
        "data": [{ "path": "/gas", "value": "29 322" }]
    })
    .to_string()
}

fn parts() -> (CatalogSet, ActionTable, StaticRefs) {
    (
        CatalogSet::builtin(),
        ActionTable::new().with(
            "pay_bill",
            ActionSpec::from_value(
                &json!({ "args": { "bill": { "kind": "string", "required": true } } }),
            )
            .unwrap(),
        ),
        StaticRefs::new(),
    )
}

fn collect(stream: &mut ShowUiStream<'_>, text: &str, chunk: usize) -> Vec<(usize, StreamEvent)> {
    let mut events = Vec::new();
    let mut fed = 0;
    let mut rest = text;
    while !rest.is_empty() {
        let mut cut = chunk.min(rest.len());
        while !rest.is_char_boundary(cut) {
            cut += 1;
        }
        let (head, tail) = rest.split_at(cut);
        fed += head.len();
        events.extend(stream.feed(head).into_iter().map(|event| (fed, event)));
        rest = tail;
    }
    events
}

#[test]
fn components_arrive_as_soon_as_they_close() {
    let (catalogs, actions, refs) = parts();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let text = input();
    for chunk in [1, 3, 17, 4096] {
        let mut stream = ShowUiStream::new(&validator);
        let events = collect(&mut stream, &text, chunk);
        let kinds: Vec<String> = events
            .iter()
            .map(|(_, event)| match event {
                StreamEvent::Surface(id) => format!("surface {id}"),
                StreamEvent::Root(id) => format!("root {id}"),
                StreamEvent::Component { index, component } => {
                    format!("component {index} {}", component.id)
                }
                StreamEvent::Rejected { index, .. } => format!("rejected {index}"),
                StreamEvent::Data(_) => "data".into(),
                StreamEvent::Invalid(problems) => format!("invalid {problems}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                "surface s1",
                "root card",
                "component 0 card",
                "component 1 gas",
                "rejected 2",
                "component 3 pay",
                "data"
            ],
            "chunk {chunk}"
        );
        if chunk == 1 {
            let card_closed = events
                .iter()
                .find(|(_, event)| matches!(event, StreamEvent::Component { index: 0, .. }))
                .map(|(fed, _)| *fed)
                .unwrap();
            let marker = "\"children\":[\"gas\",\"bad\",\"pay\"]}";
            let card_end = text.find(marker).unwrap() + marker.len();
            assert_eq!(card_closed, card_end);
        }
        let finished = stream.finish().unwrap_err();
        assert!(finished.to_string().contains("\"purple\" is not one of"));
    }
}

#[test]
fn a_valid_stream_finishes_like_show_ui() {
    let (catalogs, actions, refs) = parts();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let text = input().replace("purple", "info");
    let mut stream = ShowUiStream::new(&validator);
    let events = collect(&mut stream, &text, 5);
    assert!(events.iter().all(|(_, event)| !matches!(
        event,
        StreamEvent::Rejected { .. } | StreamEvent::Invalid(_)
    )));
    let surface = stream.finish().unwrap();
    assert_eq!(surface, validator.show_ui_str(&text).unwrap());
}

#[test]
fn duplicates_garbage_and_limits() {
    let (catalogs, actions, refs) = parts();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let mut stream = ShowUiStream::new(&validator);
    let events = stream.feed(
        r#"{"root":"a","components":[{"id":"a","type":"Divider"},{"id":"a","type":"Divider"},"#,
    );
    assert!(matches!(
        events.last(),
        Some(StreamEvent::Rejected { index: 1, .. })
    ));
    let events = stream.feed("]]]");
    assert!(matches!(events.last(), Some(StreamEvent::Invalid(_))));
    assert!(stream.failed());
    assert!(stream.feed("{}").is_empty());
    assert!(stream.finish().is_err());

    let mut deep = ShowUiStream::new(&validator);
    let events = deep.feed(&format!(
        "{{\"root\":\"a\",\"components\":[{}",
        "[".repeat(40)
    ));
    assert!(events.iter().any(|event| matches!(event, StreamEvent::Invalid(problems) if problems.to_string().contains("deeper than 32"))));

    let mut big = ShowUiStream::new(&validator);
    let chunk = format!("\"{}\",", "x".repeat(4000)).repeat(50);
    assert!(big.feed(&format!("{{\"pad\":[{chunk}")).is_empty());
    let events = big.feed(&chunk);
    assert!(matches!(events.last(), Some(StreamEvent::Invalid(_))));

    let mut odd = ShowUiStream::new(&validator);
    let events = odd.feed(r#"{"components":[1,"two",{"id":"x","type":"Divider"}],"root":"x"}"#);
    assert!(matches!(
        &events[0],
        StreamEvent::Component { index: 2, .. }
    ));
    assert!(odd.finish().is_err());
}
