use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use polyui_core::Client;
use polyui_core::protocol;
use polyui_server::anthropic::{self, AnthropicProvider, ApiKey};
use polyui_server::bills::{self, BillsHost};
use polyui_server::{Agent, AgentConfig, ModelEvent, ModelProvider, ModelRequest, User};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    let strict_tools: Vec<String> = requests
        .first()
        .map(|request| {
            request
                .tools
                .iter()
                .filter(|tool| tool["strict"] == true)
                .filter_map(|tool| tool["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    println!("model: {model}");
    println!("tools sent with strict: {strict_tools:?}");
    println!(
        "strict accepted by the API: {}",
        if provider.inner.strict_tools() {
            "yes"
        } else {
            "no, the provider fell back to non-strict"
        }
    );
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

#[derive(Debug, Default, Clone)]
struct Timing {
    deltas: Vec<(Duration, usize)>,
    input_done: Option<Duration>,
}

struct Timed {
    inner: AnthropicProvider,
    started: Instant,
    timing: Arc<Mutex<Timing>>,
}

impl ModelProvider for Timed {
    fn stream(&self, request: ModelRequest) -> BoxStream<'static, ModelEvent> {
        let started = self.started;
        let timing = self.timing.clone();
        self.inner
            .stream(request)
            .map(move |event| {
                if let Ok(mut timing) = timing.lock() {
                    match &event {
                        ModelEvent::ToolInputDelta { partial_json, .. } => {
                            timing.deltas.push((started.elapsed(), partial_json.len()));
                        }
                        ModelEvent::ToolCallFinished { name, .. }
                            if name == "show_ui" && timing.input_done.is_none() =>
                        {
                            timing.input_done = Some(started.elapsed());
                        }
                        _ => {}
                    }
                }
                event
            })
            .boxed()
    }
}

async fn first_open(features: &[&str], key: ApiKey, model: &str) -> (Option<Duration>, Timing) {
    let started = Instant::now();
    let timing = Arc::new(Mutex::new(Timing::default()));
    let provider = Arc::new(Timed {
        inner: AnthropicProvider::new(key, model),
        started,
        timing: timing.clone(),
    });
    let agent = Agent::new(Arc::new(BillsHost::new()), provider).with_config(AgentConfig {
        max_model_calls: 1,
        ..AgentConfig::default()
    });
    let hello_text = json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen"], "features": features, "locale": "en" }).to_string();
    let hello = protocol::parse_hello(&hello_text, &Default::default()).unwrap();
    let (session, mut outgoing) = agent.open_session(hello, User::new(bills::HOUSEHOLD_USER));
    session
        .receive(
            &json!({ "v": 1, "t": "user.say", "text": "Show all four of my bills with a pay button for each" })
                .to_string(),
        )
        .await;
    let mut opened = None;
    let mut kinds = Vec::new();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(180), outgoing.recv())
            .await
            .expect("no answer within 180 s")
            .expect("session closed");
        let kind = message["t"].as_str().unwrap_or_default().to_string();
        if kind == "ui.open" && opened.is_none() {
            opened = Some(started.elapsed());
        }
        if kind != "agent.delta" {
            kinds.push(kind.clone());
        }
        if kind == "agent.done" || kind == "agent.error" {
            break;
        }
    }
    println!("features {features:?}: messages {kinds:?}");
    let timing = timing
        .lock()
        .map(|timing| timing.clone())
        .unwrap_or_default();
    (opened, timing)
}

#[tokio::test]
#[ignore = "calls the real Anthropic API; run with POLYUI_LIVE=1 and ANTHROPIC_API_KEY set"]
async fn live_streaming_opens_ui_sooner() {
    if std::env::var("POLYUI_LIVE").as_deref() != Ok("1") {
        println!("skipped: set POLYUI_LIVE=1 to run");
        return;
    }
    let model = std::env::var("POLYUI_MODEL").unwrap_or_else(|_| anthropic::DEFAULT_MODEL.into());
    let key = || ApiKey::from_env().expect("ANTHROPIC_API_KEY is not set");
    let (streamed, timing) = first_open(&["stream"], key(), &model).await;
    let (whole, plain) = first_open(&[], key(), &model).await;
    let count = timing.deltas.len();
    let bytes: usize = timing.deltas.iter().map(|(_, size)| size).sum();
    let span = match (timing.deltas.first(), timing.deltas.last()) {
        (Some((first, _)), Some((last, _))) => *last - *first,
        _ => Duration::ZERO,
    };
    println!("model: {model}");
    println!(
        "show_ui input: {count} deltas, {bytes} bytes over {span:?}, about {} bytes every {:?}",
        bytes / count.max(1),
        span / u32::try_from(count.max(2) - 1).unwrap_or(1)
    );
    println!(
        "with stream: first ui.open at {streamed:?}, show_ui input complete at {:?}",
        timing.input_done
    );
    println!(
        "without stream: first ui.open at {whole:?}, show_ui input complete at {:?}",
        plain.input_done
    );
    assert!(streamed.is_some(), "no ui.open with streaming");
}
