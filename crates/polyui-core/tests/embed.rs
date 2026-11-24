use polyui_core::client::{MAX_APP_STRING_CHARS, MCP_APP_MIME_TYPE};
use polyui_core::summary::summarize;
use polyui_core::validate::{ActionSpec, ActionTable, Problems, StaticRefs, Surface, Validator};
use polyui_core::{CatalogSet, Client, Locale, RenderBatch};
use serde_json::{Value, json};

const URI: &str = "ui://weather/forecast";

fn actions() -> ActionTable {
    ActionTable::new().with("ask_agent", ActionSpec::new(false))
}

fn show(catalogs: &CatalogSet, components: Value) -> Result<Surface, Problems> {
    let actions = actions();
    let refs = StaticRefs::new();
    Validator::new(catalogs, &actions, &refs)
        .show_ui(&json!({ "root": "app", "components": components }))
}

fn embed(props: Value) -> Value {
    json!([{ "id": "app", "type": "embed:Embed", "props": props }])
}

fn problems(catalogs: &CatalogSet, components: Value) -> String {
    match show(catalogs, components.clone()) {
        Ok(_) => panic!("accepted {components}"),
        Err(problems) => problems.to_string(),
    }
}

#[test]
fn the_model_only_sees_embed_when_the_host_turns_it_on() {
    let message = problems(
        &CatalogSet::builtin(),
        embed(json!({ "resource": URI, "title": "Forecast" })),
    );
    assert!(message.contains("embed:Embed"), "{message}");
    let enabled = CatalogSet::builtin().with_embed();
    let surface = show(
        &enabled,
        embed(json!({ "resource": URI, "title": "Forecast", "on_message": { "action": "ask_agent" } })),
    )
    .unwrap_or_else(|problems| panic!("{problems}"));
    assert_eq!(surface.components[0].props["resource"], json!(URI));
    let summary = summarize("s1", &surface, &enabled, Locale::En);
    assert!(summary.contains("Forecast"), "{summary}");
    assert!(!summary.contains("ui://"), "{summary}");
}

#[test]
fn the_resource_must_be_a_ui_uri_written_literally() {
    let enabled = CatalogSet::builtin().with_embed();
    for resource in [
        json!("https://example.com/app.html"),
        json!("ui:forecast"),
        json!("ui://"),
        json!("javascript:alert(1)"),
        json!("data:text/html,<script>alert(1)</script>"),
        json!({ "$bind": "/app" }),
        json!(42),
    ] {
        let message = problems(
            &enabled,
            embed(json!({ "resource": resource, "title": "Forecast" })),
        );
        assert!(message.contains("resource"), "{resource}: {message}");
    }
    let message = problems(&enabled, embed(json!({ "resource": URI })));
    assert!(message.contains("title"), "{message}");
}

#[test]
fn the_model_cannot_write_the_server_only_app_prop_or_markup() {
    let enabled = CatalogSet::builtin().with_embed();
    for name in ["app", "html", "srcdoc", "sandbox", "csp"] {
        let mut props = json!({ "resource": URI, "title": "Forecast" });
        props[name] = json!({
            "resource": { "uri": URI, "mimeType": MCP_APP_MIME_TYPE, "text": "<script>alert(1)</script>" }
        });
        let message = problems(&enabled, embed(props));
        assert!(message.contains(name), "{name}: {message}");
    }
}

fn client(catalogs: &[&str]) -> Client {
    let hello = json!({ "v": 1, "t": "hello", "catalogs": catalogs, "modalities": ["screen"], "locale": "en" });
    Client::new(&hello.to_string()).unwrap_or_else(|error| panic!("{error}"))
}

fn app(uri: &str, mime: &str, html: &str) -> Value {
    json!({
        "resource": { "uri": uri, "mimeType": mime, "text": html, "_meta": { "ui": { "csp": { "connectDomains": ["https://api.example.com"] } } } },
        "tool_input": { "city": "Tashkent" },
        "tool_result": { "content": [{ "type": "text", "text": "Sunny" }] }
    })
}

fn open(app: Value) -> String {
    json!({
        "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "app",
        "components": [{ "id": "app", "type": "embed:Embed", "props": { "resource": URI, "title": "Forecast", "on_message": { "action_id": "act_ask" }, "app": app } }],
        "data": {}
    })
    .to_string()
}

