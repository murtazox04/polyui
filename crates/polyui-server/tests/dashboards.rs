#![cfg(feature = "query")]

use polyui_core::Client;
use polyui_core::protocol;
use polyui_server::shop::ShopHost;
use polyui_server::{Agent, ScriptedProvider, ScriptedReply, Session, User};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

const HELLO: &str =
    r#"{"v":1,"t":"hello","catalogs":["core@1","chart@1"],"modalities":["screen"],"locale":"en"}"#;
const MONTHLY: &str = "SELECT to_char(order_date, '%Y-%m') AS x, round(sum(quantity * unit_price * (1 - discount)), 0) AS y FROM orders WHERE status = 'paid' GROUP BY to_char(order_date, '%Y-%m') ORDER BY x";
const BY_CATEGORY: &str = "SELECT p.category AS x, sum(o.quantity) AS y FROM orders o JOIN products p ON o.product_id = p.product_id WHERE o.status = 'paid' GROUP BY p.category ORDER BY y DESC";
const ORDER_COUNT: &str = "SELECT count(*) AS orders FROM orders WHERE status = 'paid'";
const BY_CHANNEL: &str =
    "SELECT channel AS x, count(*) AS y FROM orders GROUP BY channel ORDER BY x";

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/data/shop")
}

struct Harness {
    provider: Arc<ScriptedProvider>,
    host: Arc<ShopHost>,
    session: Arc<Session>,
    outgoing: Receiver<Value>,
}

async fn start(provider: ScriptedProvider) -> Harness {
    let provider = Arc::new(provider);
    let host = Arc::new(ShopHost::open(data_dir()).await.unwrap());
    let agent = Agent::new(host.clone(), provider.clone());
    let hello = protocol::parse_hello(HELLO, &Default::default()).unwrap();
    let (session, outgoing) = agent.open_session(hello, User::new("owner"));
    Harness {
        provider,
        host,
        session,
        outgoing,
    }
}

impl Harness {
    async fn say(&mut self, text: &str) -> Vec<Value> {
        self.session
            .receive(&json!({ "v": 1, "t": "user.say", "text": text }).to_string())
            .await;
        let mut seen = Vec::new();
        loop {
            let message = tokio::time::timeout(Duration::from_secs(20), self.outgoing.recv())
                .await
                .expect("timed out")
                .expect("session closed");
            let end = message["t"] == "agent.done" || message["t"] == "agent.error";
            seen.push(message);
            if end {
                return seen;
            }
        }
    }

    fn tool_results(&self) -> Vec<Value> {
        self.provider
            .requests()
            .last()
            .map(|request| request.messages.clone())
            .unwrap_or_default()
            .iter()
            .filter_map(|message| message["content"].as_array().cloned())
            .flatten()
            .filter(|block| block["type"] == "tool_result")
            .collect()
    }

    fn model_saw(&self) -> String {
        self.provider
            .requests()
            .iter()
            .map(|request| json!(request.messages).to_string())
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

fn component<'a>(message: &'a Value, list: &str, id: &str) -> &'a Value {
    message[list]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == id))
        .unwrap_or_else(|| panic!("no component {id} in {message}"))
}

fn dashboard() -> Value {
    json!({
        "root": "dash",
        "components": [
            { "id": "dash", "type": "chart:Dashboard", "props": { "title": "Store this year", "columns": 2 }, "children": ["orders", "trend", "mix"] },
            { "id": "orders", "type": "Stat", "props": { "label": "Paid orders", "value": { "$ref": "stat:q3" } } },
            { "id": "trend", "type": "chart:Chart", "props": { "title": "Revenue by month", "kind": "line", "unit": "so'm", "rows": { "$ref": "query:q1" } } },
            { "id": "mix", "type": "chart:Chart", "props": { "title": "Units by category", "kind": "bar", "rows": { "$ref": "query:q2" } } }
        ]
    })
}

fn dashboard_script() -> ScriptedProvider {
    let provider = ScriptedProvider::new();
    provider
        .reply(ScriptedReply::new().tool("t1", "describe_data", json!({})))
        .reply(
            ScriptedReply::new()
                .tool("t2", "run_query", json!({ "sql": MONTHLY }))
                .tool("t3", "run_query", json!({ "sql": BY_CATEGORY }))
                .tool("t4", "run_query", json!({ "sql": ORDER_COUNT })),
        )
        .reply(ScriptedReply::new().tool("t5", "show_ui", dashboard()))
        .reply(ScriptedReply::new().text("Here is the store."));
    provider
}

