use polyui_core::validate::{ActionSpec, ActionTable, Operation, StaticRefs, Surface, Validator};
use polyui_core::{CatalogSet, Client, Component, Limits, PropKind, text};
use serde_json::{Value, json};

fn hello() -> String {
    json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen", "voice"], "locale": "en" }).to_string()
}

fn ops_summary(batch: &Value) -> Vec<String> {
    batch["ops"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|op| {
            format!(
                "{} {}",
                op["op"].as_str().unwrap_or_default(),
                op["id"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

fn pending_confirm_client() -> Client {
    let mut client = Client::new(&hello()).unwrap();
    client.receive(
        &json!({
            "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "pay",
            "components": [ { "id": "pay", "type": "Button", "props": { "label": "Pay", "on_press": { "action_id": "act_1", "confirm": true } } } ],
            "data": {}
        })
        .to_string(),
    );
    client.receive(
        &json!({ "v": 1, "t": "ui.confirm", "surface": "s1", "action_id": "act_1", "nonce": "n_1", "title": "Pay gas" })
            .to_string(),
    );
    client
}

fn approves(spoken: &str) -> bool {
    let mut client = pending_confirm_client();
    client
        .match_voice("s1", spoken)
        .and_then(|found| found.action)
        .is_some_and(|action| action["confirm"] == "n_1")
}

#[test]
fn negated_or_mixed_answers_never_approve_a_confirm() {
    for spoken in [
        "don't confirm",
        "do not confirm",
        "I don't want to confirm",
        "не подтверждаю",
        "нет, не подтверждаю",
        "tasdiqlamayman",
        "тасдиқламайман",
        "yo'q, tasdiqlamayman",
        "yes no",
        "confirm the other one instead not this",
        "ha emas",
    ] {
        assert!(!approves(spoken), "\"{spoken}\" approved the payment");
    }
}

#[test]
fn short_polite_answers_still_approve_a_confirm() {
    for spoken in [
        "yes",
        "yes please",
        "ha, tasdiqlayman",
        "да, подтверждаю",
        "ҳа",
        "xa, to'la",
    ] {
        assert!(approves(spoken), "\"{spoken}\" did not approve the payment");
    }
}

#[test]
fn a_replaced_child_is_attached_to_its_parent_again() {
    let mut client = Client::new(&hello()).unwrap();
    client.receive(
        &json!({
            "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "card",
            "components": [
                { "id": "card", "type": "Card", "props": { "title": "Bills" }, "children": ["note"] },
                { "id": "note", "type": "Text", "props": { "text": "Due soon" } }
            ],
            "data": {}
        })
        .to_string(),
    );
    let batch = client
        .receive(
            &json!({
                "v": 1, "t": "ui.patch", "surface": "s1", "seq": 2,
                "upsert": [ { "id": "note", "type": "Badge", "props": { "text": "Due soon" } } ]
            })
            .to_string(),
        )
        .to_value();
    let ops = ops_summary(&batch);
    let create = ops.iter().position(|op| op == "create note").unwrap();
    let children = ops.iter().position(|op| op == "children card");
    assert!(
        children.is_some_and(|children| children > create),
        "the parent must list the new child after it is created: {ops:?}"
    );
}

#[test]
fn a_child_that_falls_back_to_text_is_attached_again() {
    let mut client = Client::new(&hello()).unwrap();
    client.receive(
        &json!({
            "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "card",
            "components": [
                { "id": "card", "type": "Card", "props": { "title": "Bills" }, "children": ["due"] },
                { "id": "due", "type": "Stat", "props": { "label": "Due", "value": { "$bind": "/due" } } }
            ],
            "data": { "due": "29 322" }
        })
        .to_string(),
    );
    let batch = client
        .receive(
            &json!({ "v": 1, "t": "ui.data", "surface": "s1", "seq": 2, "ops": [ { "op": "remove", "path": "/due" } ] })
                .to_string(),
        )
        .to_value();
    let ops = ops_summary(&batch);
    let create = ops.iter().position(|op| op == "create due").unwrap();
    let children = ops.iter().position(|op| op == "children card");
    assert!(
        children.is_some_and(|children| children > create),
        "{ops:?}"
    );
}

#[test]
fn a_replaced_root_is_announced_again() {
    let mut client = Client::new(&hello()).unwrap();
    client.receive(
        &json!({
            "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "top",
            "components": [ { "id": "top", "type": "Text", "props": { "text": "Hello" } } ],
            "data": {}
        })
        .to_string(),
    );
    let batch = client
        .receive(
            &json!({
                "v": 1, "t": "ui.patch", "surface": "s1", "seq": 2,
                "upsert": [ { "id": "top", "type": "Badge", "props": { "text": "Hello" } } ]
            })
            .to_string(),
        )
        .to_value();
    let ops = ops_summary(&batch);
    let create = ops.iter().position(|op| op == "create top").unwrap();
    let root = ops.iter().position(|op| op == "root top");
    assert!(root.is_some_and(|root| root > create), "{ops:?}");
}

fn current(components: Value, data: Value) -> Surface {
    let components: Vec<Component> = serde_json::from_value(components).unwrap();
    Surface {
        id: Some("s1".into()),
        root: components[0].id.clone(),
        components,
        data: data.as_object().cloned().unwrap_or_default(),
    }
}

#[test]
fn data_only_updates_recheck_existing_bindings() {
    let catalogs = CatalogSet::builtin();
    let actions = ActionTable::new();
    let refs = StaticRefs::new();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let surface = current(
        json!([
            { "id": "card", "type": "Card", "children": ["pic", "total"] },
            { "id": "pic", "type": "Image", "props": { "src": { "$bind": "/u" }, "alt": "Logo" } },
            { "id": "total", "type": "Stat", "props": { "label": "Total", "value": { "$bind": "/v" } } }
        ]),
        json!({}),
    );
    let script =
        json!({ "surface": "s1", "data": [ { "path": "/u", "value": "javascript:alert(1)" } ] });
    let problems = validator.update_ui(&script, &surface).unwrap_err();
    assert!(problems.to_string().contains("\"pic\""), "{problems}");
    let long = json!({ "surface": "s1", "data": [ { "path": "/v", "value": "9".repeat(41) } ] });
    let problems = validator.update_ui(&long, &surface).unwrap_err();
    assert!(problems.to_string().contains("\"total\""), "{problems}");
    let fine = json!({ "surface": "s1", "data": [ { "path": "/v", "value": "29 322" }, { "path": "/u", "value": "https://example.com/a.png" } ] });
    assert!(validator.update_ui(&fine, &surface).is_ok());
}

fn deep_path(segments: usize) -> String {
    (0..segments).map(|index| format!("/k{index}")).collect()
}

fn open_message(surface: &Surface) -> String {
    json!({
        "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": surface.root,
        "components": surface.components, "data": surface.data
    })
    .to_string()
}

#[test]
fn the_server_only_accepts_data_the_client_can_take() {
    let catalogs = CatalogSet::builtin();
    let actions = ActionTable::new();
    let refs = StaticRefs::new();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let show = |path: String| {
        validator.show_ui(&json!({
            "root": "t",
            "components": [ { "id": "t", "type": "Text", "props": { "text": "x" } } ],
            "data": [ { "path": path, "value": 1 } ]
        }))
    };
    let fits = show(deep_path(31)).unwrap();
    let mut client = Client::new(&hello()).unwrap();
    assert!(client.receive(&open_message(&fits)).send.is_empty());
    assert!(show(deep_path(32)).is_err(), "nesting 33 in ui.open");

    let picked: Vec<u32> = (0..1000).collect();
    let full = current(
        json!([ { "id": "t", "type": "Text", "props": { "text": "x" } } ]),
        json!({ "a": picked }),
    );
    let one_more = json!({ "surface": "s1", "data": [ { "path": "/a/-", "value": 1 } ] });
    assert!(
        validator.update_ui(&one_more, &full).is_err(),
        "array of 1001"
    );

    let limits = Limits::default();
    assert!(limits.max_data_bytes < limits.max_message_bytes);
}

#[cfg(feature = "stream")]
mod stream {
    use super::*;
    use polyui_core::stream::{ShowUiStream, StreamEvent};

    fn events(text: &str) -> Vec<StreamEvent> {
        let catalogs = CatalogSet::builtin();
        let actions = ActionTable::new();
        let refs = StaticRefs::new();
        let validator = Validator::new(&catalogs, &actions, &refs);
        let mut stream = ShowUiStream::new(&validator);
        stream.feed(text)
    }

    #[test]
    fn ids_are_checked_before_they_are_emitted() {
        let found = events(r#"{"surface":"no spaces","root":"a b","#);
        assert!(
            !found
                .iter()
                .any(|event| matches!(event, StreamEvent::Surface(_) | StreamEvent::Root(_))),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|event| matches!(event, StreamEvent::Invalid(_)))
        );
    }

    #[test]
    fn long_top_level_strings_are_cut_off() {
        let text = format!(r#"{{"root":"{}""#, "r".repeat(5000));
        let found = events(&text);
        assert!(
            matches!(found.last(), Some(StreamEvent::Invalid(_))),
            "{found:?}"
        );
    }

    #[test]
    fn the_component_count_is_capped() {
        let items: Vec<String> = (0..300)
            .map(|index| format!(r#"{{"id":"c{index}","type":"Divider"}}"#))
            .collect();
        let text = format!(r#"{{"root":"c0","components":[{}]"#, items.join(","));
        let found = events(&text);
        let emitted = found
            .iter()
            .filter(|event| matches!(event, StreamEvent::Component { .. }))
            .count();
        assert!(emitted <= 256, "{emitted} components were emitted");
        assert!(matches!(found.last(), Some(StreamEvent::Invalid(_))));
    }

    #[test]
    fn late_data_is_checked_against_components_already_emitted() {
        let text = json!({
            "root": "pic",
            "components": [ { "id": "pic", "type": "Image", "props": { "src": { "$bind": "/u" }, "alt": "Logo" } } ],
            "data": [ { "path": "/u", "value": "javascript:alert(1)" } ]
        })
        .to_string();
        let found = events(&text);
        assert!(
            !found
                .iter()
                .any(|event| matches!(event, StreamEvent::Data(_))),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|event| matches!(event, StreamEvent::Invalid(_)))
        );
    }
}

#[test]
fn blank_option_labels_are_rejected_and_ordinals_keep_positions() {
    let catalogs = CatalogSet::builtin();
    let actions = ActionTable::new().with("pick", ActionSpec::new(false));
    let refs = StaticRefs::new();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let result = validator.show_ui(&json!({
        "root": "pick",
        "components": [ { "id": "pick", "type": "ChoiceGroup", "props": {
            "options": [ { "id": "a", "label": "Gas" }, { "id": "b", "label": "   " }, { "id": "c", "label": "Power" } ],
            "value": "/choice"
        } } ]
    }));
    assert!(result.is_err());
    let spoken = text::join_ordinals(
        &["Gas".to_string(), " ".to_string(), "Power".to_string()],
        text::Locale::En,
    );
    assert_eq!(spoken, "first, Gas; third, Power");
}

fn acme() -> CatalogSet {
    let catalog = polyui_core::Catalog::from_value(&json!({
        "id": "acme",
        "version": 1,
        "types": {
            "Logo": {
                "description": "A logo.",
                "role": "img",
                "children": false,
                "props": { "src": { "kind": "url", "schemes": ["https", "http"], "required": true } },
                "alt": "logo",
                "say": "logo"
            }
        }
    }))
    .unwrap();
    CatalogSet::builtin().with(catalog).unwrap()
}

#[test]
fn ref_failures_read_the_same_whatever_the_value() {
    let catalogs = CatalogSet::builtin();
    let actions = ActionTable::new();
    let refs = StaticRefs::new()
        .declare(
            "account",
            PropKind::String {
                min: None,
                max: Some(40),
            },
        )
        .with("account", "gas", json!("**** 0083"))
        .with("account", "broken", json!("x".repeat(41)));
    let validator = Validator::new(&catalogs, &actions, &refs);
    let stat = |key: &str, prop: &str| {
        json!({ "root": "s", "components": [ { "id": "s", "type": "Stat", "props": {
            "label": "Gas", "value": "1", prop: { "$ref": format!("account:{key}") }
        } } ] })
    };
    let existing = validator.show_ui(&stat("gas", "unit")).unwrap_err();
    let missing = validator.show_ui(&stat("nope", "unit")).unwrap_err();
    assert_eq!(existing.to_string(), missing.to_string());
    let messages: Vec<String> = ["gas", "nope", "broken"]
        .iter()
        .map(|key| {
            let surface = validator.show_ui(&stat(key, "hint")).unwrap();
            match validator.resolve_refs(&surface.components, Operation::Show) {
                Ok(_) => "resolved".to_string(),
                Err(problems) => problems.items[0].message.clone(),
            }
        })
        .collect();
    assert_eq!(messages[0], "resolved");
    assert_eq!(messages[1], polyui_core::validate::REF_FAILURE);
    assert_eq!(messages[2], messages[1]);
}

#[test]
fn http_follows_the_host_setting_in_refs_and_the_client() {
    let catalogs = acme();
    let actions = ActionTable::new();
    let refs = StaticRefs::new()
        .declare(
            "brand",
            PropKind::Url {
                schemes: vec!["https".into(), "http".into()],
            },
        )
        .with("brand", "logo", json!("http://example.com/logo.png"));
    let input = json!({ "root": "l", "components": [ { "id": "l", "type": "acme:Logo", "props": { "src": { "$ref": "brand:logo" } } } ] });
    let strict = Validator::new(&catalogs, &actions, &refs);
    assert!(strict.show_ui(&input).is_err());
    let open = Validator::new(&catalogs, &actions, &refs).allow_http(true);
    let surface = open.show_ui(&input).unwrap();
    let resolved = open
        .resolve_refs(&surface.components, Operation::Show)
        .unwrap();
    assert_eq!(
        resolved[0].props["src"],
        json!("http://example.com/logo.png")
    );

    let hello =
        json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "acme@1"], "modalities": ["screen"] })
            .to_string();
    let message = json!({
        "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "l",
        "components": [ { "id": "l", "type": "acme:Logo", "props": { "src": "http://example.com/logo.png" } } ],
        "data": {}
    })
    .to_string();
    let mut client = Client::with_catalogs(&hello, acme()).unwrap();
    let batch = client.receive(&message).to_value();
    assert_eq!(batch["ops"][0]["type"], "Text", "{batch}");
    let mut allowing = Client::with_catalogs(&hello, acme())
        .unwrap()
        .with_allow_http(true);
    let batch = allowing.receive(&message).to_value();
    assert_eq!(batch["ops"][0]["type"], "acme:Logo", "{batch}");
}
