use polyui_core::Client;
use polyui_core::protocol::{self, Hello};
use polyui_core::tools::{STRICT_LIMITS, StrictCost, ToolProfile};
use polyui_server::bills::{BillsHost, SERVICES, mask};
use polyui_server::{Agent, AgentConfig, ScriptedProvider, ScriptedReply, Session, StopReason};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

const GAS_BILL: &str = "b_gas_2611";

struct Harness {
    provider: Arc<ScriptedProvider>,
    host: Arc<BillsHost>,
    session: Arc<Session>,
    outgoing: Receiver<Value>,
}

fn hello(catalogs: &[&str]) -> Hello {
    let text = json!({ "v": 1, "t": "hello", "catalogs": catalogs, "modalities": ["screen"], "locale": "en" })
        .to_string();
    protocol::parse_hello(&text, &Default::default()).unwrap()
}

fn start_with(provider: ScriptedProvider, config: AgentConfig, catalogs: &[&str]) -> Harness {
    let provider = Arc::new(provider);
    let host = Arc::new(BillsHost::new());
    let agent = Agent::new(host.clone(), provider.clone()).with_config(config);
    let user = polyui_server::User::new(polyui_server::bills::HOUSEHOLD_USER);
    let (session, outgoing) = agent.open_session(hello(catalogs), user);
    Harness {
        provider,
        host,
        session,
        outgoing,
    }
}

fn start(provider: ScriptedProvider) -> Harness {
    start_with(provider, AgentConfig::default(), &["core@1", "chart@1"])
}

impl Harness {
    async fn say(&self, text: &str) {
        self.session
            .receive(&json!({ "v": 1, "t": "user.say", "text": text }).to_string())
            .await;
    }

    async fn act(&self, surface: &str, action_id: &str, nonce: Option<&str>) {
        let mut message = json!({ "v": 1, "t": "ui.action", "surface": surface, "action_id": action_id, "source": "tap" });
        if let Some(nonce) = nonce {
            message["confirm"] = json!(nonce);
        }
        self.session.receive(&message.to_string()).await;
    }

    async fn next(&mut self) -> Value {
        tokio::time::timeout(Duration::from_secs(5), self.outgoing.recv())
            .await
            .expect("timed out waiting for a server message")
            .expect("the session closed")
    }

    async fn until_end(&mut self) -> Vec<Value> {
        let mut seen = Vec::new();
        loop {
            let message = self.next().await;
            let end = message["t"] == "agent.done" || message["t"] == "agent.error";
            seen.push(message);
            if end {
                return seen;
            }
        }
    }

