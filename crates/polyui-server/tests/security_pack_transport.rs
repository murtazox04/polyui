use axum::http::HeaderMap;
use futures_util::future::BoxFuture;
use futures_util::stream::{self, BoxStream};
use futures_util::{SinkExt, StreamExt};
use polyui_core::{ActionRegistry, Locale, RefResolver};
use polyui_server::anthropic::SseDecoder;
use polyui_server::bills::BillsHost;
use polyui_server::{
    ActionCall, ActionOutcome, Agent, ConfirmText, Host, ModelEvent, ModelProvider, ModelRequest,
    ScriptedProvider, ScriptedReply, ServerConfig, StopReason, Usage, User,
};
use serde_json::{Map, Value, json};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::Frame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::{Data, OpCode};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const WAIT: Duration = Duration::from_secs(5);

async fn spawn_with(agent: Agent, config: ServerConfig) -> SocketAddr {
    let app = polyui_server::router(agent, config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(polyui_server::serve(listener, app));
    addr
}

async fn spawn(provider: Arc<dyn ModelProvider>) -> SocketAddr {
    spawn_with(
        Agent::new(Arc::new(BillsHost::new()), provider),
        ServerConfig::default(),
    )
    .await
}

fn hello() -> String {
    json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen"], "locale": "en" })
        .to_string()
}

async fn connect(addr: SocketAddr) -> Socket {
    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    socket
}

async fn read(socket: &mut Socket) -> Option<Value> {
    loop {
        match tokio::time::timeout(WAIT, socket.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => return serde_json::from_str(text.as_str()).ok(),
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
            _ => return None,
        }
    }
}

async fn joined(addr: SocketAddr) -> Socket {
    let mut socket = connect(addr).await;
    socket.send(Message::text(hello())).await.unwrap();
    let welcome = read(&mut socket).await.expect("welcome arrives");
    assert_eq!(welcome["t"], "welcome");
    socket
}

async fn assert_bad_message(socket: &mut Socket) {
    let reply = read(socket).await.expect("an answer arrives");
    assert_eq!(reply["t"], "ui.error", "{reply}");
    assert_eq!(reply["reason"], "bad_message", "{reply}");
}

async fn still_serving(addr: SocketAddr) {
    let started = Instant::now();
    let mut socket = joined(addr).await;
    socket
        .send(Message::text(r#"{"v":1,"t":"ui.resync","surface":"s1"}"#))
        .await
        .unwrap();
    assert_bad_message(&mut socket).await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "a new client waited {:?}",
        started.elapsed()
    );
}

fn deep(depth: usize) -> String {
    format!("{}1{}", "[".repeat(depth), "]".repeat(depth))
}

struct SlowProvider {
    calls: AtomicUsize,
    delay: Duration,
}

impl ModelProvider for SlowProvider {
    fn stream(&self, request: ModelRequest) -> BoxStream<'static, ModelEvent> {
        let _ = request;
        self.calls.fetch_add(1, Ordering::SeqCst);
        let delay = self.delay;
        stream::once(async move {
            tokio::time::sleep(delay).await;
            ModelEvent::MessageFinished {
                stop_reason: StopReason::EndTurn,
                content: vec![json!({ "type": "text", "text": "ok" })],
                usage: Usage::default(),
            }
        })
        .boxed()
    }
}

struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct HangingProvider {
    calls: AtomicUsize,
    dropped: Arc<AtomicBool>,
}

impl ModelProvider for HangingProvider {
    fn stream(&self, request: ModelRequest) -> BoxStream<'static, ModelEvent> {
        drop(request);
        self.calls.fetch_add(1, Ordering::SeqCst);
        let flag = DropFlag(self.dropped.clone());
        stream::unfold(flag, |flag| async move {
            std::future::pending::<()>().await;
            Some((ModelEvent::TextDelta(String::new()), flag))
        })
        .boxed()
    }
}

struct HeaderHost {
    inner: BillsHost,
}

impl Host for HeaderHost {
    fn system_prompt(&self, user: &User, locale: Locale) -> String {
        self.inner.system_prompt(user, locale)
    }

    fn actions(&self) -> &dyn ActionRegistry {
        self.inner.actions()
    }

