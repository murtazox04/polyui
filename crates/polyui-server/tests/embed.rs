use futures_util::future::BoxFuture;
use polyui_core::protocol::{self, Hello};
use polyui_core::{ActionRegistry, Client, Locale, PropKind, RefResolver, RenderOp};
use polyui_server::bills::{BillsHost, HOUSEHOLD_USER, TARIFF_APP_HTML, TARIFF_APP_URI};
use polyui_server::{
    ActionCall, ActionOutcome, Agent, ConfirmText, Host, ScriptedProvider, ScriptedReply, Session,
    UiResource, User,
};
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

const MIME: &str = "text/html;profile=mcp-app";

struct AppHost {
    inner: BillsHost,
    html: Option<String>,
    reads: AtomicUsize,
    lookups: AtomicUsize,
}

impl AppHost {
    fn new(inner: BillsHost) -> Self {
        Self {
            inner,
            html: None,
            reads: AtomicUsize::new(0),
            lookups: AtomicUsize::new(0),
        }
    }

    fn tariff() -> Self {
        Self::new(BillsHost::new().with_tariff_app())
    }

    fn serving(html: String) -> Self {
        Self {
            html: Some(html),
            ..Self::tariff()
        }
    }
}

struct CountingRefs<'a> {
    inner: Box<dyn RefResolver + 'a>,
    lookups: &'a AtomicUsize,
}

impl RefResolver for CountingRefs<'_> {
    fn source_kind(&self, source: &str) -> Option<PropKind> {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        self.inner.source_kind(source)
    }

    fn resolve(&self, source: &str, key: &str) -> Result<Value, String> {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        self.inner.resolve(source, key)
    }
}

impl Host for AppHost {
    fn system_prompt(&self, user: &User, locale: Locale) -> String {
        self.inner.system_prompt(user, locale)
    }

    fn actions(&self) -> &dyn ActionRegistry {
        self.inner.actions()
    }

    fn refs<'a>(&'a self, user: &'a User) -> Box<dyn RefResolver + 'a> {
        Box::new(CountingRefs {
            inner: self.inner.refs(user),
            lookups: &self.lookups,
        })
    }

    fn confirm_text(
        &self,
        user: &User,
        action: &str,
        args: &Map<String, Value>,
        locale: Locale,
    ) -> ConfirmText {
        self.inner.confirm_text(user, action, args, locale)
    }

    fn run_action<'a>(
        &'a self,
        user: &'a User,
        call: ActionCall<'a>,
    ) -> BoxFuture<'a, ActionOutcome> {
        self.inner.run_action(user, call)
    }

    fn catalogs(&self) -> polyui_core::CatalogSet {
        self.inner.catalogs()
    }

    fn ui_resource<'a>(
        &'a self,
        user: &'a User,
        uri: &'a str,
    ) -> BoxFuture<'a, Option<UiResource>> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        match &self.html {
            Some(html) => Box::pin(async move {
                Some(UiResource::new(
                    json!({ "uri": uri, "mimeType": MIME, "text": html }),
                ))
            }),
            None => self.inner.ui_resource(user, uri),
        }
    }
}

fn hello_text(catalogs: &[&str], features: &[&str]) -> String {
    json!({ "v": 1, "t": "hello", "catalogs": catalogs, "modalities": ["screen"], "features": features, "locale": "en" })
        .to_string()
}

fn hello(catalogs: &[&str], features: &[&str]) -> Hello {
    protocol::parse_hello(&hello_text(catalogs, features), &Default::default()).unwrap()
}

const WITH_EMBED: [&str; 3] = ["core@1", "chart@1", "embed@1"];
const WITHOUT_EMBED: [&str; 2] = ["core@1", "chart@1"];

fn start(
    host: Arc<AppHost>,
    provider: Arc<ScriptedProvider>,
    catalogs: &[&str],
    features: &[&str],
) -> (Arc<Session>, Receiver<Value>) {
    Agent::new(host, provider).open_session(hello(catalogs, features), User::new(HOUSEHOLD_USER))
}

async fn say(session: &Session, text: &str) {
    session
        .receive(&json!({ "v": 1, "t": "user.say", "text": text }).to_string())
        .await;
}

async fn until_end(outgoing: &mut Receiver<Value>) -> Vec<Value> {
    let mut seen = Vec::new();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), outgoing.recv())
            .await
            .expect("timed out waiting for a server message")
            .expect("the session closed");
        let end = message["t"] == "agent.done" || message["t"] == "agent.error";
        seen.push(message);
        if end {
            return seen;
        }
    }
}

