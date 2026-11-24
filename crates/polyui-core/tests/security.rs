use polyui_core::validate::{
    self, ActionSpec, ActionTable, Operation, Problems, StaticRefs, Surface, Validator,
};
use polyui_core::{Catalog, CatalogSet, Client, Locale, PropKind};
use serde_json::{Value, json};

fn actions() -> ActionTable {
    ActionTable::new()
        .with(
            "pay_bill",
            ActionSpec::from_value(&json!({
                "args": { "bill": { "kind": "string", "required": true, "max": 32 }, "receipt": { "kind": "url" } },
                "requires_confirm": true
            }))
            .unwrap(),
        )
        .with("noop", ActionSpec::new(false))
}

fn refs() -> StaticRefs {
    StaticRefs::new()
        .declare(
            "account",
            PropKind::String {
                min: None,
                max: Some(16),
            },
        )
        .with("account", "card", json!("4471"))
        .declare(
            "brand",
            PropKind::Url {
                schemes: vec!["https".into()],
            },
        )
        .with("brand", "logo", json!("javascript:alert(1)"))
}

fn extended() -> CatalogSet {
    let acme = Catalog::from_value(&json!({
        "id": "acme",
        "version": 1,
        "types": {
            "Gallery": {
                "description": "Pictures with links.",
                "role": "group",
                "props": {
                    "cover": { "kind": "object", "fields": { "src": { "kind": "url", "required": true }, "link": { "kind": "object", "fields": { "href": { "kind": "url" } } } } },
                    "images": { "kind": "array", "max": 4, "of": { "kind": "url" } },
                    "links": { "kind": "array", "of": { "kind": "object", "fields": { "href": { "kind": "url", "required": true } } } }
                },
                "alt": "gallery",
                "say": "gallery"
            }
        }
    }))
    .unwrap();
    CatalogSet::builtin().with(acme).unwrap()
}

fn check(catalogs: &CatalogSet, input: Value) -> Result<Surface, Problems> {
    let actions = actions();
    let refs = refs();
    Validator::new(catalogs, &actions, &refs).show_ui(&input)
}

fn rejected(catalogs: &CatalogSet, input: Value, needle: &str) -> Problems {
    let problems = check(catalogs, input).expect_err("input must be rejected");
    let text = problems.to_string();
    assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
    assert!(text.contains("nothing was shown"));
    problems
}

fn single(component: Value) -> Value {
    json!({ "root": component["id"].clone(), "components": [component] })
}

#[test]
fn unknown_component_injection() {
    let catalogs = CatalogSet::builtin();
    for type_name in [
        "Script",
        "<script>",
        "core:Button",
        "chart:Script",
        "acme:Card",
        "",
    ] {
        let problems = check(
            &catalogs,
            single(json!({ "id": "x", "type": type_name, "props": { "text": "<img onerror=alert(1)>" } })),
        )
        .expect_err("unknown types are rejected");
        assert!(problems.to_string().contains("is not in the catalog") || type_name.is_empty());
    }
}

#[test]
fn dangerous_urls_in_every_url_position() {
    let catalogs = extended();
    let bad = [
        "javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        " javascript:alert(1)",
        "java\tscript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "Data:image/svg+xml;base64,PHN2Zz4=",
        "file:///etc/passwd",
        "vbscript:msgbox(1)",
        "//evil.example/x",
    ];
    for url in bad {
        rejected(
            &catalogs,
            single(json!({ "id": "i", "type": "Image", "props": { "src": url, "alt": "x" } })),
            "prop \"src\"",
        );
        rejected(
            &catalogs,
            single(
                json!({ "id": "g", "type": "acme:Gallery", "props": { "cover": { "src": url } } }),
            ),
            "prop \"cover.src\"",
        );
        rejected(
            &catalogs,
            single(
                json!({ "id": "g", "type": "acme:Gallery", "props": { "cover": { "src": "https://ok.example/a.png", "link": { "href": url } } } }),
            ),
            "prop \"cover.link.href\"",
        );
        rejected(
            &catalogs,
            single(
                json!({ "id": "g", "type": "acme:Gallery", "props": { "images": ["https://ok.example/a.png", url] } }),
            ),
            "prop \"images[1]\"",
        );
        rejected(
            &catalogs,
            single(
                json!({ "id": "g", "type": "acme:Gallery", "props": { "links": [{ "href": url }] } }),
            ),
            "prop \"links[0].href\"",
        );
        rejected(
            &catalogs,
            single(
                json!({ "id": "b", "type": "Button", "props": { "label": "Pay", "on_press": { "action": "pay_bill", "args": { "bill": "b1", "receipt": url } } } }),
            ),
            "prop \"on_press.args.receipt\"",
        );
        rejected(
            &catalogs,
            json!({ "root": "i", "components": [{ "id": "i", "type": "Image", "props": { "src": { "$bind": "/u" }, "alt": "x" } }], "data": [{ "path": "/u", "value": url }] }),
            "data at \"/u\" does not fit",
        );
    }
}