    fn refs<'a>(&'a self, user: &'a User) -> Box<dyn RefResolver + 'a> {
        self.inner.refs(user)
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

    fn authenticate(&self, headers: &HeaderMap) -> Result<User, String> {
        headers
            .get("x-user")
            .and_then(|value| value.to_str().ok())
            .map(User::new)
            .ok_or_else(|| "sign in first".to_string())
    }
}

#[tokio::test]
async fn ws_invalid_json_gets_bad_message_and_the_socket_stays_usable() {
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(ScriptedReply::new().text("Still here."));
    let addr = spawn(provider.clone()).await;
    let mut socket = joined(addr).await;
    for garbage in [
        "{",
        "not json",
        "[]",
        "null",
        "{\"v\":1}",
        "{\"v\":1,\"t\":\"ui.action\"}",
        "{\"v\":1,\"t\":\"ui.resync\",\"surface\":\"\\ud800\"}",
        "\u{FEFF}{\"v\":1,\"t\":\"user.say\",\"text\":\"x\"}",
        "{\"v\":1,\"t\":\"user.say\",\"text\":\"x\"} trailing",
    ] {
        socket.send(Message::text(garbage)).await.unwrap();
        assert_bad_message(&mut socket).await;
    }
    socket
        .send(Message::text(
            json!({ "v": 1, "t": "user.say", "text": "hi" }).to_string(),
        ))
        .await
        .unwrap();
    let mut kinds = Vec::new();
    while let Some(message) = read(&mut socket).await {
        kinds.push(message["t"].as_str().unwrap_or_default().to_string());
        if message["t"] == "agent.done" {
            break;
        }
    }
    assert_eq!(kinds, ["agent.delta", "agent.done"]);
    assert_eq!(provider.requests().len(), 1);
}

#[tokio::test]
async fn ws_invalid_utf8_text_frames_drop_only_that_connection() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone()).await;
    let mut socket = joined(addr).await;
    let frame = Frame::message(
        vec![b'{', 0xff, 0xfe, 0xc3, 0x28, b'}'],
        OpCode::Data(Data::Text),
        true,
    );
    let _ = socket.send(Message::Frame(frame)).await;
    let after = tokio::time::timeout(WAIT, socket.next()).await;
    assert!(
        !matches!(after, Ok(Some(Ok(Message::Text(_))))),
        "invalid UTF-8 must not be processed: {after:?}"
    );
    let mut first = connect(addr).await;
    let frame = Frame::message(vec![0xc0, 0xaf], OpCode::Data(Data::Text), true);
    let _ = first.send(Message::Frame(frame)).await;
    assert!(
        read(&mut first)
            .await
            .is_none_or(|reply| reply["t"] != "welcome")
    );
    still_serving(addr).await;
    assert!(provider.requests().is_empty());
}

#[tokio::test]
async fn ws_binary_frames_are_refused() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone()).await;
    let mut socket = joined(addr).await;
    socket
        .send(Message::binary(
            json!({ "v": 1, "t": "user.say", "text": "hi" })
                .to_string()
                .into_bytes(),
        ))
        .await
        .unwrap();
    assert_bad_message(&mut socket).await;
    assert!(provider.requests().is_empty());
}

#[tokio::test]
async fn ws_hello_must_be_valid_json() {
    let addr = spawn(Arc::new(ScriptedProvider::new())).await;
    for first in ["{", "", "{\"v\":1,\"t\":\"hello\"}", deep(1000).as_str()] {
        let mut socket = connect(addr).await;
        socket.send(Message::text(first)).await.unwrap();
        assert_bad_message(&mut socket).await;
        let closed = tokio::time::timeout(WAIT, socket.next()).await;
        assert!(
            !matches!(closed, Ok(Some(Ok(Message::Text(_))))),
            "{closed:?}"
        );
    }
    still_serving(addr).await;
}

#[tokio::test]
async fn ws_deep_and_oversized_messages_are_refused() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone()).await;
    let mut socket = joined(addr).await;
    socket.send(Message::text(deep(120_000))).await.unwrap();
    assert_bad_message(&mut socket).await;
    let deep_say = format!(
        "{{\"v\":1,\"t\":\"user.say\",\"text\":\"x\",\"pad\":{}}}",
        deep(100_000)
    );
    socket.send(Message::text(deep_say)).await.unwrap();
    assert_bad_message(&mut socket).await;
    let huge = format!(
        "{{\"v\":1,\"t\":\"user.say\",\"text\":\"{}\"}}",
        "a".repeat(300 * 1024)
    );
    let _ = socket.send(Message::text(huge)).await;
    let after = tokio::time::timeout(WAIT, socket.next()).await;
    assert!(
        !matches!(after, Ok(Some(Ok(Message::Text(_))))),
        "an oversized frame must not be processed: {after:?}"
    );
    still_serving(addr).await;
    assert!(provider.requests().is_empty());
}

