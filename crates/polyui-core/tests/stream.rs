#![cfg(feature = "stream")]

use polyui_core::stream::{ShowUiStream, StreamEvent};
use polyui_core::{ActionRegistry, ActionSpec, ActionTable, CatalogSet, StaticRefs, Validator};
use serde_json::json;
use std::cell::Cell;

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

fn collect(
    stream: &mut ShowUiStream,
    validator: &Validator<'_>,
    text: &str,
    chunk: usize,
) -> Vec<(usize, StreamEvent)> {
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
        events.extend(
            stream
                .feed(validator, head)
                .into_iter()
                .map(|event| (fed, event)),
        );
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
        let mut stream = ShowUiStream::new(*validator.limits());
        let events = collect(&mut stream, &validator, &text, chunk);
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
        let finished = stream.finish(&validator).unwrap_err();
        assert!(finished.to_string().contains("\"purple\" is not one of"));
    }
}

#[test]
fn a_valid_stream_finishes_like_show_ui() {
    let (catalogs, actions, refs) = parts();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let text = input().replace("purple", "info");
    let mut stream = ShowUiStream::new(*validator.limits());
    let events = collect(&mut stream, &validator, &text, 5);
    assert!(events.iter().all(|(_, event)| !matches!(
        event,
        StreamEvent::Rejected { .. } | StreamEvent::Invalid(_)
    )));
    let surface = stream.finish(&validator).unwrap();
    assert_eq!(surface, validator.show_ui_str(&text).unwrap());
}

#[test]
fn duplicates_garbage_and_limits() {
    let (catalogs, actions, refs) = parts();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let mut stream = ShowUiStream::new(*validator.limits());
    let events = stream.feed(
        &validator,
        r#"{"root":"a","components":[{"id":"a","type":"Divider"},{"id":"a","type":"Divider"},"#,
    );
    assert!(matches!(
        events.last(),
        Some(StreamEvent::Rejected { index: 1, .. })
    ));
    let events = stream.feed(&validator, "]]]");
    assert!(matches!(events.last(), Some(StreamEvent::Invalid(_))));
    assert!(stream.failed());
    assert!(stream.feed(&validator, "{}").is_empty());
    assert!(stream.finish(&validator).is_err());

    let mut deep = ShowUiStream::new(*validator.limits());
    let events = deep.feed(
        &validator,
        &format!("{{\"root\":\"a\",\"components\":[{}", "[".repeat(40)),
    );
    assert!(events.iter().any(|event| matches!(event, StreamEvent::Invalid(problems) if problems.to_string().contains("deeper than 32"))));

    let mut big = ShowUiStream::new(*validator.limits());
    let chunk = format!("\"{}\",", "x".repeat(4000)).repeat(50);
    assert!(
        big.feed(&validator, &format!("{{\"pad\":[{chunk}"))
            .is_empty()
    );
    let events = big.feed(&validator, &chunk);
    assert!(matches!(events.last(), Some(StreamEvent::Invalid(_))));

    let mut odd = ShowUiStream::new(*validator.limits());
    let events = odd.feed(
        &validator,
        r#"{"components":[1,"two",{"id":"x","type":"Divider"}],"root":"x"}"#,
    );
    assert!(matches!(
        &events[0],
        StreamEvent::Component { index: 2, .. }
    ));
    assert!(odd.finish(&validator).is_err());
}

struct CountingActions {
    table: ActionTable,
    lookups: Cell<usize>,
}

impl CountingActions {
    fn take(&self) -> usize {
        self.lookups.replace(0)
    }
}

impl ActionRegistry for CountingActions {
    fn action(&self, name: &str) -> Option<&ActionSpec> {
        self.lookups.set(self.lookups.get() + 1);
        self.table.action(name)
    }

    fn names(&self) -> Vec<&str> {
        self.table.names()
    }
}

fn many_buttons(count: usize) -> String {
    let ids: Vec<String> = (0..count).map(|index| format!("b{index}")).collect();
    let mut components = vec![json!({ "id": "list", "type": "List", "children": ids })];
    components.extend(ids.iter().map(|id| {
        json!({ "id": id, "type": "Button", "props": { "label": format!("Pay {id}"), "on_press": { "action": "pay_bill", "args": { "bill": id } } } })
    }));
    json!({ "root": "list", "components": components }).to_string()
}

fn fragment_ends(text: &str, size: usize) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut end = 0;
    while end < text.len() {
        end = (end + size).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }
        ends.push(end);
    }
    ends
}

#[test]
fn one_stream_checks_each_component_once() {
    let (catalogs, table, refs) = parts();
    let actions = CountingActions {
        table,
        lookups: Cell::new(0),
    };
    let validator = Validator::new(&catalogs, &actions, &refs);
    let mut streamed = Vec::new();
    for count in [100, 200] {
        let text = many_buttons(count);
        validator.show_ui_str(&text).unwrap();
        let whole = actions.take();
        let mut stream = ShowUiStream::new(*validator.limits());
        let events = collect(&mut stream, &validator, &text, 16);
        let work = actions.take();
        let components = events
            .iter()
            .filter(|(_, event)| matches!(event, StreamEvent::Component { .. }))
            .count();
        assert_eq!(components, count + 1);
        assert!(
            work <= whole,
            "{count} buttons: {work} action lookups while streaming, {whole} for one show_ui"
        );
        streamed.push((count, text, work));
    }
    let (_, _, small) = &streamed[0];
    let (_, text, large) = &streamed[1];
    assert!(
        *large <= 2 * small + 2,
        "{small} lookups for 100, {large} for 200"
    );

    for end in fragment_ends(text, 16) {
        let mut restarted = ShowUiStream::new(*validator.limits());
        restarted.feed(&validator, &text[..end]);
    }
    let restarted = actions.take();
    assert!(
        restarted > 50 * large,
        "starting over on every fragment took {restarted} lookups, one stream took {large}"
    );
}
