use polyui_core::Client;
use polyui_core::protocol;
use polyui_server::bills::{BillsHost, HOUSEHOLD_USER};
use polyui_server::{Agent, ScriptedProvider, ScriptedReply, Session, User};
use serde_json::{Map, Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Receiver;

const CHUNK_BYTES: usize = 15;
const CHUNK_EVERY: Duration = Duration::from_millis(21);

fn hello_text(features: &[&str]) -> String {
    json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen"], "features": features, "locale": "en" }).to_string()
}

fn start(provider: Arc<ScriptedProvider>, features: &[&str]) -> (Arc<Session>, Receiver<Value>) {
    let agent = Agent::new(Arc::new(BillsHost::new()), provider);
    let hello = protocol::parse_hello(&hello_text(features), &Default::default()).unwrap();
    agent.open_session(hello, User::new(HOUSEHOLD_USER))
}

fn bill_row(service: &str, bill: &str, amount: &str) -> [Value; 3] {
    [
        json!({ "id": format!("{service}_row"), "type": "Row", "children": [format!("{service}_amount"), format!("{service}_pay")] }),
        json!({ "id": format!("{service}_amount"), "type": "Stat", "props": { "label": service, "value": amount } }),
        json!({ "id": format!("{service}_pay"), "type": "Button", "props": { "label": format!("Pay {service}"), "tone": "primary", "on_press": { "action": "pay_bill", "args": { "bill": bill } } } }),
    ]
}

fn bills_input() -> Value {
    let mut components = vec![
        json!({ "id": "card", "type": "Card", "props": { "title": "Your bills" }, "children": ["intro", "gas_row", "power_row", "water_row", "net_row", "total"] }),
        json!({ "id": "intro", "type": "Text", "props": { "text": "Four bills are due this month." } }),
    ];
    for (service, bill, amount) in [
        ("gas", "b_gas_2611", "29 322 so'm"),
        ("power", "b_power_2611", "84 110 so'm"),
        ("water", "b_water_2611", "18 400 so'm"),
        ("net", "b_net_2611", "120 000 so'm"),
    ] {
        components.extend(bill_row(service, bill, amount));
    }
    components
        .push(json!({ "id": "total", "type": "Text", "props": { "text": { "$bind": "/total" } } }));
    json!({ "root": "card", "components": components, "data": [{ "path": "/total", "value": "Total 251 832 so'm" }] })
}

async fn until_end(
    outgoing: &mut Receiver<Value>,
    started: Instant,
) -> (Vec<Value>, Option<Duration>) {
    let mut seen = Vec::new();
    let mut opened = None;
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), outgoing.recv())
            .await
            .expect("timed out waiting for a server message")
            .expect("the session closed");
        if message["t"] == "ui.open" && opened.is_none() {
            opened = Some(started.elapsed());
        }
        let end = message["t"] == "agent.done" || message["t"] == "agent.error";
        seen.push(message);
        if end {
            return (seen, opened);
        }
    }
}

async fn run(
    features: &[&str],
    replies: Vec<ScriptedReply>,
) -> (Arc<Session>, Receiver<Value>, Vec<Value>, Option<Duration>) {
    let provider = Arc::new(ScriptedProvider::new());
    for reply in replies {
        provider.reply(reply);
    }
    let (session, mut outgoing) = start(provider, features);
    let started = Instant::now();
    session
        .receive(&json!({ "v": 1, "t": "user.say", "text": "show my bills" }).to_string())
        .await;
    let (messages, opened) = until_end(&mut outgoing, started).await;
    (session, outgoing, messages, opened)
}

fn paced_bills() -> ScriptedReply {
    ScriptedReply::new()
        .text("Here are your bills.")
        .tool("t1", "show_ui", bills_input())
        .paced(CHUNK_BYTES, CHUNK_EVERY)
}

fn ui_messages(messages: &[Value]) -> Vec<&Value> {
    messages
        .iter()
        .filter(|message| {
            message["t"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("ui."))
        })
        .collect()
}