    fn requests_json(&self) -> String {
        self.provider
            .requests()
            .iter()
            .map(|request| {
                json!({ "system": request.system, "tools": request.tools, "messages": request.messages })
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn of_type<'a>(messages: &'a [Value], kind: &str) -> Vec<&'a Value> {
    messages
        .iter()
        .filter(|message| message["t"] == kind)
        .collect()
}

fn tool_results(request_messages: &[Value]) -> Vec<Value> {
    request_messages
        .iter()
        .filter(|message| message["role"] == "user")
        .filter_map(|message| message["content"].as_array())
        .flatten()
        .filter(|block| block["type"] == "tool_result")
        .cloned()
        .collect()
}

fn bill_surface() -> Value {
    json!({
        "root": "card",
        "components": [
            { "id": "card", "type": "Card", "props": { "title": "Gas bill" }, "children": ["due", "pay"] },
            { "id": "due", "type": "Stat", "props": { "label": "Due", "value": "29 322,19", "unit": "so'm", "hint": { "$ref": "account:gas" } } },
            { "id": "pay", "type": "Button", "props": { "label": "Pay", "tone": "primary", "on_press": { "action": "pay_bill", "args": { "bill": GAS_BILL } } } }
        ]
    })
}

fn handle_of(open: &Value, component: &str, prop: &str) -> String {
    open["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == component)
        .and_then(|item| item["props"][prop]["action_id"].as_str())
        .unwrap()
        .to_string()
}

async fn open_bill(harness: &mut Harness) -> Value {
    harness
        .provider
        .reply(ScriptedReply::new().text("Here is your gas bill.").tool(
            "t1",
            "show_ui",
            bill_surface(),
        ));
    harness
        .provider
        .reply(ScriptedReply::new().text("Tap Pay when ready."));
    harness.say("Show my gas bill and let me pay it").await;
    let messages = harness.until_end().await;
    of_type(&messages, "ui.open")[0].clone()
}

#[tokio::test]
async fn valid_show_ui_becomes_ui_open() {
    let mut harness = start(ScriptedProvider::new());
    let open = open_bill(&mut harness).await;
    assert_eq!(open["surface"], "s1");
    assert_eq!(open["seq"], 1);
    let wire = open.to_string();
    assert!(!wire.contains("pay_bill") && !wire.contains(GAS_BILL) && !wire.contains("$ref"));
    assert!(wire.contains(&mask(SERVICES[0].account_number)));
    assert!(handle_of(&open, "pay", "on_press").starts_with("act_"));

    let mut client = Client::new(
        &json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen"] })
            .to_string(),
    )
    .unwrap();
    let batch = client.receive(&wire);
    assert!(batch.send.is_empty(), "client complained: {:?}", batch.send);

    let requests = harness.provider.requests();
    assert_eq!(requests.len(), 2);
    let results = tool_results(&requests[1].messages);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["is_error"], Value::Null);
    let summary = results[0]["content"].as_str().unwrap();
    assert!(
        summary.starts_with("surface s1 open: Card(2) \"Gas bill\""),
        "{summary}"
    );
    assert_eq!(requests[1].messages[1]["role"], "assistant");
    let strict: Vec<&Value> = requests[0]
        .tools
        .iter()
        .filter(|tool| tool["strict"] == true)
        .collect();
    assert!(strict.iter().any(|tool| tool["name"] == "close_ui"));
    let cost = strict
        .iter()
        .map(|tool| StrictCost::of_tool(tool))
        .fold(StrictCost::default(), StrictCost::plus);
    assert!(cost.within(STRICT_LIMITS), "{cost:?}");
}

#[tokio::test]
async fn invalid_call_gets_problems_and_the_retry_is_shown() {
    let mut harness = start(ScriptedProvider::new());
    harness.provider.reply(ScriptedReply::new().tool(
        "t1",
        "show_ui",
        json!({ "root": "card", "components": [ { "id": "card", "type": "PayButton", "props": {} } ] }),
    ));
    harness
        .provider
        .reply(ScriptedReply::new().tool("t2", "show_ui", bill_surface()));
    harness.provider.reply(ScriptedReply::new().text("Done."));
    harness.say("Show my gas bill").await;
    let messages = harness.until_end().await;
    assert_eq!(of_type(&messages, "ui.open").len(), 1);
    assert_eq!(messages.last().unwrap()["t"], "agent.done");
    let requests = harness.provider.requests();
    assert_eq!(requests.len(), 3);
    let first = &tool_results(&requests[1].messages)[0];
    assert_eq!(first["is_error"], true);
    let text = first["content"].as_str().unwrap();
    assert!(text.starts_with("1 problem, nothing was shown:"), "{text}");
    assert!(text.contains("\"PayButton\" is not in the catalog"));
    let second = &tool_results(&requests[2].messages)[1];
    assert_eq!(second["is_error"], Value::Null);
}

#[tokio::test]
async fn the_tool_profile_changes_the_prompt_but_not_the_checks() {
    let mut harness = start(ScriptedProvider::new());
    assert_eq!(AgentConfig::default().tool_profile, ToolProfile::Compact);
    let tools = harness.session.tools();
    assert!(!tools[0]["input_schema"].to_string().contains("anyOf"));
    let description = tools[0]["description"].as_str().unwrap();
    assert!(
        description.contains("\npay_bill(bill: b_gas_2611|"),
        "{description}"
    );
    assert!(description.contains("\nchart:Chart("), "{description}");
    harness.provider.reply(ScriptedReply::new().tool(
        "t1",
        "show_ui",
        json!({ "root": "s", "components": [ { "id": "s", "type": "Stat", "props": { "label": "Gas", "value": "29 322", "tone": "loud", "color": "red" } } ] }),
    ));
    harness
        .provider
        .reply(ScriptedReply::new().tool("t2", "show_ui", bill_surface()));
    harness.provider.reply(ScriptedReply::new().text("Done."));
    harness.say("Show my gas bill").await;
    let messages = harness.until_end().await;
    assert_eq!(of_type(&messages, "ui.open").len(), 1);
    let requests = harness.provider.requests();
    let first = &tool_results(&requests[1].messages)[0];
    assert_eq!(first["is_error"], true);
    let text = first["content"].as_str().unwrap();
    assert!(text.starts_with("2 problems, nothing was shown:"), "{text}");
    assert!(
        text.contains("\"tone\"") && text.contains("\"color\""),
        "{text}"
    );

    let full = start_with(
        ScriptedProvider::new(),
        AgentConfig {
            tool_profile: ToolProfile::Full,
            ..AgentConfig::default()
        },
        &["core@1", "chart@1"],
    );
    assert!(
        full.session.tools()[0]["input_schema"]
            .to_string()
            .contains("anyOf")
    );

    let host = Arc::new(BillsHost::new());
    let subset = Agent::new(host, Arc::new(ScriptedProvider::new())).with_tool_types(vec![
        "Card".into(),
        "Stat".into(),
        "Button".into(),
    ]);
    let (session, outgoing) = subset.open_session(
        hello(&["core@1", "chart@1"]),
        polyui_server::User::new(polyui_server::bills::HOUSEHOLD_USER),
    );
    drop(outgoing);
    assert_eq!(
        session.tools()[0]["input_schema"]["properties"]["components"]["items"]["properties"]["type"]
            ["enum"],
        json!(["Card", "Stat", "Button"])
    );
    assert!(
        !session.tools()[0]["description"]
            .as_str()
            .unwrap()
            .contains("chart:Chart(")
    );
}

#[tokio::test]
async fn retry_limit_sends_a_plain_text_fallback() {
    let mut harness = start(ScriptedProvider::new());
    let broken = json!({ "root": "missing", "components": [ { "id": "a", "type": "Text", "props": { "text": "x" } } ] });
    harness
        .provider
        .reply(ScriptedReply::new().tool("t1", "show_ui", broken.clone()));
    harness
        .provider
        .reply(ScriptedReply::new().broken_tool("t2", "show_ui", "{\"root\": \"a"));
    harness
        .provider
        .reply(ScriptedReply::new().tool("t3", "show_ui", broken));
    harness
        .provider
        .reply(ScriptedReply::new().text("never reached"));
    harness.say("Show something").await;
    let messages = harness.until_end().await;
    assert!(of_type(&messages, "ui.open").is_empty());
    let deltas = of_type(&messages, "agent.delta");
    assert_eq!(deltas.len(), 1);
    assert!(
        deltas[0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Sorry, I could not show that")
    );
    assert_eq!(messages.last().unwrap()["t"], "agent.done");
    assert_eq!(harness.provider.requests().len(), 3);
    assert_eq!(harness.provider.remaining(), 1);

    harness.provider.reply(ScriptedReply::new());
    harness.say("ok").await;
    harness.until_end().await;
    let requests = harness.provider.requests();
    let history = &requests[3].messages;
    let results = tool_results(history);
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|result| result["is_error"] == true));
    assert!(
        results[1]["content"]
            .as_str()
            .unwrap()
            .contains("INVALID_JSON")
    );
    assert!(
        results[2]["content"]
            .as_str()
            .unwrap()
            .contains("Do not call show_ui again")
    );
    let last = history.last().unwrap();
    assert_eq!(last["role"], "user");
    assert_eq!(last["content"][0]["type"], "tool_result");
    assert_eq!(last["content"][1]["text"], "ok");
}

#[tokio::test]
async fn cut_off_replies_are_never_acted_on() {
    let mut harness = start(ScriptedProvider::new());
    harness.provider.reply(
        ScriptedReply::new()
            .tool("t1", "show_ui", bill_surface())
            .stop(StopReason::MaxTokens),
    );
    harness.say("Show my gas bill").await;
    let messages = harness.until_end().await;
    assert!(of_type(&messages, "ui.open").is_empty());
    assert_eq!(messages.last().unwrap()["t"], "agent.error");
}

#[tokio::test]
async fn confirm_flow_runs_the_action_once_with_host_text() {
    let mut harness = start(ScriptedProvider::new());
    let open = open_bill(&mut harness).await;
    let action_id = handle_of(&open, "pay", "on_press");

    harness.act("s1", &action_id, None).await;
    let confirm = harness.next().await;
    assert_eq!(confirm["t"], "ui.confirm");
    assert_eq!(confirm["title"], "Pay: Hududgaz Toshkent");
    assert_eq!(confirm["body"], "29 322,19 so'm, card **** 4471");
    assert_eq!(confirm["expires_ms"], 60000);
    assert!(!harness.host.is_paid(GAS_BILL));

    harness
        .provider
        .reply(ScriptedReply::new().text("Paid. Thank you!"));
    let nonce = confirm["nonce"].as_str().unwrap().to_string();
    harness.act("s1", &action_id, Some(&nonce)).await;
    let result = harness.next().await;
    assert_eq!(result["t"], "ui.action.result");
    assert_eq!(result["status"], "ok");
    assert!(harness.host.is_paid(GAS_BILL));
    let turn = harness.until_end().await;
    assert_eq!(turn[0]["text"], "Paid. Thank you!");
    let requests = harness.provider.requests();
    let report = requests.last().unwrap().messages.last().unwrap()["content"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|block| block["text"].as_str())
        .unwrap()
        .to_string();
    assert!(report.starts_with("[UI event]"), "{report}");
    assert!(report.contains("pay_bill") && report.contains("Result: ok."));
    assert!(report.contains("On screen now: surface s1 open:"));

    harness.act("s1", &action_id, Some(&nonce)).await;
    let reused = harness.next().await;
    assert_eq!(reused["t"], "ui.action.result");
    assert_eq!(reused["status"], "rejected");
}

#[tokio::test]
async fn expired_nonce_is_rejected() {
    let config = AgentConfig {
        confirm_ttl: Duration::from_millis(50),
        ..AgentConfig::default()
    };
    let mut harness = start_with(ScriptedProvider::new(), config, &["core@1", "chart@1"]);
    let open = open_bill(&mut harness).await;
    let action_id = handle_of(&open, "pay", "on_press");
    harness.act("s1", &action_id, None).await;
    let confirm = harness.next().await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    harness
        .act("s1", &action_id, confirm["nonce"].as_str())
        .await;
    let result = harness.next().await;
    assert_eq!(result["status"], "expired");
    assert!(!harness.host.is_paid(GAS_BILL));
    harness.act("s1", &action_id, Some("n_made_up")).await;
    assert_eq!(harness.next().await["status"], "rejected");
    assert!(!harness.host.is_paid(GAS_BILL));
}

#[tokio::test]
async fn tampered_and_stale_handles_are_rejected() {
    let mut harness = start(ScriptedProvider::new());
    let open = open_bill(&mut harness).await;
    let action_id = handle_of(&open, "pay", "on_press");
    let calls = harness.provider.requests().len();

    harness.act("s1", "act_forged000000", None).await;
    assert_eq!(harness.next().await["status"], "rejected");
    harness.act("s9", &action_id, None).await;
    assert_eq!(harness.next().await["status"], "rejected");
    let forged_args = json!({ "v": 1, "t": "ui.action", "surface": "s1", "action_id": action_id, "args": { "bill": "b_power_2611" }, "values": { "/bill": "b_power_2611" } });
    harness.session.receive(&forged_args.to_string()).await;
    let confirm = harness.next().await;
    assert_eq!(confirm["t"], "ui.confirm");
    assert_eq!(confirm["title"], "Pay: Hududgaz Toshkent");

    harness
        .provider
        .reply(ScriptedReply::new().tool("t9", "close_ui", json!({ "surface": "s1" })));
    harness.provider.reply(ScriptedReply::new().text("Closed."));
    harness.say("close it").await;
    let messages = harness.until_end().await;
    assert_eq!(of_type(&messages, "ui.close")[0]["seq"], 2);
    harness
        .act("s1", &action_id, confirm["nonce"].as_str())
        .await;
    assert_eq!(harness.next().await["status"], "rejected");
    assert!(!harness.host.is_paid(GAS_BILL));
    assert!(!harness.host.is_paid("b_power_2611"));
    assert_eq!(harness.provider.requests().len(), calls + 2);
}

#[tokio::test]
async fn ref_values_never_reach_the_model() {
    let mut harness = start(ScriptedProvider::new());
    harness.provider.reply(ScriptedReply::new().tool(
        "t1",
        "show_ui",
        json!({
            "root": "card",
            "components": [
                { "id": "card", "type": "Card", "props": { "title": "Accounts" }, "children": ["gas", "phone", "usage"] },
                { "id": "gas", "type": "Text", "props": { "text": { "$ref": "account:gas" } } },
                { "id": "phone", "type": "Input", "props": { "label": "Phone", "value": "/phone" } },
                { "id": "usage", "type": "Button", "props": { "label": { "$ref": "card:main" }, "on_press": { "action": "show_usage", "args": { "service": "gas" }, "with": ["/phone"] } } }
            ]
        }),
    ));
    harness.provider.reply(ScriptedReply::new().text("Shown."));
    harness.say("Show my accounts").await;
    let messages = harness.until_end().await;
    let open = of_type(&messages, "ui.open")[0].clone();
    let masked = mask(SERVICES[0].account_number);
    assert!(open.to_string().contains(&masked));
    assert!(open.to_string().contains("**** 4471"));

    let action_id = handle_of(&open, "usage", "on_press");
    harness
        .provider
        .reply(ScriptedReply::new().text("Here is the usage."));
    let tap = json!({ "v": 1, "t": "ui.action", "surface": "s1", "action_id": action_id, "values": { "/phone": "+998901234567", "/other": "dropped" }, "source": "tap" });
    harness.session.receive(&tap.to_string()).await;
    assert_eq!(harness.next().await["status"], "ok");
    harness.until_end().await;

    let everything = harness.requests_json();
    for service in SERVICES {
        assert!(!everything.contains(service.account_number));
        assert!(!everything.contains(&mask(service.account_number)));
    }
    assert!(!everything.contains("**** 4471"));
    assert!(everything.contains("+998901234567"));
    assert!(!everything.contains("dropped"));
    assert!(everything.contains("[ref]"));
    assert!(
        harness
            .session
            .model_summary("s1")
            .unwrap()
            .contains("+998901234567")
    );
}

#[tokio::test]
async fn charts_get_a_scene_and_a_spoken_summary() {
    let mut harness = start(ScriptedProvider::new());
    harness.provider.reply(ScriptedReply::new().tool(
        "t1",
        "show_ui",
        json!({
            "root": "chart",
            "components": [
                { "id": "chart", "type": "chart:Chart", "props": { "title": "Gas use", "kind": "bar", "unit": "m3", "rows": [ { "x": "2026-10", "y": 71 }, { "x": "2026-11", "y": 112 } ] } }
            ]
        }),
    ));
    harness
        .provider
        .reply(ScriptedReply::new().text("Usage went up."));
    harness.say("chart my gas").await;
    let messages = harness.until_end().await;
    let open = of_type(&messages, "ui.open")[0];
    let chart = &open["components"][0];
    assert_eq!(chart["type"], "chart:Chart");
    assert!(
        chart["props"]["scene"]["marks"]
            .as_array()
            .is_some_and(|marks| !marks.is_empty())
    );
    assert!(chart["say"].as_str().unwrap().starts_with("Gas use."));
    let summary = harness.session.model_summary("s1").unwrap();
    assert!(!summary.contains("scene"));
}

#[tokio::test]
async fn charts_downgrade_to_text_for_core_only_clients() {
    let mut harness = start_with(ScriptedProvider::new(), AgentConfig::default(), &["core@1"]);
    assert!(
        !harness.session.tools()[0]
            .to_string()
            .contains("chart:Chart"),
        "the model should not be offered chart types"
    );
    harness.provider.reply(ScriptedReply::new().tool(
        "t1",
        "show_ui",
        json!({ "root": "chart", "components": [ { "id": "chart", "type": "chart:Chart", "props": { "title": "Gas use", "kind": "line", "rows": [ { "x": "a", "y": 1 } ] } } ] }),
    ));
    harness.provider.reply(ScriptedReply::new());
    harness.say("chart").await;
    let messages = harness.until_end().await;
    let open = of_type(&messages, "ui.open")[0];
    assert_eq!(open["components"][0]["type"], "Text");
}

#[tokio::test]
async fn update_ui_patches_and_resync_returns_a_snapshot() {
    let mut harness = start(ScriptedProvider::new());
    let open = open_bill(&mut harness).await;
    let old_handle = handle_of(&open, "pay", "on_press");
    harness.provider.reply(ScriptedReply::new().tool(
        "t5",
        "update_ui",
        json!({
            "surface": "s1",
            "upsert": [ { "id": "pay", "type": "Button", "props": { "label": "Pay now", "tone": "primary", "on_press": { "action": "pay_bill", "args": { "bill": GAS_BILL } } } } ],
            "data": [ { "path": "/note", "value": "due soon" } ]
        }),
    ));
    harness
        .provider
        .reply(ScriptedReply::new().text("Updated."));
    harness.say("rename the button").await;
    let messages = harness.until_end().await;
    let data = of_type(&messages, "ui.data")[0];
    let patch = of_type(&messages, "ui.patch")[0];
    assert_eq!(data["seq"], 2);
    assert_eq!(patch["seq"], 3);
    assert_eq!(patch["upsert"].as_array().unwrap().len(), 1);
    let new_handle = patch["upsert"][0]["props"]["on_press"]["action_id"]
        .as_str()
        .unwrap();
    assert_ne!(new_handle, old_handle);

    harness.act("s1", &old_handle, None).await;
    assert_eq!(harness.next().await["status"], "rejected");

    harness
        .session
        .receive(r#"{"v":1,"t":"ui.resync","surface":"s1"}"#)
        .await;
    let snapshot = harness.next().await;
    assert_eq!(snapshot["t"], "ui.snapshot");
    assert_eq!(snapshot["seq"], 3);
    assert_eq!(snapshot["data"]["note"], "due soon");
}

#[tokio::test]
async fn bad_input_is_answered_with_ui_error() {
    let mut harness = start(ScriptedProvider::new());
    let huge = format!(
        "{{\"v\":1,\"t\":\"user.say\",\"text\":\"{}\"}}",
        "a".repeat(300 * 1024)
    );
    harness.session.receive(&huge).await;
    let error = harness.next().await;
    assert_eq!(error["t"], "ui.error");
    assert_eq!(error["reason"], "bad_message");
    harness.session.receive(r#"{"v":1,"t":"ui.dance"}"#).await;
    assert_eq!(harness.next().await["reason"], "unknown_message");
    harness.session.receive(r#"{"v":2,"t":"ui.action"}"#).await;
    assert_eq!(harness.next().await["reason"], "unsupported_version");
    assert!(harness.provider.requests().is_empty());
}

#[tokio::test]
async fn downgraded_parents_do_not_leave_orphans() {
    let mut harness = start_with(ScriptedProvider::new(), AgentConfig::default(), &["core@1"]);
    harness.provider.reply(ScriptedReply::new().tool(
        "t1",
        "show_ui",
        json!({ "root": "board", "components": [
            { "id": "board", "type": "chart:Dashboard", "props": { "title": "Gas" }, "children": ["due"] },
            { "id": "due", "type": "Stat", "props": { "label": "Due", "value": "29 322" } }
        ] }),
    ));
    harness.provider.reply(ScriptedReply::new());
    harness.say("dashboard").await;
    let messages = harness.until_end().await;
    let open = of_type(&messages, "ui.open")[0];
    let ids: Vec<&str> = open["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|component| component["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["board"], "{open}");
    assert_eq!(open["components"][0]["type"], "Text");
}

#[tokio::test]
async fn surfaces_too_large_for_one_message_are_rejected() {
    let mut harness = start(ScriptedProvider::new());
    let mut components = vec![
        json!({ "id": "card", "type": "Card", "children": (0..60).map(|index| format!("t{index}")).collect::<Vec<_>>() }),
    ];
    for index in 0..60 {
        components.push(json!({ "id": format!("t{index}"), "type": "Text", "props": { "text": "x".repeat(3000) } }));
    }
    let input = json!({ "root": "card", "components": components });
    assert!(input.to_string().len() < 256 * 1024);
    harness
        .provider
        .reply(ScriptedReply::new().tool("t1", "show_ui", input));
    harness.provider.reply(ScriptedReply::new());
    harness.say("long").await;
    let messages = harness.until_end().await;
    assert!(of_type(&messages, "ui.open").is_empty());
    let requests = harness.provider.requests();
    let results = tool_results(&requests[1].messages);
    assert_eq!(results[0]["is_error"], true, "{}", results[0]["content"]);
}