#[test]
fn ref_values_are_checked_and_unregistered_sources_rejected() {
    let catalogs = CatalogSet::builtin();
    rejected(
        &catalogs,
        single(
            json!({ "id": "t", "type": "Text", "props": { "text": { "$ref": "secrets:password" } } }),
        ),
        "$ref source \"secrets\" is not registered",
    );
    rejected(
        &catalogs,
        single(json!({ "id": "t", "type": "Text", "props": { "text": { "$ref": "account" } } })),
        "must look like \"source:key\"",
    );
    rejected(
        &catalogs,
        single(
            json!({ "id": "t", "type": "Text", "props": { "text": { "$ref": "account:card", "extra": 1 } } }),
        ),
        "must be the only key",
    );
    rejected(
        &catalogs,
        single(
            json!({ "id": "i", "type": "Image", "props": { "src": { "$ref": "account:card" }, "alt": "x" } }),
        ),
        "$ref source \"account\" gives text of 0 to 16 characters, but this prop needs an absolute https URL.",
    );
    let refs = refs();
    let actions = actions();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let surface = check(
        &catalogs,
        single(json!({ "id": "i", "type": "Image", "props": { "src": { "$ref": "brand:logo" }, "alt": "x" } })),
    )
    .expect("the $ref shape is valid");
    let refused = validator
        .resolve_refs(&surface.components, Operation::Show)
        .expect_err("a javascript: value from a resolver is still rejected");
    assert!(!refused.to_string().contains("javascript"));
    let missing = check(
        &catalogs,
        single(
            json!({ "id": "t", "type": "Text", "props": { "text": { "$ref": "account:pin" } } }),
        ),
    )
    .unwrap();
    let missing = validator
        .resolve_refs(&missing.components, Operation::Show)
        .unwrap_err();
    assert_eq!(
        refused.items[0].message, missing.items[0].message,
        "every resolve failure reads the same"
    );
    assert_eq!(missing.items[0].message, validate::REF_FAILURE);
}

#[test]
fn ref_values_never_reach_the_summary() {
    let catalogs = CatalogSet::builtin();
    let surface = check(
        &catalogs,
        single(json!({ "id": "s", "type": "Stat", "props": { "label": "Card", "value": { "$ref": "account:card" } } })),
    )
    .unwrap();
    let refs = refs();
    let actions = actions();
    let resolved = Validator::new(&catalogs, &actions, &refs)
        .resolve_refs(&surface.components, Operation::Show)
        .unwrap();
    assert_eq!(resolved[0].props["value"], json!("4471"));
    let summary = surface.summary("s1", &catalogs, Locale::En);
    assert!(summary.contains("[ref]"), "{summary}");
    assert!(!summary.contains("4471"), "{summary}");
}

#[test]
fn actions_must_be_registered_and_args_must_fit() {
    let catalogs = CatalogSet::builtin();
    rejected(
        &catalogs,
        single(
            json!({ "id": "b", "type": "Button", "props": { "label": "x", "on_press": { "action": "drop_tables" } } }),
        ),
        "is not registered",
    );
    rejected(
        &catalogs,
        single(
            json!({ "id": "b", "type": "Button", "props": { "label": "x", "on_press": { "action": "pay_bill", "args": { "bill": 7 } } } }),
        ),
        "on_press.args.bill",
    );
    rejected(
        &catalogs,
        single(
            json!({ "id": "b", "type": "Button", "props": { "label": "x", "on_press": { "action": "pay_bill", "args": { "bill": "b".repeat(33) } } } }),
        ),
        "max is 32",
    );
    rejected(
        &catalogs,
        single(
            json!({ "id": "b", "type": "Button", "props": { "label": "x", "on_press": { "action": "pay_bill", "args": { "bill": { "$ref": "account:card" } } } } }),
        ),
        "not allowed here",
    );
    rejected(
        &catalogs,
        single(
            json!({ "id": "b", "type": "Button", "props": { "label": "x", "on_press": { "action_id": "act_forged", "confirm": false } } }),
        ),
        "action needs a string",
    );
    rejected(
        &catalogs,
        single(
            json!({ "id": "b", "type": "Button", "props": { "label": "x", "on_press": { "action": "noop", "with": ["/a", "not a pointer"] } } }),
        ),
        "on_press.with[1]",
    );
}

