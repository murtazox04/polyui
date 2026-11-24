#![cfg(feature = "query")]

use futures_util::stream::BoxStream;
use polyui_core::Client;
use polyui_core::protocol;
use polyui_server::anthropic::{self, AnthropicProvider, ApiKey};
use polyui_server::shop::ShopHost;
use polyui_server::{Agent, AgentConfig, ModelEvent, ModelProvider, ModelRequest, User};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_MODEL_CALLS: u32 = 6;

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

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[tokio::test]
#[ignore = "calls the real Anthropic API; run with POLYUI_LIVE=1 and ANTHROPIC_API_KEY set"]
async fn live_shop_dashboard() {
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
    let host = Arc::new(
        ShopHost::open(manifest_dir().join("../../examples/data/shop"))
            .await
            .unwrap(),
    );
    let agent = Agent::new(host.clone(), provider.clone()).with_config(AgentConfig {
        max_model_calls: MAX_MODEL_CALLS,
        ..AgentConfig::default()
    });
    let hello_text = json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen"], "locale": "en" }).to_string();
    let hello = protocol::parse_hello(&hello_text, &Default::default()).unwrap();
    let (session, mut outgoing) = agent.open_session(hello, User::new("owner"));
    session
        .receive(
            &json!({ "v": 1, "t": "user.say", "text": "How is the store doing this year? Build me a dashboard." })
                .to_string(),
        )
        .await;
    let mut opens = Vec::new();
    let mut text = String::new();
    let ending = loop {
        let message = tokio::time::timeout(Duration::from_secs(240), outgoing.recv())
            .await
            .expect("no answer within 240 s")
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
        .map(|list| list.clone())
        .unwrap_or_default();
    println!("model: {model}");
    println!("model calls: {}", requests.len());
    println!("query executions: {}", host.engine().executions());
    println!("usage: {}", session.usage());
    println!("end: {ending}");
    println!("agent text: {text}");
    if let Some(last) = requests.last() {
        for message in &last.messages {
            for block in message["content"].as_array().into_iter().flatten() {
                match block["type"].as_str() {
                    Some("tool_use") => println!("tool_use {} {}", block["name"], block["input"]),
                    Some("tool_result") => println!(
                        "tool_result error={} {}",
                        block["is_error"] == true,
                        block["content"]
                    ),
                    _ => {}
                }
            }
        }
    }
    assert!(requests.len() <= MAX_MODEL_CALLS as usize);
    assert!(!opens.is_empty(), "no ui.open arrived");
    let open: &Value = &opens[0];
    let wire = open.to_string();
    assert!(!wire.contains("$ref"));
    let mut client = Client::new(&hello_text).unwrap();
    let batch = client.receive(&wire);
    assert!(
        batch.send.is_empty(),
        "client refused ui.open: {:?}",
        batch.send
    );
    let fixture = manifest_dir().join("tests/fixtures/shop_dashboard_open.json");
    std::fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    std::fs::write(&fixture, serde_json::to_string_pretty(open).unwrap()).unwrap();
    println!("saved {}", fixture.display());
}