fn embed(id: &str, uri: &str) -> Value {
    json!({ "id": id, "type": "embed:Embed", "props": { "resource": uri, "title": "Tariff calculator", "height": "large", "on_message": { "action": "tariff_message" } } })
}

fn tariff_card(uri: &str) -> Value {
    json!({
        "root": "card",
        "components": [
            { "id": "card", "type": "Card", "props": { "title": "Internet tariffs" }, "children": ["calc", "note"] },
            embed("calc", uri),
            { "id": "note", "type": "Text", "props": { "text": "Move the sliders to compare." } }
        ]
    })
}

fn of_type<'a>(messages: &'a [Value], kind: &str) -> Vec<&'a Value> {
    messages
        .iter()
        .filter(|message| message["t"] == kind)
        .collect()
}

fn tool_results(provider: &ScriptedProvider) -> Vec<Value> {
    provider
        .requests()
        .iter()
        .flat_map(|request| request.messages.iter())
        .filter(|message| message["role"] == "user")
        .filter_map(|message| message["content"].as_array())
        .flatten()
        .filter(|block| block["type"] == "tool_result")
        .cloned()
        .collect()
}

fn find<'a>(components: &'a Value, id: &str) -> Option<&'a Value> {
    components
        .as_array()?
        .iter()
        .find(|component| component["id"] == id)
}

