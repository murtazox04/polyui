use polyui_core::protocol::{Message, Open};
use polyui_core::validate::{self, Operation};
use polyui_core::{
    ActionSpec, ActionTable, CatalogSet, Client, Locale, PropKind, StaticRefs, Validator,
};
use serde_json::{Value, json};

fn hello(locale: &str, catalogs: &[&str]) -> String {
    json!({ "v": 1, "t": "hello", "catalogs": catalogs, "modalities": ["voice"], "locale": locale })
        .to_string()
}

#[test]
fn server_pipeline_feeds_the_client() {
    let catalogs = CatalogSet::builtin();
    let actions = ActionTable::new().with(
        "pay_bill",
        ActionSpec::from_value(&json!({ "args": { "bill": { "kind": "string", "required": true } }, "requires_confirm": true })).unwrap(),
    );
    let refs = StaticRefs::new()
        .declare(
            "bill",
            PropKind::String {
                min: None,
                max: Some(40),
            },
        )
        .with("bill", "gas_total", json!("29 322"));
    let validator = Validator::new(&catalogs, &actions, &refs);
    let surface = validator
        .show_ui(&json!({
            "root": "card",
            "components": [
                { "id": "card", "type": "Card", "props": { "title": "Hisoblar" }, "children": ["total", "pay"] },
                { "id": "total", "type": "Stat", "props": { "label": "Gaz", "value": { "$ref": "bill:gas_total" }, "unit": "so'm" } },
                { "id": "pay", "type": "Button", "props": { "label": "To'lash", "tone": "primary", "on_press": { "action": "pay_bill", "args": { "bill": "b_17" } } } }
            ]
        }))
        .unwrap();
    let summary = surface.summary("s1", &catalogs, Locale::UzLatn);
    assert_eq!(
        summary,
        "surface s1 open: Card(2) \"Hisoblar\" > [Stat \"Gaz: [ref] so'm\", Button \"To'lash\" (act)]"
    );
    let mut resolved = surface.clone();
    resolved.components = validator
        .resolve_refs(&surface.components, Operation::Show)
        .unwrap();
    let sealed = validate::seal_actions(&mut resolved.components, &catalogs, &actions, || {
        "act_1".to_string()
    });
    assert_eq!(sealed.len(), 1);
    let fallback = resolved.fallback(&catalogs, Locale::UzLatn);
    assert_eq!(fallback, "Hisoblar. Gaz: 29 322 so'm. [To'lash]");
    let open = Message::Open(Open {
        surface: "s1".into(),
        seq: 1,
        root: resolved.root.clone(),
        components: resolved.components.clone(),
        data: resolved.data.clone(),
        fallback: Some(fallback),
    });
    let wire = open.to_json();
    assert!(!wire.contains("b_17") && !wire.contains("pay_bill") && !wire.contains("$ref"));

    let mut client = Client::new(&hello("uz-Latn", &["core@1"])).unwrap();
    let batch = client.receive(&wire);
    assert!(batch.send.is_empty());
    assert_eq!(
        batch.fallback.as_deref(),
        Some("Hisoblar. Gaz: 29 322 so'm. [To'lash]")
    );
    let utterances = serde_json::to_value(client.say("s1")).unwrap();
    assert_eq!(
        utterances,
        json!([
            { "id": "card", "text": "Hisoblar." },
            { "id": "total", "text": "Gaz 29 322 so'm." },
            { "id": "pay", "text": "To'lash." }
        ])
    );
    let found = client.match_voice("s1", "toʻlash").unwrap();
    assert_eq!(found.component, "pay");
    assert_eq!(found.action.unwrap()["action_id"], json!("act_1"));
    let first = client.match_voice("s1", "birinchisi").unwrap();
    assert_eq!(first.component, "pay");
}

#[test]
fn update_ui_applies_to_the_server_surface() {
    let catalogs = CatalogSet::builtin();
    let actions = ActionTable::new();
    let refs = StaticRefs::new();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let mut surface = validator
        .show_ui(&json!({ "root": "l", "components": [{ "id": "l", "type": "List", "props": { "title": "Steps" }, "children": ["a"] }, { "id": "a", "type": "Text", "props": { "text": "one" } }] }))
        .unwrap();
    let update = validator
        .update_ui_str(
            r#"{"surface":"s1","upsert":[{"id":"l","type":"List","props":{"title":"Steps"},"children":["a","b"]},{"id":"b","type":"Text","props":{"text":{"$bind":"/two"}}}],"data":[{"path":"/two","value":"two"}]}"#,
            &surface,
        )
        .unwrap();
    surface.apply(&update).unwrap();
    assert_eq!(surface.fallback(&catalogs, Locale::En), "Steps: one. two");
    assert_eq!(
        surface.summary("s1", &catalogs, Locale::En),
        "surface s1 open: List(2) \"Steps\" > [Text \"one\", Text \"two\"]"
    );
    let nothing = validator
        .update_ui(&json!({ "surface": "s1" }), &surface)
        .unwrap_err();
    assert_eq!(
        nothing.to_string(),
        "1 problem, nothing was changed:\n1. input: nothing to change: give upsert, remove, root or data."
    );
    let closed = validator.close_ui(&json!({ "surface": 7 })).unwrap_err();
    assert!(
        closed
            .to_string()
            .starts_with("1 problem, nothing was closed:")
    );
}