#[test]
fn sealing_hides_args_and_marks_confirm() {
    let catalogs = CatalogSet::builtin();
    let mut surface = check(
        &catalogs,
        json!({
            "root": "r",
            "components": [
                { "id": "r", "type": "Row", "children": ["pay", "ok"] },
                { "id": "pay", "type": "Button", "props": { "label": "Pay", "on_press": { "action": "pay_bill", "args": { "bill": "b_17" }, "with": ["/form/phone"] } } },
                { "id": "ok", "type": "Button", "props": { "label": "Ok", "on_press": { "action": "noop" } } }
            ]
        }),
    )
    .unwrap();
    let mut counter = 0;
    let sealed = validate::seal_actions(&mut surface.components, &catalogs, &actions(), || {
        counter += 1;
        format!("act_{counter}")
    });
    assert_eq!(sealed.len(), 2);
    assert_eq!(sealed[0].action, "pay_bill");
    assert_eq!(sealed[0].args["bill"], json!("b_17"));
    assert!(sealed[0].requires_confirm);
    assert!(!sealed[1].requires_confirm);
    assert_eq!(
        surface.components[1].props["on_press"],
        json!({ "action_id": "act_1", "confirm": true, "with": ["/form/phone"] })
    );
    let wire = serde_json::to_string(&surface.components).unwrap();
    assert!(!wire.contains("b_17") && !wire.contains("pay_bill"));
}

#[test]
fn graph_attacks() {
    let catalogs = CatalogSet::builtin();
    rejected(
        &catalogs,
        json!({ "root": "a", "components": [{ "id": "a", "type": "Card", "children": ["b"] }, { "id": "b", "type": "Card", "children": ["c"] }, { "id": "c", "type": "Card", "children": ["b"] }] }),
        "already has parent",
    );
    rejected(
        &catalogs,
        json!({ "root": "a", "components": [{ "id": "a", "type": "Card" }, { "id": "b", "type": "Card", "children": ["c"] }, { "id": "c", "type": "Card", "children": ["b"] }] }),
        "children make a cycle",
    );
    rejected(
        &catalogs,
        json!({ "root": "a", "components": [{ "id": "a", "type": "Card", "children": ["b"] }, { "id": "b", "type": "Divider" }, { "id": "b", "type": "Divider" }] }),
        "duplicate id",
    );
    rejected(
        &catalogs,
        json!({ "root": "a", "components": [{ "id": "a", "type": "Row", "children": ["b", "c"] }, { "id": "b", "type": "Card", "children": ["d"] }, { "id": "c", "type": "Card", "children": ["d"] }, { "id": "d", "type": "Divider" }] }),
        "which already has parent \"b\"",
    );
    let children: Vec<String> = (0..256).map(|n| format!("t{n}")).collect();
    let mut components = vec![json!({ "id": "root", "type": "Row", "children": children })];
    components.extend((0..256).map(|n| json!({ "id": format!("t{n}"), "type": "Divider" })));
    rejected(
        &catalogs,
        json!({ "root": "root", "components": components }),
        "257 components, max is 256",
    );
}

#[test]
fn invalid_pointers_everywhere() {
    let catalogs = CatalogSet::builtin();
    for pointer in ["", "no-slash", "/a~", "/a~2", "~1"] {
        rejected(
            &catalogs,
            single(
                json!({ "id": "i", "type": "Input", "props": { "label": "x", "value": pointer } }),
            ),
            "prop \"value\"",
        );
        rejected(
            &catalogs,
            single(json!({ "id": "t", "type": "Text", "props": { "text": { "$bind": pointer } } })),
            "prop \"text\"",
        );
    }
    let deep_pointer = "/a".repeat(33);
    rejected(
        &catalogs,
        single(
            json!({ "id": "i", "type": "Input", "props": { "label": "x", "value": deep_pointer } }),
        ),
        "more than 32 parts",
    );
}

fn as_str_input(catalogs: &CatalogSet, input: &str) -> Problems {
    let actions = actions();
    let refs = refs();
    Validator::new(catalogs, &actions, &refs)
        .show_ui_str(input)
        .expect_err("input must be rejected")
}