#[tokio::test]
async fn the_tariff_app_reaches_the_client_and_stays_out_of_the_model() {
    let host = Arc::new(AppHost::tariff());
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(ScriptedReply::new().tool("t1", "show_ui", tariff_card(TARIFF_APP_URI)));
    provider.reply(ScriptedReply::new().text("Here is the calculator."));
    provider.reply(ScriptedReply::new().text("I will switch you to Oila 100."));
    let (session, mut outgoing) = start(host.clone(), provider.clone(), &WITH_EMBED, &[]);
    assert_eq!(session.welcome()["catalogs"], json!(WITH_EMBED));
    assert!(
        Value::Array(session.tools().to_vec())
            .to_string()
            .contains("embed:Embed")
    );
    assert!(session.system_prompt().contains(TARIFF_APP_URI));

    say(&session, "Which internet tariff should we take?").await;
    let messages = until_end(&mut outgoing).await;
    let open = of_type(&messages, "ui.open")[0].clone();
    let calc = find(&open["components"], "calc").unwrap();
    assert_eq!(calc["props"]["app"]["resource"]["text"], TARIFF_APP_HTML);
    assert_eq!(calc["props"]["app"]["resource"]["mimeType"], MIME);
    assert_eq!(
        calc["props"]["app"]["tool_result"]["structuredContent"]["offers"][1]["id"],
        "oila_100"
    );
    assert_eq!(host.reads.load(Ordering::Relaxed), 1);

    let results = tool_results(&provider);
    assert_eq!(results.len(), 1);
    assert!(results[0].get("is_error").is_none());
    let summary = results[0]["content"].as_str().unwrap();
    assert!(
        summary.contains("embed:Embed \"Tariff calculator\""),
        "{summary}"
    );
    assert!(!summary.contains("<!doctype"));
    assert!(!summary.contains(TARIFF_APP_URI));
    let surface = open["surface"].as_str().unwrap();
    let model = session.model_summary(surface).unwrap();
    assert!(!model.contains("<!doctype") && !model.contains("\"app\""));

    let mut client = Client::new(&hello_text(&WITH_EMBED, &[])).unwrap();
    client.receive(&session.welcome().to_string());
    let batch = client.receive(&open.to_string());
    let drawn = batch.ops.iter().find_map(|op| match op {
        RenderOp::Create { id, props, .. } if id == "calc" => Some(props.clone()),
        _ => None,
    });
    let drawn = drawn.expect("the client draws the embed");
    assert_eq!(drawn["app"]["resource"]["text"], TARIFF_APP_HTML);

    let action = client
        .fire(
            surface,
            "calc",
            "on_message",
            Some(r#"{"text":"Please switch our internet to Oila 100."}"#),
        )
        .expect("on_message fires a ui.action");
    session.receive(&action.to_string()).await;
    let after = until_end(&mut outgoing).await;
    assert_eq!(of_type(&after, "ui.action.result")[0]["status"], "ok");
    let report =
        provider.requests().last().unwrap().messages.last().unwrap()["content"].to_string();
    assert!(report.contains("[UI event]"));
    assert!(report.contains("Please switch our internet to Oila 100."));
}

#[tokio::test]
async fn clients_without_embed_are_never_offered_it() {
    let host = Arc::new(AppHost::tariff());
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(ScriptedReply::new().tool("t1", "show_ui", tariff_card(TARIFF_APP_URI)));
    provider.reply(ScriptedReply::new().text("I can only describe the tariffs here."));
    let (session, mut outgoing) = start(host.clone(), provider.clone(), &WITHOUT_EMBED, &[]);
    assert_eq!(session.welcome()["catalogs"], json!(WITHOUT_EMBED));
    assert!(
        !Value::Array(session.tools().to_vec())
            .to_string()
            .contains("embed:Embed")
    );
    say(&session, "Which tariff?").await;
    let messages = until_end(&mut outgoing).await;
    assert!(of_type(&messages, "ui.open").is_empty());
    let results = tool_results(&provider);
    assert_eq!(results[0]["is_error"], true);
    assert!(
        results[0]["content"]
            .as_str()
            .unwrap()
            .contains("embed:Embed")
    );
    assert_eq!(host.reads.load(Ordering::Relaxed), 0);

    let plain = Arc::new(AppHost::new(BillsHost::new()));
    let (session, mut outgoing) = start(plain, Arc::new(ScriptedProvider::new()), &WITH_EMBED, &[]);
    assert_eq!(session.welcome()["catalogs"], json!(WITHOUT_EMBED));
    assert!(
        !Value::Array(session.tools().to_vec())
            .to_string()
            .contains("embed:Embed")
    );
    assert!(outgoing.try_recv().is_err());
}

#[tokio::test]
async fn unknown_resources_and_oversized_apps_are_problems_for_the_model() {
    let host = Arc::new(AppHost::tariff());
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(ScriptedReply::new().tool("t1", "show_ui", tariff_card("ui://bills/weather")));
    provider.reply(ScriptedReply::new().text("That app does not exist."));
    let (session, mut outgoing) = start(host, provider.clone(), &WITH_EMBED, &[]);
    say(&session, "Show the weather app").await;
    let messages = until_end(&mut outgoing).await;
    assert!(of_type(&messages, "ui.open").is_empty());
    let results = tool_results(&provider);
    assert_eq!(results[0]["is_error"], true);
    assert!(
        results[0]["content"].as_str().unwrap().contains(
            "components[1] id \"calc\" prop \"resource\": ui://bills/weather is not an MCP App this host offers."
        ),
        "{}",
        results[0]["content"]
    );

    let host = Arc::new(AppHost::serving(
        "x".repeat(150 * 1024) + &"y".repeat(150 * 1024),
    ));
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(ScriptedReply::new().tool("t1", "show_ui", tariff_card(TARIFF_APP_URI)));
    provider.reply(ScriptedReply::new().text("Too big."));
    let (session, mut outgoing) = start(host, provider.clone(), &WITH_EMBED, &[]);
    say(&session, "Show the calculator").await;
    let messages = until_end(&mut outgoing).await;
    assert!(of_type(&messages, "ui.open").is_empty());
    let results = tool_results(&provider);
    assert_eq!(results[0]["is_error"], true);
    assert!(
        results[0]["content"]
            .as_str()
            .unwrap()
            .contains("is too large to send to the client"),
        "{}",
        results[0]["content"]
    );
}

#[tokio::test]
async fn too_many_different_apps_are_refused_before_any_read() {
    let host = Arc::new(AppHost::tariff());
    let provider = Arc::new(ScriptedProvider::new());
    let ids: Vec<String> = (0..9).map(|index| format!("e{index}")).collect();
    let mut components = vec![
        json!({ "id": "card", "type": "Card", "props": { "title": "Apps" }, "children": ids }),
    ];
    components.extend(
        ids.iter()
            .enumerate()
            .map(|(index, id)| embed(id, &format!("ui://bills/app{index}"))),
    );
    provider.reply(ScriptedReply::new().tool(
        "t1",
        "show_ui",
        json!({ "root": "card", "components": components }),
    ));
    provider.reply(ScriptedReply::new().text("Fewer then."));
    let (session, mut outgoing) = start(host.clone(), provider.clone(), &WITH_EMBED, &[]);
    say(&session, "Show every app").await;
    until_end(&mut outgoing).await;
    let results = tool_results(&provider);
    assert!(
        results[0]["content"]
            .as_str()
            .unwrap()
            .contains("more than 8 different MCP Apps")
    );
    assert_eq!(host.reads.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn update_ui_patches_an_app_in() {
    let host = Arc::new(AppHost::tariff());
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(ScriptedReply::new().tool(
        "t1",
        "show_ui",
        json!({ "surface": "tariffs", "root": "card", "components": [{ "id": "card", "type": "Card", "props": { "title": "Internet tariffs" } }] }),
    ));
    provider.reply(ScriptedReply::new().tool(
        "t2",
        "update_ui",
        json!({ "surface": "tariffs", "upsert": [
            { "id": "card", "type": "Card", "props": { "title": "Internet tariffs" }, "children": ["calc"] },
            embed("calc", TARIFF_APP_URI)
        ] }),
    ));
    provider.reply(ScriptedReply::new().text("Added the calculator."));
    let (session, mut outgoing) = start(host, provider.clone(), &WITH_EMBED, &[]);
    say(&session, "Add the calculator").await;
    let messages = until_end(&mut outgoing).await;
    let patch = of_type(&messages, "ui.patch")[0];
    let calc = find(&patch["upsert"], "calc").unwrap();
    assert_eq!(calc["props"]["app"]["resource"]["uri"], TARIFF_APP_URI);
    let results = tool_results(&provider);
    assert!(
        results
            .iter()
            .all(|result| result.get("is_error").is_none())
    );
    assert!(
        !results[1]["content"]
            .as_str()
            .unwrap()
            .contains("<!doctype")
    );
}

#[tokio::test]
async fn streamed_surfaces_carry_the_app_and_read_it_once() {
    let host = Arc::new(AppHost::tariff());
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(
        ScriptedReply::new()
            .tool("t1", "show_ui", tariff_card(TARIFF_APP_URI))
            .paced(24, Duration::ZERO),
    );
    provider.reply(ScriptedReply::new().text("Here it is."));
    let (session, mut outgoing) = start(host.clone(), provider.clone(), &WITH_EMBED, &["stream"]);
    say(&session, "Calculator please").await;
    let messages = until_end(&mut outgoing).await;
    let drawn = messages
        .iter()
        .filter(|message| message["t"] == "ui.open" || message["t"] == "ui.patch")
        .flat_map(|message| {
            [&message["components"], &message["upsert"]]
                .into_iter()
                .filter_map(|list| find(list, "calc"))
                .collect::<Vec<_>>()
        })
        .next()
        .expect("the embed was streamed");
    assert_eq!(drawn["props"]["app"]["resource"]["text"], TARIFF_APP_HTML);
    assert!(of_type(&messages, "ui.close").is_empty());
    assert_eq!(host.reads.load(Ordering::Relaxed), 1);
}

fn many_refs(count: usize) -> Value {
    let ids: Vec<String> = (0..count).map(|index| format!("t{index}")).collect();
    let mut components = vec![json!({ "id": "list", "type": "List", "children": ids })];
    components.extend(ids.iter().map(
        |id| json!({ "id": id, "type": "Text", "props": { "text": { "$ref": "account:gas" } } }),
    ));
    json!({ "root": "list", "components": components })
}

async fn streamed_lookups(count: usize) -> usize {
    let host = Arc::new(AppHost::new(BillsHost::new()));
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(
        ScriptedReply::new()
            .tool("t1", "show_ui", many_refs(count))
            .paced(32, Duration::ZERO),
    );
    provider.reply(ScriptedReply::new().text("Done."));
    let (session, mut outgoing) = start(host.clone(), provider, &WITHOUT_EMBED, &["stream"]);
    say(&session, "Show the accounts").await;
    let messages = until_end(&mut outgoing).await;
    assert!(!of_type(&messages, "ui.patch").is_empty());
    assert!(of_type(&messages, "ui.close").is_empty());
    host.lookups.load(Ordering::Relaxed)
}

#[tokio::test]
async fn streaming_checks_each_component_a_fixed_number_of_times() {
    let small = streamed_lookups(100).await;
    let large = streamed_lookups(200).await;
    println!("ref lookups while streaming: {small} for 100 components, {large} for 200");
    assert!(large <= 8 * 200, "{large} ref lookups for 200 components");
    assert!(large <= 2 * small + 16, "{small} for 100, {large} for 200");
}
