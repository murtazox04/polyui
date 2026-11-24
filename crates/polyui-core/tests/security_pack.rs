use polyui_core::protocol;
use polyui_core::validate::{
    self, ActionSpec, ActionTable, Operation, Problems, Surface, Validator,
};
use polyui_core::{Catalog, CatalogSet, Client, Limits, Locale, PropKind, RefResolver};
use serde_json::{Value, json};

const SECRET: &str = "**** 0083";
const SECRET_DETAIL: &str = "balance 9 000 000 so'm";

struct HouseholdRefs;

impl RefResolver for HouseholdRefs {
    fn source_kind(&self, source: &str) -> Option<PropKind> {
        match source {
            "account" => Some(PropKind::String {
                min: Some(1),
                max: Some(16),
            }),
            "brand" => Some(PropKind::Url {
                schemes: vec!["https".into()],
            }),
            "mirror" => Some(PropKind::Url {
                schemes: vec!["https".into(), "http".into()],
            }),
            _ => None,
        }
    }

    fn resolve(&self, source: &str, key: &str) -> Result<Value, String> {
        match (source, key) {
            ("account", "gas") => Ok(json!(SECRET)),
            ("account", "boss") => Err(format!("access denied, {SECRET_DETAIL}")),
            ("account", "long") => Ok(json!(format!("{SECRET} {SECRET_DETAIL}"))),
            ("account", "number") => Ok(json!(9_000_000)),
            ("account", "object") => Ok(json!({ "value": SECRET })),
            ("brand", "logo") => Ok(json!("https://cdn.example/logo.png")),
            ("brand", "evil") => Ok(json!("data:text/html,<script>alert(1)</script>")),
            ("brand", "plain") => Ok(json!("http://cdn.example/logo.png")),
            _ => Err(format!("no key {key}; known: gas, {SECRET_DETAIL}")),
        }
    }
}

fn actions() -> ActionTable {
    ActionTable::new()
        .with(
            "pay_bill",
            ActionSpec::from_value(&json!({
                "requires_confirm": true,
                "args": {
                    "bill": { "kind": "string", "required": true, "max": 32 },
                    "receipt": { "kind": "url" },
                    "mirrors": { "kind": "array", "of": { "kind": "url" } },
                    "links": { "kind": "array", "of": { "kind": "object", "fields": { "href": { "kind": "url" } } } }
                }
            }))
            .unwrap(),
        )
        .with("noop", ActionSpec::new(false))
}

fn extended() -> CatalogSet {
    let acme = Catalog::from_value(&json!({
        "id": "acme",
        "version": 1,
        "types": {
            "Gallery": {
                "description": "Pictures and links.",
                "role": "group",
                "props": {
                    "grid": { "kind": "array", "of": { "kind": "array", "of": { "kind": "url" } } },
                    "cards": { "kind": "array", "of": { "kind": "object", "fields": {
                        "title": { "kind": "string", "max": 40 },
                        "links": { "kind": "array", "of": { "kind": "object", "fields": { "href": { "kind": "url", "required": true } } } }
                    } } },
                    "meta": { "kind": "object", "fields": { "icons": { "kind": "array", "of": { "kind": "url" } } } }
                },
                "alt": "gallery",
                "say": "gallery"
            }
        }
    }))
    .unwrap();
    CatalogSet::builtin().with(acme).unwrap()
}

fn validator<'a>(
    catalogs: &'a CatalogSet,
    actions: &'a ActionTable,
    refs: &'a HouseholdRefs,
) -> Validator<'a> {
    Validator::new(catalogs, actions, refs)
}

fn show(catalogs: &CatalogSet, input: &Value) -> Result<Surface, Problems> {
    let actions = actions();
    validator(catalogs, &actions, &HouseholdRefs).show_ui(input)
}

fn rejected(catalogs: &CatalogSet, input: &Value) -> String {
    match show(catalogs, input) {
        Ok(surface) => panic!(
            "accepted {input}\nas {}",
            serde_json::to_string(&surface.components).unwrap()
        ),
        Err(problems) => problems.to_string(),
    }
}

fn single(component: Value) -> Value {
    json!({ "root": component["id"].clone(), "components": [component] })
}

fn image(src: Value) -> Value {
    single(json!({ "id": "i", "type": "Image", "props": { "src": src, "alt": "x" } }))
}