#[test]
fn limits_are_checked_before_parsing() {
    let catalogs = CatalogSet::builtin();
    let oversized = format!(
        "{{\"root\":\"t\",\"components\":[{{\"id\":\"t\",\"type\":\"Text\",\"props\":{{\"text\":\"x\"}}}}],\"pad\":\"{}\"}}",
        "y".repeat(256 * 1024)
    );
    assert!(
        as_str_input(&catalogs, &oversized)
            .to_string()
            .contains("bytes, max is 262144")
    );
    let deep = format!(
        "{{\"root\":\"t\",\"components\":[{{\"id\":\"t\",\"type\":\"Text\",\"props\":{{\"text\":{}{}}}}}]}}",
        "[".repeat(29),
        "]".repeat(29)
    );
    assert!(
        as_str_input(&catalogs, &deep)
            .to_string()
            .contains("deeper than 32")
    );
    let wide = format!(
        "{{\"root\":\"t\",\"components\":[{}]}}",
        vec!["1"; 10_000].join(",")
    );
    assert!(
        as_str_input(&catalogs, &wide)
            .to_string()
            .contains("more than 1000 items")
    );
    let huge = format!(
        "{{\"root\":\"t\",\"components\":[{{\"id\":\"t\",\"type\":\"Text\",\"props\":{{\"text\":\"{}\"}}}}]}}",
        "z".repeat(5000)
    );
    assert!(
        as_str_input(&catalogs, &huge)
            .to_string()
            .contains("longer than 4096")
    );
    let mut nested = json!("leaf");
    for _ in 0..33 {
        nested = json!([nested]);
    }
    rejected(
        &catalogs,
        json!({ "root": "t", "components": [], "x": nested }),
        "deeper than 32",
    );
    let items: Vec<u32> = (0..10_000).collect();
    rejected(
        &catalogs,
        json!({ "root": "t", "components": items }),
        "more than 1000 items",
    );
    rejected(
        &catalogs,
        single(json!({ "id": "t", "type": "Text", "props": { "text": "q".repeat(4001) } })),
        "max is 4000",
    );
}

#[test]
fn client_defends_itself() {
    let hello = json!({ "v": 1, "t": "hello", "catalogs": ["core@1"], "modalities": ["screen"] })
        .to_string();
    let mut client = Client::new(&hello).unwrap();
    let wide: Vec<Value> = (0..10_000)
        .map(|n| json!({ "id": format!("c{n}"), "type": "Divider" }))
        .collect();
    let batch = client.receive(&json!({ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "c0", "components": wide }).to_string());
    assert!(batch.ops.is_empty());
    assert_eq!(batch.send[0]["reason"], json!("bad_message"));
    let too_many: Vec<Value> = (0..300)
        .map(|n| json!({ "id": format!("c{n}"), "type": "Divider" }))
        .collect();
    let batch = client.receive(&json!({ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "c0", "components": too_many }).to_string());
    assert!(batch.ops.is_empty());
    let cyclic = json!({ "v": 1, "t": "ui.open", "surface": "s2", "seq": 1, "root": "a", "components": [
        { "id": "a", "type": "Card", "children": ["b"] },
        { "id": "b", "type": "Card", "children": ["a", "b", "c"] },
        { "id": "c", "type": "Card", "children": ["a"] },
        { "id": "c", "type": "Text", "props": { "text": "dup" } }
    ] });
    let batch = client.receive(&cyclic.to_string());
    let value = batch.to_value();
    assert_eq!(
        value["ops"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|op| op["op"] == "create")
            .count(),
        3
    );
    assert_eq!(client.say("s2").len(), 0);
    let script = json!({ "v": 1, "t": "ui.open", "surface": "s3", "seq": 1, "root": "i", "components": [
        { "id": "i", "type": "Image", "props": { "src": "javascript:alert(1)", "alt": "x" } }
    ] });
    let batch = client.receive(&script.to_string()).to_value();
    assert_eq!(batch["ops"][0]["type"], json!("Text"));
    assert!(!batch.to_string().contains("javascript"));
    let deep_set = format!("{}1{}", "[".repeat(40), "]".repeat(40));
    assert!(client.set_value("s3", "/x", &deep_set).ops.is_empty());
    assert!(client.set_value("s3", "nope", "1").ops.is_empty());
    assert!(
        client
            .fire(
                "s3",
                "i",
                "src",
                Some(&format!("{{\"a\":\"{}\"}}", "x".repeat(5000)))
            )
            .is_none()
    );
}

#[test]
fn extension_catalogs_cannot_open_dangerous_schemes() {
    for scheme in ["javascript", "data", "file", "vbscript"] {
        let attempt = Catalog::from_value(&json!({
            "id": "evil",
            "version": 1,
            "types": { "Link": { "description": "x", "role": "button", "props": { "href": { "kind": "url", "schemes": [scheme] } } } }
        }));
        assert!(attempt.is_err(), "{scheme}");
    }
}