#[tokio::test]
async fn ws_flood_of_bad_messages_keeps_the_server_responsive() {
    let addr = spawn(Arc::new(ScriptedProvider::new())).await;
    let flooder = joined(addr).await;
    let (mut sink, mut source) = flooder.split();
    let total = 20_000;
    let reader = tokio::spawn(async move {
        let mut replies = 0;
        while replies < total {
            match tokio::time::timeout(Duration::from_secs(20), source.next()).await {
                Ok(Some(Ok(Message::Text(_)))) => replies += 1,
                Ok(Some(Ok(_))) => {}
                _ => break,
            }
        }
        replies
    });
    let writer = tokio::spawn(async move {
        for n in 0..total {
            if sink.feed(Message::text(format!("bad {n}"))).await.is_err() {
                break;
            }
        }
        let _ = sink.flush().await;
        sink
    });
    still_serving(addr).await;
    let sink = writer.await.unwrap();
    assert_eq!(reader.await.unwrap(), total);
    drop(sink);
    still_serving(addr).await;
}

#[tokio::test]
async fn ws_flood_from_a_client_that_never_reads_is_bounded() {
    let addr = spawn(Arc::new(ScriptedProvider::new())).await;
    let mut socket = joined(addr).await;
    let total = 400_000;
    let started = Instant::now();
    let mut sent = 0;
    let mut stalled = false;
    while sent < total {
        let batch = (0..2_000).map(|_| Ok(Message::text("x")));
        let mut batch = stream::iter(batch);
        match tokio::time::timeout(Duration::from_secs(3), socket.send_all(&mut batch)).await {
            Ok(Ok(())) => sent += 2_000,
            Ok(Err(_)) => break,
            Err(_) => {
                stalled = true;
                break;
            }
        }
    }
    let closed = sent < total && !stalled;
    assert!(
        stalled || closed,
        "the server accepted all {sent} messages in {:?} while the client read none of the replies",
        started.elapsed()
    );
}

#[tokio::test]
async fn ws_user_say_flood_is_bounded_per_session() {
    let provider = Arc::new(SlowProvider {
        calls: AtomicUsize::new(0),
        delay: Duration::from_millis(50),
    });
    let addr = spawn(provider.clone()).await;
    let mut socket = joined(addr).await;
    let total = 300;
    for n in 0..total {
        socket
            .feed(Message::text(
                json!({ "v": 1, "t": "user.say", "text": format!("message {n}") }).to_string(),
            ))
            .await
            .unwrap();
    }
    socket.flush().await.unwrap();
    let mut refused = 0;
    let mut closed = false;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), socket.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let value: Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                if value["t"] == "agent.error" {
                    refused += 1;
                }
            }
            Ok(Some(Ok(Message::Close(_))) | None | Some(Err(_))) => {
                closed = true;
                break;
            }
            _ => {}
        }
    }
    let calls = provider.calls.load(Ordering::SeqCst);
    assert!(
        closed || refused >= total - 32,
        "{total} messages: {refused} refused, {calls} model calls so far, and the rest are queued"
    );
}

#[tokio::test]
async fn ws_sessions_count_against_max_sessions() {
    let agent = Agent::new(
        Arc::new(BillsHost::new()),
        Arc::new(ScriptedProvider::new()),
    );
    let addr = spawn_with(
        agent,
        ServerConfig {
            max_sessions: 2,
            ..ServerConfig::default()
        },
    )
    .await;
    let mut welcomed = 0;
    let mut sockets = Vec::new();
    for _ in 0..4 {
        let Ok((mut socket, _)) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws")).await
        else {
            continue;
        };
        if socket.send(Message::text(hello())).await.is_ok()
            && read(&mut socket)
                .await
                .is_some_and(|reply| reply["t"] == "welcome")
        {
            welcomed += 1;
        }
        sockets.push(socket);
    }
    assert!(
        welcomed <= 2,
        "{welcomed} WebSocket sessions were opened with max_sessions = 2"
    );
}

