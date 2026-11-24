use futures_util::stream::BoxStream;
use polyui_core::Client;
use polyui_core::protocol;
use polyui_server::anthropic::{self, AnthropicProvider, ApiKey};
use polyui_server::bills::{self, BillsHost};
use polyui_server::{Agent, ModelEvent, ModelProvider, ModelRequest, User};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Recording {
    inner: AnthropicProvider,
    requests: Mutex<Vec<ModelRequest>>,
}

impl ModelProvider for Recording {
    fn stream(&self, request: ModelRequest) -> BoxStream<'static, ModelEvent> {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        self.inner.stream(request)
    }
}

fn ui_tool_results(requests: &[ModelRequest]) -> Vec<Value> {
    requests
        .last()
        .map(|request| request.messages.clone())
        .unwrap_or_default()
        .iter()
        .filter_map(|message| message["content"].as_array().cloned())
        .flatten()
        .filter(|block| block["type"] == "tool_result")
        .collect()
}

#[tokio::test]
#[ignore = "calls the real Anthropic API; run with POLYUI_LIVE=1 and ANTHROPIC_API_KEY set"]
async fn live_gas_bill_opens_valid_ui() {
    if std::env::var("POLYUI_LIVE").as_deref() != Ok("1") {
        println!("skipped: set POLYUI_LIVE=1 to run");
        return;
    }
    let key = ApiKey::from_env().expect("ANTHROPIC_API_KEY is not set");
    let model = std::env::var("POLYUI_MODEL").unwrap_or_else(|_| anthropic::DEFAULT_MODEL.into());
    let provider = Arc::new(Recording {
        inner: AnthropicProvider::new(key, model.clone()),
        requests: Mutex::new(Vec::new()),
    });
    let agent = Agent::new(Arc::new(BillsHost::new()), provider.clone());
    let hello_text = json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen"], "locale": "en" }).to_string();
    let hello = protocol::parse_hello(&hello_text, &Default::default()).unwrap();
    let (session, mut outgoing) = agent.open_session(hello, User::new(bills::HOUSEHOLD_USER));
    session
        .receive(
            &json!({ "v": 1, "t": "user.say", "text": "Show my gas bill and let me pay it" })
                .to_string(),
        )
        .await;

    let mut opens = Vec::new();
    let mut text = String::new();
    let ending = loop {
        let message = tokio::time::timeout(Duration::from_secs(180), outgoing.recv())
            .await
            .expect("no answer within 180 s")
            .expect("session closed");
        match message["t"].as_str() {
            Some("ui.open") => opens.push(message),
            Some("agent.delta") => text.push_str(message["text"].as_str().unwrap_or_default()),
            Some("agent.done") | Some("agent.error") => break message,
            _ => {}
        }
    };
    let requests = provider
        .requests
        .lock()
        .map(|r| r.clone())
        .unwrap_or_default();
    let results = ui_tool_results(&requests);
    let errors_before_open = results
        .iter()
        .take_while(|result| result["is_error"] == true)
        .count();
    println!("model: {model}");
    println!("end: {ending}");
    println!("agent text: {text}");
    println!("model calls: {}", requests.len());
    println!("usage: {}", session.usage());
    println!(
        "valid on the first try: {}",
        if errors_before_open == 0 && !opens.is_empty() {
            "yes"
        } else {
            "no"
        }
    );
    for (index, result) in results.iter().enumerate() {
        println!("tool result {index}: {}", result["content"]);
    }
    assert_eq!(ending["t"], "agent.done", "the turn failed: {ending}");
    assert!(!opens.is_empty(), "no ui.open arrived");
    let open = &opens[0];
    println!("ui.open: {open}");
    let mut client = Client::new(&hello_text).unwrap();
    let batch = client.receive(&open.to_string());
    assert!(
        batch.send.is_empty(),
        "the client rejected ui.open: {:?}",
        batch.send
    );
    let wire = open.to_string();
    assert!(!wire.contains("pay_bill") && !wire.contains("$ref"));
    let has_pay = open["components"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["props"]["on_press"]["action_id"].is_string())
    });
    println!("has a pay button handle: {has_pay}");
}