#[test]
fn downgrade_sends_alt_text_for_undeclared_types() {
    let catalogs = CatalogSet::builtin();
    let actions = ActionTable::new();
    let refs = StaticRefs::new();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let surface = validator
        .show_ui(&json!({ "root": "d", "components": [
            { "id": "d", "type": "chart:Dashboard", "props": { "title": "Spend" }, "children": ["s"] },
            { "id": "s", "type": "Stat", "props": { "label": "Total", "value": "10" } }
        ] }))
        .unwrap();
    let client_catalogs = catalogs.restrict(&["core@1".to_string()]);
    let downgraded =
        validate::downgrade_for_client(&surface, &catalogs, &client_catalogs, Locale::En);
    assert_eq!(downgraded.components[0].type_name, "Text");
    assert_eq!(
        downgraded.components[0].props["text"],
        json!("Spend. Total: 10")
    );
    assert!(downgraded.components[0].children.is_empty());
    assert_eq!(
        downgraded.components.len(),
        1,
        "children of a downgraded component are dropped"
    );
}

fn open(client: &mut Client, components: Value, data: Value) {
    let batch = client.receive(&json!({ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "root", "components": components, "data": data }).to_string());
    assert!(batch.send.is_empty(), "{:?}", batch.send);
}

#[test]
fn confirm_sheet_and_button_ordinals_by_voice() {
    let mut client = Client::new(&hello("ru", &["core@1"])).unwrap();
    open(
        &mut client,
        json!([
            { "id": "root", "type": "Card", "children": ["ask"] },
            { "id": "ask", "type": "ConfirmSheet", "props": { "title": "Удалить?", "on_confirm": { "action_id": "yes", "confirm": false }, "on_cancel": { "action_id": "no", "confirm": false } } }
        ]),
        json!({}),
    );
    assert_eq!(
        client.match_voice("s1", "да").unwrap().action.unwrap()["action_id"],
        json!("yes")
    );
    assert_eq!(
        client.match_voice("s1", "нет").unwrap().action.unwrap()["action_id"],
        json!("no")
    );

    let mut client = Client::new(&hello("en", &["core@1"])).unwrap();
    open(
        &mut client,
        json!([
            { "id": "root", "type": "List", "children": ["a", "b", "c"] },
            { "id": "a", "type": "Button", "props": { "label": "Gas", "on_press": { "action_id": "pa", "confirm": false } }, "voice": { "ordinal": 3 } },
            { "id": "b", "type": "Button", "props": { "label": "Power", "disabled": true, "on_press": { "action_id": "pb", "confirm": false } } },
            { "id": "c", "type": "Button", "props": { "label": "Water", "on_press": { "action_id": "pc", "confirm": false } }, "voice": { "synonyms": ["suv"] } }
        ]),
        json!({}),
    );
    assert_eq!(
        client.match_voice("s1", "the third one").unwrap().component,
        "a"
    );
    assert_eq!(client.match_voice("s1", "second").unwrap().component, "c");
    assert_eq!(client.match_voice("s1", "suv").unwrap().component, "c");
    assert!(client.match_voice("s1", "power").is_none());
    assert!(client.fire("s1", "b", "on_press", None).is_none());
}

#[test]
fn multiple_choice_toggles_and_welcome_restricts() {
    let mut client = Client::new(&hello("uz-Cyrl", &["core@1", "chart@1"])).unwrap();
    open(
        &mut client,
        json!([
            { "id": "root", "type": "ChoiceGroup", "props": { "label": "Танланг", "multiple": true, "value": "/picked", "options": [{ "id": "a", "label": "Олма" }, { "id": "b", "label": "Ўрик" }, { "id": "c", "label": "Нок" }] } }
        ]),
        json!({ "picked": ["c"] }),
    );
    assert_eq!(
        serde_json::to_value(client.say("s1")).unwrap(),
        json!([{ "id": "root", "text": "Танланг. Биринчи, Олма; иккинчи, Ўрик; учинчи, Нок." }])
    );
    let picked = client.match_voice("s1", "иккинчисини").unwrap();
    assert_eq!(picked.option.as_deref(), Some("b"));
    assert!(picked.action.is_none());
    assert_eq!(client.data("s1").unwrap()["picked"], json!(["c", "b"]));
    client.match_voice("s1", "нок");
    assert_eq!(client.data("s1").unwrap()["picked"], json!(["b"]));
    let batch = client.match_voice("s1", "урик").unwrap().batch;
    assert_eq!(batch.to_value()["ops"][0]["value"], json!([]));

    client.receive(
        &json!({ "v": 1, "t": "welcome", "session": "sess_1", "catalogs": ["core@1"] }).to_string(),
    );
    assert_eq!(client.session(), Some("sess_1"));
    assert!(client.catalogs().get("chart:Chart").is_none());
}

#[test]
fn renderer_contract_json_wrappers() {
    let mut client = Client::new(&hello("en", &["core@1"])).unwrap();
    let batch: Value = serde_json::from_str(&client.receive_json(
        &json!({ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "i", "components": [
            { "id": "i", "type": "Input", "props": { "label": "Name", "value": "/name" } }
        ] })
        .to_string(),
    ))
    .unwrap();
    assert_eq!(batch["ops"][0]["value"], Value::Null);
    let typed: Value =
        serde_json::from_str(&client.set_value_json("s1", "/name", "\"Ali\"")).unwrap();
    assert_eq!(typed["ops"][0]["op"], json!("update"));
    assert_eq!(typed["ops"][0]["value"], json!("Ali"));
    assert_eq!(typed["ops"][0]["props"]["value"], json!("/name"));
    let same: Value =
        serde_json::from_str(&client.set_value_json("s1", "/name", "\"Ali\"")).unwrap();
    assert_eq!(same["ops"], json!([]));
    assert_eq!(client.say_json("s1"), r#"[{"id":"i","text":"Name?"}]"#);
}