#[tokio::test]
async fn dashboard_numbers_come_from_the_server_not_the_model() {
    let mut harness = start(dashboard_script()).await;
    let messages = harness.say("How is the store doing?").await;
    let results = harness.tool_results();
    assert!(
        results[0]["content"]
            .as_str()
            .unwrap()
            .contains("orders (1169 rows)")
    );
    assert!(!results[0]["content"].as_str().unwrap().contains("email"));
    let saved = results[1]["content"].as_str().unwrap();
    assert!(saved.starts_with("Saved as q1: 11 rows."), "{saved}");
    assert!(saved.contains("{\"$ref\": \"query:q1\"}"));
    assert!(
        results[3]["content"]
            .as_str()
            .unwrap()
            .contains("{\"$ref\": \"stat:q3\"}")
    );
    let summary = results[4]["content"].as_str().unwrap();
    assert!(results[4]["is_error"].is_null(), "{summary}");
    assert!(summary.contains("[ref]"), "{summary}");

    let opens = of_type(&messages, "ui.open");
    assert_eq!(opens.len(), 1);
    let open = opens[0];
    let wire = open.to_string();
    assert!(!wire.contains("$ref") && !wire.contains("query:q1"));
    let expected = harness.host.engine().run(MONTHLY).await.unwrap();
    let rows = component(open, "components", "trend")["props"]["rows"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(rows.len(), 11);
    assert_eq!(rows[0]["x"], expected.rows[0]["x"]);
    assert_eq!(rows[0]["y"], expected.rows[0]["y"]);
    assert!(component(open, "components", "trend")["props"]["scene"]["marks"].is_array());
    let paid = harness.host.engine().run(ORDER_COUNT).await.unwrap();
    let count = paid.single_value().and_then(Value::as_u64).unwrap();
    let shown = component(open, "components", "orders")["props"]["value"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(shown.replace([',', ' ', '\u{a0}'], ""), count.to_string());

    let mut client = Client::new(HELLO).unwrap();
    let batch = client.receive(&wire);
    assert!(
        batch.send.is_empty(),
        "client refused ui.open: {:?}",
        batch.send
    );
    assert!(
        !harness
            .model_saw()
            .contains(&format!("\"y\":{}", rows[0]["y"]))
    );
}

#[tokio::test]
async fn changing_one_panel_reruns_only_its_query_and_scene() {
    let provider = dashboard_script();
    provider
        .reply(ScriptedReply::new().tool("t6", "run_query", json!({ "sql": BY_CHANNEL })))
        .reply(ScriptedReply::new().tool(
            "t7",
            "update_ui",
            json!({ "surface": "s1", "upsert": [
                { "id": "mix", "type": "chart:Chart", "props": { "title": "Orders by channel", "kind": "pie", "rows": { "$ref": "query:q4" } } }
            ] }),
        ))
        .reply(ScriptedReply::new().text("Changed."));
    let mut harness = start(provider).await;
    harness.say("How is the store doing?").await;
    let after_show = harness.host.engine().executions();
    assert_eq!(after_show, 6);
    let messages = harness
        .say("Show orders by channel instead of categories")
        .await;
    assert_eq!(harness.host.engine().executions(), after_show + 2);
    let patches = of_type(&messages, "ui.patch");
    assert_eq!(patches.len(), 1);
    let upsert = patches[0]["upsert"].as_array().unwrap();
    assert_eq!(upsert.len(), 1);
    assert_eq!(upsert[0]["id"], "mix");
    assert_eq!(upsert[0]["props"]["rows"].as_array().unwrap().len(), 3);
    assert!(upsert[0]["props"]["scene"].is_object());
    assert!(of_type(&messages, "ui.open").is_empty());
    let summary = harness.session.model_summary("s1").unwrap();
    assert!(summary.contains("Orders by channel"), "{summary}");
}

#[tokio::test]
async fn bad_refs_give_the_model_fixable_problems() {
    let provider = ScriptedProvider::new();
    provider
        .reply(ScriptedReply::new().tool(
            "t1",
            "run_query",
            json!({ "sql": "SELECT category, count(*) AS n FROM products GROUP BY category" }),
        ))
        .reply(ScriptedReply::new().tool(
            "t2",
            "show_ui",
            json!({ "root": "dash", "components": [
                { "id": "dash", "type": "chart:Dashboard", "props": { "title": "x" }, "children": ["c", "s"] },
                { "id": "c", "type": "chart:Chart", "props": { "title": "Mix", "kind": "bar", "rows": { "$ref": "query:q1" } } },
                { "id": "s", "type": "Stat", "props": { "label": "Total", "value": { "$ref": "stat:q9" } } }
            ] }),
        ))
        .reply(ScriptedReply::new().tool(
            "t3",
            "show_ui",
            json!({ "root": "s", "components": [
                { "id": "s", "type": "Stat", "props": { "label": "Total", "value": { "$ref": "query:q1" } } }
            ] }),
        ))
        .reply(ScriptedReply::new().text("Sorry."));
    let mut harness = start(provider).await;
    let messages = harness.say("Show categories").await;
    assert!(of_type(&messages, "ui.open").is_empty());
    let results = harness.tool_results();
    let first = results[1]["content"].as_str().unwrap();
    assert_eq!(results[1]["is_error"], true);
    assert!(first.contains("id \"c\""), "{first}");
    assert!(first.contains("columns named x and y"), "{first}");
    assert!(
        first.contains("query q9 does not exist. Saved queries: q1."),
        "{first}"
    );
    let second = results[2]["content"].as_str().unwrap();
    assert!(
        second.contains("$ref source \"query\" gives a list"),
        "{second}"
    );
}

#[tokio::test]
async fn sql_injection_from_the_prompt_cannot_write() {
    let before: Vec<Vec<u8>> = ["orders.parquet", "products.csv", "customers.csv"]
        .iter()
        .map(|name| std::fs::read(data_dir().join(name)).unwrap())
        .collect();
    let city = "Tashkent'); DROP TABLE orders; --";
    let provider = ScriptedProvider::new();
    provider
        .reply(ScriptedReply::new().tool(
            "t1",
            "run_query",
            json!({ "sql": format!("SELECT count(*) AS n FROM customers WHERE city = '{city}'") }),
        ))
        .reply(ScriptedReply::new().tool("t2", "run_query", json!({ "sql": "DROP TABLE orders" })))
        .reply(ScriptedReply::new().tool(
            "t3",
            "run_query",
            json!({ "sql": "COPY orders TO '/tmp/polyui-server-copy.csv'" }),
        ))
        .reply(ScriptedReply::new().tool(
            "t4",
            "run_query",
            json!({ "sql": "SELECT email FROM customers" }),
        ))
        .reply(ScriptedReply::new().tool(
            "t5",
            "run_query",
            json!({ "sql": "SELECT count(*) AS n FROM orders" }),
        ))
        .reply(ScriptedReply::new().text("Done."));
    let mut harness = start(provider).await;
    harness
        .say(&format!("How many customers live in {city}"))
        .await;
    let results = harness.tool_results();
    for result in &results[..4] {
        assert_eq!(result["is_error"], true, "{result}");
        let text = result["content"].as_str().unwrap();
        assert!(
            !text.contains(
                &data_dir()
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            )
        );
    }
    assert!(results[4]["content"].as_str().unwrap().contains("1169"));
    assert!(!std::path::Path::new("/tmp/polyui-server-copy.csv").exists());
    let after: Vec<Vec<u8>> = ["orders.parquet", "products.csv", "customers.csv"]
        .iter()
        .map(|name| std::fs::read(data_dir().join(name)).unwrap())
        .collect();
    assert_eq!(before, after);
}

#[tokio::test]
async fn data_tools_are_offered_only_by_hosts_with_data() {
    let harness = start(ScriptedProvider::new()).await;
    let names: Vec<&str> = harness
        .session
        .tools()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"describe_data") && names.contains(&"run_query"));
    assert!(harness.session.system_prompt().contains("stat:q2"));
    let bills = Agent::new(
        Arc::new(polyui_server::bills::BillsHost::new()),
        Arc::new(ScriptedProvider::new()),
    );
    let hello = protocol::parse_hello(HELLO, &Default::default()).unwrap();
    let (session, outgoing) = bills.open_session(hello, User::guest());
    drop(outgoing);
    assert!(
        !session
            .tools()
            .iter()
            .any(|tool| tool["name"] == "run_query")
    );
}

#[test]
fn saved_live_dashboard_is_accepted_by_the_client() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shop_dashboard_open.json");
    let open: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let wire = open.to_string();
    assert!(!wire.contains("$ref"));
    let types: Vec<&str> = open["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|component| component["type"].as_str())
        .collect();
    assert_eq!(types[0], "chart:Dashboard");
    assert!((3..=4).contains(&(types.len() - 1)));
    let mut client = Client::new(HELLO).unwrap();
    let batch = client.receive(&wire);
    assert!(
        batch.send.is_empty(),
        "client refused the fixture: {:?}",
        batch.send
    );
    assert!(!batch.ops.is_empty());
}