fn created(batch: &RenderBatch) -> Value {
    assert!(batch.send.is_empty(), "unexpected {:?}", batch.send);
    batch
        .to_value()
        .pointer("/ops/0")
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn the_client_core_passes_a_checked_app_to_the_renderer() {
    let mut client = client(&["core@1", "embed@1"]);
    let html = format!("<!doctype html><p>{}</p>", "a".repeat(150_000));
    let op = created(&client.receive(&open(app(URI, MCP_APP_MIME_TYPE, &html))));
    assert_eq!(op["type"], json!("embed:Embed"));
    assert_eq!(op["props"]["app"]["resource"]["text"], json!(html));
    assert_eq!(op["props"]["app"]["tool_input"]["city"], json!("Tashkent"));
    assert_eq!(op["props"]["height"], json!("medium"));
    assert_eq!(op["a11y"], json!({ "role": "figure", "label": "Forecast" }));
}

#[test]
fn the_client_core_drops_apps_that_do_not_fit() {
    let cases = [
        app(URI, "text/html", "<p>x</p>"),
        app(URI, "text/html;profile=mcp-app;x=1", "<p>x</p>"),
        app("ui://weather/other", MCP_APP_MIME_TYPE, "<p>x</p>"),
        json!({ "resource": { "uri": URI, "mimeType": MCP_APP_MIME_TYPE, "blob": "PHA+eDwvcD4=" } }),
        json!({ "resource": { "uri": URI, "mimeType": MCP_APP_MIME_TYPE, "text": "<p>x</p>" }, "tool_input": "city" }),
        json!("<script>alert(1)</script>"),
    ];
    for case in cases {
        let mut client = client(&["core@1", "embed@1"]);
        let op = created(&client.receive(&open(case.clone())));
        assert_eq!(op["type"], json!("embed:Embed"), "{case}");
        assert!(op["props"].get("app").is_none(), "{case}");
    }
}

#[test]
fn app_html_has_its_own_ceiling() {
    let mut client = client(&["core@1", "embed@1"]);
    let html = "a".repeat(MAX_APP_STRING_CHARS + 1);
    let batch = client.receive(&open(app(URI, MCP_APP_MIME_TYPE, &html)));
    assert!(batch.ops.is_empty());
    assert_eq!(batch.send[0]["reason"], json!("bad_message"));
    let mut client = self::client(&["core@1", "embed@1"]);
    let message = json!({
        "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "t",
        "components": [{ "id": "t", "type": "Text", "props": { "text": "hi", "app": { "resource": { "text": "a".repeat(5000) } } } }],
        "data": {}
    });
    let batch = client.receive(&message.to_string());
    assert!(
        batch.ops.is_empty(),
        "only embed:Embed gets the larger limit"
    );
}

#[test]
fn a_client_that_did_not_declare_embed_draws_the_title() {
    let mut client = client(&["core@1"]);
    let mut message: Value =
        serde_json::from_str(&open(app(URI, MCP_APP_MIME_TYPE, "<p>x</p>"))).unwrap_or(Value::Null);
    message["components"][0]["alt"] = json!("Forecast");
    let batch = client.receive(&message.to_string());
    assert_eq!(batch.send[0]["reason"], json!("unknown_component"));
    let op = batch
        .to_value()
        .pointer("/ops/0")
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(op["type"], json!("Text"));
    assert_eq!(op["props"]["text"], json!("Forecast"));
}

#[test]
fn app_messages_fire_the_on_message_action_with_detail() {
    let mut client = client(&["core@1", "embed@1"]);
    created(&client.receive(&open(app(URI, MCP_APP_MIME_TYPE, "<p>x</p>"))));
    let action = client
        .fire(
            "s1",
            "app",
            "on_message",
            Some(r#"{"text":"Show Samarkand too"}"#),
        )
        .unwrap_or_else(|| panic!("no action"));
    assert_eq!(action["t"], json!("ui.action"));
    assert_eq!(action["action_id"], json!("act_ask"));
    assert_eq!(action["detail"], json!({ "text": "Show Samarkand too" }));
    assert!(client.fire("s1", "app", "resource", None).is_none());
}