#[tokio::test]
async fn http_send_refuses_invalid_utf8_bodies() {
    let addr = spawn(Arc::new(ScriptedProvider::new())).await;
    let http = reqwest::Client::new();
    let invalid = vec![b'{', 0xff, 0xfe, b'}'];
    let start = http
        .post(format!("http://{addr}/send"))
        .body(invalid.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(start.status(), 400);
    let welcome: Value = http
        .post(format!("http://{addr}/send"))
        .body(hello())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session = welcome["session"].as_str().unwrap();
    let inside = http
        .post(format!("http://{addr}/send?session={session}"))
        .body(invalid)
        .send()
        .await
        .unwrap();
    assert_eq!(inside.status(), 400);
}

#[tokio::test]
async fn http_send_reports_invalid_and_deep_json_without_reaching_the_model() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone()).await;
    let http = reqwest::Client::new();
    for bad_hello in ["{", "null", deep(10_000).as_str()] {
        let response = http
            .post(format!("http://{addr}/send"))
            .body(bad_hello.to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["reason"], "bad_message");
    }
    let welcome: Value = http
        .post(format!("http://{addr}/send"))
        .body(hello())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session = welcome["session"].as_str().unwrap().to_string();
    let events = http
        .get(format!("http://{addr}/events?session={session}"))
        .send()
        .await
        .unwrap();
    let bodies = ["{".to_string(), deep(100_000), "[]".to_string()];
    for body in &bodies {
        let accepted = http
            .post(format!("http://{addr}/send?session={session}"))
            .body(body.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(accepted.status(), 202);
    }
    let too_big = http
        .post(format!("http://{addr}/send?session={session}"))
        .body("x".repeat(300 * 1024))
        .send()
        .await
        .unwrap();
    assert_eq!(too_big.status(), 413);
    let mut stream = events.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut errors = 0;
    while errors < bodies.len() {
        let chunk = tokio::time::timeout(WAIT, stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        for data in decoder.push(&chunk) {
            let value: Value = serde_json::from_str(&data).unwrap();
            assert_eq!(value["t"], "ui.error");
            assert_eq!(value["reason"], "bad_message");
            errors += 1;
        }
    }
    assert!(provider.requests().is_empty());
}

#[tokio::test]
async fn sessions_are_bound_to_the_user_who_opened_them() {
    let agent = Agent::new(
        Arc::new(HeaderHost {
            inner: BillsHost::new(),
        }),
        Arc::new(ScriptedProvider::new()),
    );
    let addr = spawn_with(agent, ServerConfig::default()).await;
    let http = reqwest::Client::new();
    let anonymous = http
        .post(format!("http://{addr}/send"))
        .body(hello())
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), 401);
    let welcome: Value = http
        .post(format!("http://{addr}/send"))
        .header("x-user", "alice")
        .body(hello())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session = welcome["session"].as_str().unwrap().to_string();
    let stolen_events = http
        .get(format!("http://{addr}/events?session={session}"))
        .header("x-user", "mallory")
        .send()
        .await
        .unwrap();
    assert_eq!(stolen_events.status(), 404);
    let stolen_send = http
        .post(format!("http://{addr}/send?session={session}"))
        .header("x-user", "mallory")
        .body(json!({ "v": 1, "t": "user.say", "text": "pay everything" }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(stolen_send.status(), 404);
    let own = http
        .get(format!("http://{addr}/events?session={session}"))
        .header("x-user", "alice")
        .send()
        .await
        .unwrap();
    assert_eq!(own.status(), 200);
    let refused = tokio_tungstenite::connect_async(format!("ws://{addr}/ws")).await;
    assert!(
        refused.is_err(),
        "a WebSocket without a user must be refused"
    );
}

#[tokio::test]
async fn a_disconnect_cancels_the_running_turn() {
    let provider = Arc::new(HangingProvider {
        calls: AtomicUsize::new(0),
        dropped: Arc::new(AtomicBool::new(false)),
    });
    let addr = spawn(provider.clone()).await;
    let mut socket = joined(addr).await;
    socket
        .send(Message::text(
            json!({ "v": 1, "t": "user.say", "text": "hi" }).to_string(),
        ))
        .await
        .unwrap();
    let deadline = Instant::now() + WAIT;
    while provider.calls.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert!(!provider.dropped.load(Ordering::SeqCst));
    drop(socket);
    let deadline = Instant::now() + WAIT;
    while !provider.dropped.load(Ordering::SeqCst) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        provider.dropped.load(Ordering::SeqCst),
        "the model stream kept running after the client left"
    );
}

#[tokio::test]
async fn sse_and_websocket_sessions_share_max_sessions() {
    let agent = Agent::new(
        Arc::new(BillsHost::new()),
        Arc::new(ScriptedProvider::new()),
    );
    let addr = spawn_with(
        agent,
        ServerConfig {
            max_sessions: 1,
            ..ServerConfig::default()
        },
    )
    .await;
    let socket = joined(addr).await;
    let http = reqwest::Client::new();
    let refused = http
        .post(format!("http://{addr}/send"))
        .body(hello())
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 503);
    drop(socket);
    let deadline = Instant::now() + WAIT;
    loop {
        let status = http
            .post(format!("http://{addr}/send"))
            .body(hello())
            .send()
            .await
            .unwrap()
            .status();
        if status == 200 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the closed WebSocket session was never released"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
            .await
            .is_err()
    );
}