fn apply(state: &mut Map<String, Value>, message: &Value) {
    match message["t"].as_str() {
        Some("ui.open") | Some("ui.snapshot") => {
            state.clear();
            state.insert("root".into(), message["root"].clone());
            state.insert("data".into(), message["data"].clone());
            for component in message["components"].as_array().unwrap() {
                state.insert(
                    format!("c:{}", component["id"].as_str().unwrap()),
                    component.clone(),
                );
            }
        }
        Some("ui.patch") => {
            for id in message["remove"].as_array().into_iter().flatten() {
                state.remove(&format!("c:{}", id.as_str().unwrap()));
            }
            for component in message["upsert"].as_array().into_iter().flatten() {
                state.insert(
                    format!("c:{}", component["id"].as_str().unwrap()),
                    component.clone(),
                );
            }
            if let Some(root) = message.get("root").filter(|root| !root.is_null()) {
                state.insert("root".into(), root.clone());
            }
        }
        Some("ui.data") => {
            let data = state.entry("data").or_insert_with(|| json!({}));
            for op in message["ops"].as_array().unwrap() {
                let key = op["path"]
                    .as_str()
                    .unwrap()
                    .trim_start_matches('/')
                    .to_string();
                match op["op"].as_str() {
                    Some("set") => {
                        data[key.as_str()] = op["value"].clone();
                    }
                    Some("remove") => {
                        data.as_object_mut().unwrap().remove(&key);
                    }
                    other => panic!("unexpected op {other:?}"),
                }
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn streaming_opens_the_surface_while_the_model_is_still_writing() {
    let (session, mut outgoing, messages, streamed) = run(
        &["stream"],
        vec![paced_bills(), ScriptedReply::new().text("Done.")],
    )
    .await;
    let (_, _, plain_messages, whole) =
        run(&[], vec![paced_bills(), ScriptedReply::new().text("Done.")]).await;
    let streamed = streamed.expect("a streamed ui.open");
    let whole = whole.expect("a ui.open without streaming");
    println!(
        "time to first ui.open with {CHUNK_BYTES} byte chunks every {CHUNK_EVERY:?}: streaming {streamed:?}, without streaming {whole:?}"
    );
    assert!(
        streamed * 2 < whole,
        "streaming {streamed:?}, whole {whole:?}"
    );

    let ui = ui_messages(&messages);
    assert_eq!(ui[0]["t"], "ui.open");
    let first = ui[0]["components"].as_array().unwrap();
    assert!(
        first.len() < 16,
        "the early ui.open already had {} components",
        first.len()
    );
    assert!(
        ui.iter()
            .filter(|message| message["t"] == "ui.patch")
            .count()
            >= 2
    );
    for (expected, message) in ui.iter().enumerate() {
        assert_eq!(message["seq"], expected + 1, "{message}");
    }
    assert_eq!(ui_messages(&plain_messages).len(), 1);

    let mut client = Client::new(&hello_text(&["stream"])).unwrap();
    for message in &ui {
        let batch = client.receive(&message.to_string());
        assert!(
            batch.send.is_empty(),
            "the client answered {:?} to {message}",
            batch.send
        );
    }

    let mut streamed_state = Map::new();
    for message in &ui {
        apply(&mut streamed_state, message);
    }
    session
        .receive(r#"{"v":1,"t":"ui.resync","surface":"s1"}"#)
        .await;
    let snapshot = tokio::time::timeout(Duration::from_secs(5), outgoing.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot["t"], "ui.snapshot");
    assert_eq!(snapshot["seq"], ui.len());
    let mut final_state = Map::new();
    apply(&mut final_state, &snapshot);
    assert_eq!(streamed_state, final_state);
    assert_eq!(final_state["data"]["total"], "Total 251 832 so'm");

    let pay = final_state["c:gas_pay"]["props"]["on_press"]["action_id"]
        .as_str()
        .unwrap()
        .to_string();
    session
        .receive(&json!({ "v": 1, "t": "ui.action", "surface": "s1", "action_id": pay, "source": "tap" }).to_string())
        .await;
    let confirm = tokio::time::timeout(Duration::from_secs(5), outgoing.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(confirm["t"], "ui.confirm", "{confirm}");
}

#[tokio::test]
async fn a_streamed_surface_that_fails_the_final_check_is_closed() {
    let mut broken = bills_input();
    broken["components"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!("ghost"));
    let replies = vec![
        ScriptedReply::new()
            .tool("t1", "show_ui", broken)
            .paced(CHUNK_BYTES, Duration::from_millis(1)),
        ScriptedReply::new().tool("t2", "show_ui", bills_input()),
        ScriptedReply::new().text("Done."),
    ];
    let (session, outgoing, messages, opened) = run(&["stream"], replies).await;
    assert!(opened.is_some());
    let ui = ui_messages(&messages);
    let kinds: Vec<&str> = ui
        .iter()
        .map(|message| message["t"].as_str().unwrap())
        .collect();
    let close = kinds
        .iter()
        .position(|kind| *kind == "ui.close")
        .expect("the broken surface is closed");
    assert_eq!(ui[0]["surface"], ui[close]["surface"]);
    let reopened: Vec<&&Value> = ui[close..]
        .iter()
        .filter(|message| message["t"] == "ui.open")
        .collect();
    assert_eq!(reopened.len(), 1);
    assert_ne!(reopened[0]["surface"], ui[0]["surface"]);
    assert_eq!(
        session.open_surfaces(),
        vec![reopened[0]["surface"].as_str().unwrap().to_string()]
    );
    drop(outgoing);
}