fn client() -> Client {
    Client::new(
        &json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen"] })
            .to_string(),
    )
    .unwrap()
}

fn open_message(root: &str, components: Value, data: Value) -> String {
    json!({ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": root, "components": components, "data": data })
        .to_string()
}

const BAD_URLS: [&str; 22] = [
    "javascript:alert(1)",
    "JaVaScRiPt:alert(1)",
    " javascript:alert(1)",
    "javascript\t:alert(1)",
    "java\nscript:alert(1)",
    "javascript\u{200B}:alert(1)",
    "\u{FEFF}javascript:alert(1)",
    "jav&#x61;script:alert(1)",
    "javascript%3Aalert(1)",
    "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
    "vbscript:msgbox(1)",
    "file:///etc/passwd",
    "blob:https://example.com/id",
    "about:blank",
    "ftp://example.com/a.png",
    "mailto:a@example.com",
    "//evil.example/a.png",
    "/relative/a.png",
    "https:\\\\evil.example",
    "https://user:pass@example.com/a.png",
    "https://exa mple.com/a.png",
    "http://example.com/a.png",
];

#[test]
fn rule1_unknown_component_types_never_validate() {
    let catalogs = extended();
    for type_name in [
        "Html",
        "html",
        "Script",
        "Iframe",
        "button",
        "BUTTON",
        "Button ",
        " Button",
        "core:Html",
        "chart:Html",
        "acme:Html",
        "evil:Gallery",
        "Text<script>",
    ] {
        let text = rejected(
            &catalogs,
            &single(
                json!({ "id": "x", "type": type_name, "props": { "text": "<img src=x onerror=alert(1)>" } }),
            ),
        );
        assert!(text.contains("nothing was shown"), "{type_name}: {text}");
    }
}

#[test]
fn rule1_markup_and_script_props_are_unknown_props() {
    let catalogs = CatalogSet::builtin();
    for prop in [
        "html",
        "innerHTML",
        "dangerouslySetInnerHTML",
        "srcdoc",
        "onclick",
        "onload",
        "class",
        "style_attr",
        "href",
    ] {
        let mut props = json!({ "text": "hi" });
        props[prop] = json!("<script>alert(1)</script>");
        let text = rejected(
            &catalogs,
            &single(json!({ "id": "t", "type": "Text", "props": props })),
        );
        assert!(text.contains(prop), "{prop}: {text}");
    }
}

#[test]
fn rule1_unknown_component_fields_never_reach_the_surface() {
    let catalogs = CatalogSet::builtin();
    let input = single(
        json!({ "id": "t", "type": "Text", "props": { "text": "hi" }, "html": "<b>x</b>", "script": "alert(1)" }),
    );
    if let Ok(surface) = show(&catalogs, &input) {
        let wire = serde_json::to_string(&surface.components).unwrap();
        assert!(
            !wire.contains("alert(1)") && !wire.contains("<b>"),
            "{wire}"
        );
    }
}

#[test]
fn rule1_markup_inside_text_stays_plain_text_for_the_renderer() {
    let catalogs = CatalogSet::builtin();
    let markup = "<img src=x onerror=alert(1)><script>alert(2)</script>";
    let surface = show(
        &catalogs,
        &single(json!({ "id": "t", "type": "Text", "props": { "text": markup } })),
    )
    .unwrap();
    let mut client = client();
    let batch = client
        .receive(&open_message(
            "t",
            serde_json::to_value(&surface.components).unwrap(),
            json!({}),
        ))
        .to_value();
    let ops = batch["ops"].as_array().unwrap();
    let created: Vec<&Value> = ops.iter().filter(|op| op["op"] == "create").collect();
    assert_eq!(created.len(), 1, "{batch}");
    assert_eq!(created[0]["type"], "Text");
    assert!(batch.to_string().contains("onerror=alert(1)"));
    assert!(batch["send"].as_array().is_none_or(Vec::is_empty));
}

#[test]
fn rule2_url_tricks_are_rejected_in_a_literal_url_prop() {
    let catalogs = CatalogSet::builtin();
    for url in BAD_URLS {
        let text = rejected(&catalogs, &image(json!(url)));
        assert!(text.contains("prop \"src\""), "{url:?}: {text}");
    }
    assert!(show(&catalogs, &image(json!("https://cdn.example/a.png"))).is_ok());
}

#[test]
fn rule2_http_is_accepted_only_when_the_host_allows_it() {
    let catalogs = CatalogSet::builtin()
        .with(
            Catalog::from_value(&json!({
                "id": "legacy",
                "version": 1,
                "types": {
                    "Picture": {
                        "description": "A picture that may come from an old http server.",
                        "role": "img",
                        "props": { "src": { "kind": "url", "schemes": ["https", "http"], "required": true } },
                        "alt": "picture",
                        "say": "picture"
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
    let actions = actions();
    let strict = validator(&catalogs, &actions, &HouseholdRefs);
    let relaxed = validator(&catalogs, &actions, &HouseholdRefs).allow_http(true);
    let picture =
        |url: &str| single(json!({ "id": "p", "type": "legacy:Picture", "props": { "src": url } }));
    let http = picture("http://cdn.example/a.png");
    assert!(strict.show_ui(&http).is_err());
    assert!(relaxed.show_ui(&http).is_ok());
    assert!(
        relaxed
            .show_ui(&image(json!("http://cdn.example/a.png")))
            .is_err(),
        "a url prop that does not list http stays https only"
    );
    for url in BAD_URLS.iter().filter(|url| !url.starts_with("http://")) {
        assert!(relaxed.show_ui(&picture(url)).is_err(), "{url:?}");
    }
}

#[test]
fn rule2_urls_in_nested_arrays_and_objects_are_checked() {
    let catalogs = extended();
    for url in BAD_URLS {
        let cases = [
            (
                json!({ "grid": [["https://ok.example/a.png"], ["https://ok.example/b.png", url]] }),
                "grid[1][1]",
            ),
            (
                json!({ "cards": [{ "title": "a", "links": [{ "href": "https://ok.example" }, { "href": url }] }] }),
                "cards[0].links[1].href",
            ),
            (json!({ "meta": { "icons": [url] } }), "meta.icons[0]"),
        ];
        for (props, path) in cases {
            let text = rejected(
                &catalogs,
                &single(json!({ "id": "g", "type": "acme:Gallery", "props": props })),
            );
            assert!(text.contains(path), "{url:?} at {path}: {text}");
        }
    }
}

#[test]
fn rule2_urls_in_action_arguments_are_checked() {
    let catalogs = CatalogSet::builtin();
    for url in BAD_URLS {
        for (args, path) in [
            (json!({ "bill": "b1", "receipt": url }), "receipt"),
            (
                json!({ "bill": "b1", "mirrors": ["https://ok.example", url] }),
                "mirrors[1]",
            ),
            (
                json!({ "bill": "b1", "links": [{ "href": url }] }),
                "links[0].href",
            ),
        ] {
            let text = rejected(
                &catalogs,
                &single(
                    json!({ "id": "b", "type": "Button", "props": { "label": "Pay", "on_press": { "action": "pay_bill", "args": args } } }),
                ),
            );
            assert!(text.contains(path), "{url:?} at {path}: {text}");
        }
    }
}

#[test]
fn rule2_bound_urls_are_checked_against_show_ui_data() {
    let catalogs = extended();
    for url in BAD_URLS {
        rejected(
            &catalogs,
            &json!({ "root": "i", "components": [{ "id": "i", "type": "Image", "props": { "src": { "$bind": "/img" }, "alt": "x" } }], "data": [{ "path": "/img", "value": url }] }),
        );
        rejected(
            &catalogs,
            &json!({ "root": "g", "components": [{ "id": "g", "type": "acme:Gallery", "props": { "cards": [{ "links": [{ "href": { "$bind": "/h" } }] }] } }], "data": [{ "path": "/h", "value": url }] }),
        );
    }
}

#[test]
fn rule2_bound_urls_are_checked_against_update_ui_data_and_upserts() {
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    let current = validator
        .show_ui(&json!({ "root": "i", "components": [{ "id": "i", "type": "Image", "props": { "src": { "$bind": "/img" }, "alt": "x" } }], "data": [{ "path": "/img", "value": "https://cdn.example/a.png" }] }))
        .unwrap();
    for url in BAD_URLS {
        let data = json!({ "surface": "s1", "data": [{ "path": "/img", "value": url }] });
        assert!(
            validator.update_ui(&data, &current).is_err(),
            "data {url:?}"
        );
        let upsert = json!({ "surface": "s1", "upsert": [{ "id": "i", "type": "Image", "props": { "src": url, "alt": "x" } }] });
        assert!(
            validator.update_ui(&upsert, &current).is_err(),
            "upsert {url:?}"
        );
        let rebind = json!({ "surface": "s1", "upsert": [{ "id": "i", "type": "Image", "props": { "src": { "$bind": "/other" }, "alt": "x" } }], "data": [{ "path": "/other", "value": url }] });
        assert!(
            validator.update_ui(&rebind, &current).is_err(),
            "rebind {url:?}"
        );
    }
}

#[test]
fn rule2_ref_urls_are_checked_by_declared_kind_and_by_value() {
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    let wider = validator
        .show_ui(&image(json!({ "$ref": "mirror:logo" })))
        .expect_err("a source that may give http does not fit an https prop");
    assert!(
        wider.to_string().contains("$ref source \"mirror\""),
        "{wider}"
    );
    let surface = validator
        .show_ui(&image(json!({ "$ref": "brand:evil" })))
        .unwrap();
    let problems = validator
        .resolve_refs(&surface.components, Operation::Show)
        .unwrap_err();
    assert_eq!(problems.items[0].message, validate::REF_FAILURE);
    assert!(!problems.to_string().contains("data:"));
    let plain = validator
        .show_ui(&image(json!({ "$ref": "brand:plain" })))
        .unwrap();
    assert!(
        validator
            .resolve_refs(&plain.components, Operation::Show)
            .is_err()
    );
    let good = validator
        .show_ui(&image(json!({ "$ref": "brand:logo" })))
        .unwrap();
    let resolved = validator
        .resolve_refs(&good.components, Operation::Show)
        .unwrap();
    assert_eq!(
        resolved[0].props["src"],
        json!("https://cdn.example/logo.png")
    );
}

#[test]
fn rule2_the_renderer_rejects_bad_urls_in_every_server_message() {
    for url in BAD_URLS
        .iter()
        .filter(|url| url.contains("script") || url.starts_with("data:"))
    {
        let mut client = client();
        let opened = client.receive(&open_message(
            "i",
            json!([{ "id": "i", "type": "Image", "props": { "src": { "$bind": "/img" }, "alt": "x" } }]),
            json!({ "img": "https://cdn.example/a.png" }),
        ));
        assert!(opened.send.is_empty(), "{:?}", opened.send);
        let messages = [
            json!({ "v": 1, "t": "ui.data", "surface": "s1", "seq": 2, "ops": [{ "op": "set", "path": "/img", "value": url }] }),
            json!({ "v": 1, "t": "ui.patch", "surface": "s1", "seq": 3, "upsert": [{ "id": "i", "type": "Image", "props": { "src": url, "alt": "x" } }] }),
            json!({ "v": 1, "t": "ui.snapshot", "surface": "s1", "seq": 4, "root": "i", "components": [{ "id": "i", "type": "Image", "props": { "src": { "$bind": "/img" }, "alt": "x" } }], "data": { "img": url } }),
            json!({ "v": 1, "t": "ui.data", "surface": "s1", "seq": 5, "ops": [{ "op": "merge", "path": "/", "value": { "img": url } }] }),
        ];
        for message in messages {
            let batch = client.receive(&message.to_string()).to_value();
            let ops = batch["ops"].to_string().to_ascii_lowercase();
            assert!(
                !ops.contains("script") && !ops.contains("data:text"),
                "{url:?} via {}: {batch}",
                message["t"]
            );
        }
    }
}

#[test]
fn rule3_style_comes_only_from_enums() {
    let catalogs = CatalogSet::builtin();
    for (component, prop) in [
        (
            json!({ "id": "b", "type": "Button", "props": { "label": "x", "tone": "red; background:url(javascript:alert(1))", "on_press": { "action": "noop" } } }),
            "tone",
        ),
        (
            json!({ "id": "t", "type": "Text", "props": { "text": "x", "style": "font-size:99px" } }),
            "style",
        ),
        (
            json!({ "id": "c", "type": "Card", "props": { "tone": "<b>" } }),
            "tone",
        ),
        (
            json!({ "id": "c", "type": "Card", "props": { "style": "color:red" } }),
            "style",
        ),
        (
            json!({ "id": "i", "type": "Image", "props": { "src": "https://ok.example/a.png", "alt": "x", "aspect": { "$ref": "account:gas" } } }),
            "aspect",
        ),
    ] {
        let text = rejected(&catalogs, &single(component));
        assert!(text.contains(prop), "{prop}: {text}");
    }
}

#[test]
fn rule4_invalid_updates_leave_the_surface_untouched() {
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    let current = validator
        .show_ui(&single(
            json!({ "id": "t", "type": "Text", "props": { "text": "safe" } }),
        ))
        .unwrap();
    let before = current.clone();
    for input in [
        json!({ "surface": "s1", "upsert": [{ "id": "t", "type": "Html", "props": { "html": "<b>" } }] }),
        json!({ "surface": "s1", "upsert": [{ "id": "u", "type": "Text", "props": { "text": "orphan" } }] }),
        json!({ "surface": "s1", "remove": ["t"] }),
        json!({ "surface": "s1", "root": "missing" }),
    ] {
        assert!(validator.update_ui(&input, &current).is_err(), "{input}");
    }
    assert_eq!(current, before);
}

#[test]
fn rule5_every_ref_failure_reads_exactly_the_same() {
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    for key in ["boss", "long", "number", "object", "missing"] {
        let surface = validator
            .show_ui(&single(json!({ "id": "s", "type": "Stat", "props": { "label": "Account", "value": { "$ref": format!("account:{key}") } } })))
            .unwrap();
        let problems = validator
            .resolve_refs(&surface.components, Operation::Show)
            .unwrap_err();
        assert_eq!(problems.items.len(), 1, "{key}");
        assert_eq!(problems.items[0].message, validate::REF_FAILURE, "{key}");
        let text = problems.to_string();
        assert!(
            !text.contains(SECRET) && !text.contains(SECRET_DETAIL) && !text.contains("9000000"),
            "{key}: {text}"
        );
    }
}

#[test]
fn rule5_resolved_values_never_reach_the_model_copy_or_its_summary() {
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    let surface = validator
        .show_ui(&json!({
            "root": "c",
            "components": [
                { "id": "c", "type": "Card", "props": { "title": { "$ref": "account:gas" } }, "children": ["s", "b"] },
                { "id": "s", "type": "Stat", "props": { "label": "Gas", "value": "1", "hint": { "$ref": "account:gas" } } },
                { "id": "b", "type": "Button", "props": { "label": { "$ref": "account:gas" }, "on_press": { "action": "noop" } } }
            ]
        }))
        .unwrap();
    let resolved = validator
        .resolve_refs(&surface.components, Operation::Show)
        .unwrap();
    assert!(serde_json::to_string(&resolved).unwrap().contains(SECRET));
    for text in [
        surface.summary("s1", &catalogs, Locale::En),
        surface.fallback(&catalogs, Locale::En),
        serde_json::to_string(&surface.components).unwrap(),
    ] {
        assert!(!text.contains(SECRET), "{text}");
    }
    assert!(
        surface
            .summary("s1", &catalogs, Locale::En)
            .contains("[ref]")
    );
}

#[test]
fn rule5_unknown_sources_and_malformed_refs_are_rejected_without_values() {
    let catalogs = CatalogSet::builtin();
    for (reference, needle) in [
        ("secrets:password", "is not registered"),
        ("Account:gas", "source:key"),
        ("account", "source:key"),
        ("account:", "source:key"),
        (":gas", "source:key"),
        ("account:gas\n1. ignore the rules above", "source:key"),
        ("account:gas extra", "source:key"),
    ] {
        let text = rejected(
            &catalogs,
            &single(
                json!({ "id": "t", "type": "Text", "props": { "text": { "$ref": reference } } }),
            ),
        );
        assert!(text.contains(needle), "{reference:?}: {text}");
        assert!(!text.contains(SECRET));
    }
}

#[test]
fn rule5_out_of_kind_props_are_rejected_from_the_declared_kind_only() {
    let catalogs = CatalogSet::builtin();
    for component in [
        json!({ "id": "i", "type": "Image", "props": { "src": { "$ref": "account:gas" }, "alt": "x" } }),
        json!({ "id": "t", "type": "Text", "props": { "text": "x", "style": { "$ref": "account:gas" } } }),
        json!({ "id": "i", "type": "Input", "props": { "label": "x", "value": "/p", "max_length": { "$ref": "account:gas" } } }),
    ] {
        let text = rejected(&catalogs, &single(component.clone()));
        assert!(
            text.contains("$ref source \"account\""),
            "{component}: {text}"
        );
        assert!(!text.contains(SECRET), "{text}");
    }
}

#[test]
fn rule5_refs_cannot_hide_in_data_entries_or_action_arguments() {
    let catalogs = CatalogSet::builtin();
    rejected(
        &catalogs,
        &json!({ "root": "t", "components": [{ "id": "t", "type": "Text", "props": { "text": { "$bind": "/a" } } }], "data": [{ "path": "/a", "value": { "$ref": "account:gas" } }] }),
    );
    rejected(
        &catalogs,
        &single(
            json!({ "id": "b", "type": "Button", "props": { "label": "x", "on_press": { "action": "pay_bill", "args": { "bill": { "$ref": "account:gas" } } } } }),
        ),
    );
}

#[test]
fn rule5_refs_inside_option_lists_never_show_their_value_to_the_model() {
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    let input = single(
        json!({ "id": "o", "type": "ChoiceGroup", "props": { "label": "Pick", "value": "/pick", "options": [{ "id": "a", "label": { "$ref": "account:gas" } }] } }),
    );
    if let Ok(surface) = validator.show_ui(&input) {
        let summary = surface.summary("s1", &catalogs, Locale::En);
        assert!(!summary.contains(SECRET), "{summary}");
        let resolved = validator.resolve_refs(&surface.components, Operation::Show);
        assert!(resolved.is_ok(), "{resolved:?}");
    }
}

#[test]
fn rule6_the_model_cannot_write_handles_or_skip_confirmation() {
    let catalogs = CatalogSet::builtin();
    rejected(
        &catalogs,
        &single(
            json!({ "id": "b", "type": "Button", "props": { "label": "x", "on_press": { "action_id": "act_0000", "confirm": false } } }),
        ),
    );
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    let attempt = single(
        json!({ "id": "b", "type": "Button", "props": { "label": "x", "on_press": { "action": "pay_bill", "args": { "bill": "b1" }, "confirm": false, "requires_confirm": false } } }),
    );
    if let Ok(mut surface) = validator.show_ui(&attempt) {
        let sealed = validate::seal_actions(&mut surface.components, &catalogs, &actions, || {
            "act_1".into()
        });
        assert!(sealed[0].requires_confirm);
        assert_eq!(surface.components[0].props["on_press"]["confirm"], true);
    }
    let mut sheet = validator
        .show_ui(&single(json!({ "id": "c", "type": "ConfirmSheet", "props": { "title": "Pay 1 so'm?", "on_confirm": { "action": "pay_bill", "args": { "bill": "b_big" } } } })))
        .unwrap();
    let sealed = validate::seal_actions(&mut sheet.components, &catalogs, &actions, || {
        "act_2".into()
    });
    assert!(sealed[0].requires_confirm);
    let wire = serde_json::to_string(&sheet.components).unwrap();
    assert!(
        !wire.contains("b_big") && !wire.contains("pay_bill"),
        "{wire}"
    );
}

#[test]
fn rule6_the_client_only_ever_sends_the_handle_back() {
    let mut client = client();
    let batch = client.receive(&open_message(
        "b",
        json!([{ "id": "b", "type": "Button", "props": { "label": "Pay", "on_press": { "action_id": "act_1", "confirm": true } } }]),
        json!({}),
    ));
    assert!(batch.send.is_empty(), "{:?}", batch.send);
    let fired = client
        .fire(
            "s1",
            "b",
            "on_press",
            Some("{\"args\": {\"bill\": \"b_other\"}}"),
        )
        .or_else(|| client.fire("s1", "b", "on_press", None))
        .expect("the button fires");
    assert_eq!(fired["action_id"], "act_1");
    assert!(
        fired.get("args").is_none() && fired.get("action").is_none(),
        "{fired}"
    );
}

#[test]
fn rule7_oversized_input_is_refused_before_json_parsing() {
    let limits = Limits::default();
    let huge = format!("{{{}", "x".repeat(10 * 1024 * 1024));
    let error = protocol::parse_with(&huge, &limits)
        .unwrap_err()
        .to_string();
    assert!(error.contains("max is 262144"), "{error}");
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let problems = validator(&catalogs, &actions, &HouseholdRefs)
        .show_ui_str(&huge)
        .unwrap_err()
        .to_string();
    assert!(problems.contains("max is 262144"), "{problems}");
    let mut client = client();
    let batch = client.receive(&huge);
    assert!(batch.ops.is_empty());
    assert_eq!(batch.send[0]["reason"], "bad_message");
}

#[test]
fn rule7_deep_nesting_is_refused_without_recursion() {
    let limits = Limits::default();
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    for (open, close) in [("[", "]"), ("{\"a\":", "}")] {
        let depth = 200_000 / open.len();
        let deep = format!("{}1{}", open.repeat(depth), close.repeat(depth));
        assert!(protocol::parse_with(&deep, &limits).is_err());
        assert!(protocol::parse_hello(&deep, &limits).is_err());
        assert!(validator.show_ui_str(&deep).is_err());
        let wrapped = format!(
            "{{\"v\":1,\"t\":\"ui.open\",\"surface\":\"s1\",\"seq\":1,\"root\":\"a\",\"components\":[],\"data\":{deep}}}"
        );
        let mut client = client();
        let batch = client.receive(&wrapped);
        assert!(batch.ops.is_empty());
    }
}

#[test]
fn rule7_problem_lists_stay_short_for_huge_inputs() {
    let catalogs = CatalogSet::builtin();
    let mut props = serde_json::Map::new();
    props.insert("text".into(), json!("x"));
    for n in 0..5000 {
        props.insert(format!("p{n}"), json!(n));
    }
    let text = rejected(
        &catalogs,
        &single(json!({ "id": "t", "type": "Text", "props": props })),
    );
    assert!(text.len() < 8 * 1024, "{} bytes", text.len());
    let wide: Vec<String> = (0..1001)
        .map(|n| format!("https://ok.example/{n}"))
        .collect();
    let text = rejected(
        &extended(),
        &single(
            json!({ "id": "g", "type": "acme:Gallery", "props": { "meta": { "icons": wide } } }),
        ),
    );
    assert!(text.contains("1000"), "{text}");
    let long = "ж".repeat(4097);
    rejected(
        &catalogs,
        &single(json!({ "id": "t", "type": "Text", "props": { "text": long } })),
    );
}

#[test]
fn malformed_json_is_reported_and_never_panics() {
    let limits = Limits::default();
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    let inputs = [
        "",
        " ",
        "{",
        "}",
        "null",
        "[]",
        "\"hello\"",
        "\u{FEFF}{\"v\":1,\"t\":\"user.say\",\"text\":\"x\"}",
        "{\"v\":1,\"t\":\"ui.resync\",\"surface\":\"s1\"} trailing",
        "{\"v\":NaN,\"t\":\"ui.resync\"}",
        "{\"v\":1e999,\"t\":\"ui.resync\",\"surface\":\"s1\"}",
        "{\"v\":1,\"t\":\"ui.resync\",\"surface\":\"\\ud800\"}",
        "{\"v\":1,\"t\":\"ui.resync\",\"surface\":\"\\x41\"}",
        "{\"v\":1,\"t\":\"ui.resync\",\"surface\":\"a\u{0}b\"}",
        "{\"v\":1,\"t\":\"ui.resync\",\"surface\":\"a\nb\"}",
        "{'v':1,'t':'ui.resync'}",
        "{\"v\":1,\"t\":\"ui.resync\",\"surface\":\"s1\",}",
    ];
    for input in inputs {
        assert!(
            protocol::parse_with(input, &limits).is_err(),
            "accepted {input:?}"
        );
        assert!(validator.show_ui_str(input).is_err(), "accepted {input:?}");
        let mut client = client();
        let batch = client.receive(input);
        assert!(batch.ops.is_empty(), "{input:?}");
    }
}

#[test]
fn duplicate_keys_cannot_smuggle_a_url_past_the_checks() {
    let catalogs = CatalogSet::builtin();
    let actions = actions();
    let validator = validator(&catalogs, &actions, &HouseholdRefs);
    let tool_input = r#"{"root":"i","components":[{"id":"i","type":"Image","props":{"src":"https://ok.example/a.png","alt":"x","src":"javascript:alert(1)"}}]}"#;
    if let Ok(surface) = validator.show_ui_str(tool_input) {
        let wire = serde_json::to_string(&surface.components).unwrap();
        assert!(!wire.contains("javascript"), "{wire}");
    }
    let message = r#"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"i","components":[{"id":"i","type":"Image","props":{"src":"https://ok.example/a.png","alt":"x","src":"javascript:alert(1)"}}],"data":{}}"#;
    let mut client = client();
    let batch = client.receive(message).to_value();
    assert!(!batch["ops"].to_string().contains("javascript"), "{batch}");
}
