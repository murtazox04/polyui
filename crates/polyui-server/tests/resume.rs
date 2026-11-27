use futures_util::{SinkExt, StreamExt};
use polyui_server::anthropic::SseDecoder;
use polyui_server::bills::BillsHost;
use polyui_server::{Agent, ScriptedProvider, ScriptedReply, ServerConfig};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const WAIT: Duration = Duration::from_secs(5);

fn bill_surface() -> Value {
    json!({
        "root": "card",
        "components": [
            { "id": "card", "type": "Card", "props": { "title": "Gas bill" }, "children": ["due", "pay"] },
            { "id": "due", "type": "Stat", "props": { "label": "Due", "value": "29 322,19", "unit": "so'm" } },
            { "id": "pay", "type": "Button", "props": { "label": "Pay", "tone": "primary", "on_press": { "action": "pay_bill", "args": { "bill": "b_gas_2611" } } } }
        ]
    })
}

async fn spawn(provider: Arc<ScriptedProvider>, resume_window: Duration) -> SocketAddr {
    let agent = Agent::new(Arc::new(BillsHost::new()), provider);
    let app = polyui_server::router(
        agent,
        ServerConfig {
            resume_window,
            ..ServerConfig::default()
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(polyui_server::serve(listener, app));
    addr
}

fn hello(features: &[&str], resume: Option<(&str, &str)>) -> String {
    let mut hello = json!({ "v": 1, "t": "hello", "catalogs": ["core@1"], "modalities": ["screen"], "features": features, "locale": "en" });
    if let Some((session, token)) = resume {
        hello["resume"] = json!({ "session": session, "token": token });
    }
    hello.to_string()
}

async fn connect(addr: SocketAddr, hello: String) -> (Socket, Value) {
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    socket.send(Message::text(hello)).await.unwrap();
    let welcome = next(&mut socket).await;
    assert_eq!(welcome["t"], "welcome");
    (socket, welcome)
}

async fn next(socket: &mut Socket) -> Value {
    loop {
        let message = tokio::time::timeout(WAIT, socket.next())
            .await
            .expect("a message in time")
            .expect("the socket is open")
            .expect("a frame");
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

async fn send(socket: &mut Socket, message: Value) {
    socket
        .send(Message::text(message.to_string()))
        .await
        .unwrap();
}

async fn until(socket: &mut Socket, kind: &str) -> Vec<Value> {
    let mut seen = Vec::new();
    loop {
        let message = next(socket).await;
        let done = message["t"] == kind;
        seen.push(message);
        if done {
            return seen;
        }
    }
}

fn handle(open: &Value, component: &str) -> String {
    open["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == component)
        .and_then(|item| item["props"]["on_press"]["action_id"].as_str())
        .unwrap()
        .to_string()
}

fn text(welcome: &Value, key: &str) -> String {
    welcome[key].as_str().unwrap().to_string()
}

async fn open_bill(provider: &ScriptedProvider, socket: &mut Socket) -> Value {
    provider.reply(ScriptedReply::new().text("Here is your gas bill.").tool(
        "t1",
        "show_ui",
        bill_surface(),
    ));
    provider.reply(ScriptedReply::new().text("Tap Pay when you are ready."));
    send(
        socket,
        json!({ "v": 1, "t": "user.say", "text": "show my gas bill" }),
    )
    .await;
    let turn = until(socket, "agent.done").await;
    turn.into_iter()
        .find(|message| message["t"] == "ui.open")
        .expect("the bill opens")
}

#[tokio::test]
async fn a_resumed_socket_gets_its_surfaces_handles_and_confirmation_back() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone(), Duration::from_secs(10)).await;
    let (mut socket, welcome) = connect(addr, hello(&["resume"], None)).await;
    let session = text(&welcome, "session");
    let token = text(&welcome, "resume_token");
    assert!(token.starts_with("rt_"));
    assert!(welcome.get("resumed").is_none());

    let open = open_bill(&provider, &mut socket).await;
    let pay = handle(&open, "pay");
    send(
        &mut socket,
        json!({ "v": 1, "t": "ui.action", "surface": "s1", "action_id": pay, "source": "tap" }),
    )
    .await;
    let confirm = next(&mut socket).await;
    assert_eq!(confirm["t"], "ui.confirm");
    drop(socket);

    let (mut socket, welcome) = connect(addr, hello(&["resume"], Some((&session, &token)))).await;
    assert_eq!(welcome["resumed"], true);
    assert_eq!(text(&welcome, "session"), session);
    let fresh = text(&welcome, "resume_token");
    assert_ne!(fresh, token);
    let snapshot = next(&mut socket).await;
    assert_eq!(snapshot["t"], "ui.snapshot");
    assert_eq!(snapshot["surface"], "s1");
    assert_eq!(snapshot["seq"], open["seq"]);
    assert_eq!(handle(&snapshot, "pay"), pay);
    let again = next(&mut socket).await;
    assert_eq!(again["t"], "ui.confirm");
    assert_eq!(again["nonce"], confirm["nonce"]);
    assert!(again["expires_ms"].as_u64().unwrap() <= 60_000);

    provider.reply(ScriptedReply::new().text("Paid."));
    send(
        &mut socket,
        json!({ "v": 1, "t": "ui.action", "surface": "s1", "action_id": pay, "confirm": confirm["nonce"], "source": "tap" }),
    )
    .await;
    let result = next(&mut socket).await;
    assert_eq!(result["t"], "ui.action.result");
    assert_eq!(result["status"], "ok");
    drop(socket);

    let (socket, welcome) = connect(addr, hello(&["resume"], Some((&session, &token)))).await;
    assert_eq!(welcome["resumed"], false, "an old token works only once");
    assert_ne!(text(&welcome, "session"), session);
    drop(socket);
}

#[tokio::test]
async fn a_wrong_token_or_a_closed_window_starts_a_new_session() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone(), Duration::from_millis(300)).await;
    let (socket, welcome) = connect(addr, hello(&["resume"], None)).await;
    let session = text(&welcome, "session");
    let token = text(&welcome, "resume_token");
    drop(socket);

    let (socket, refused) = connect(addr, hello(&["resume"], Some((&session, "rt_guess")))).await;
    assert_eq!(refused["resumed"], false);
    assert_ne!(text(&refused, "session"), session);
    drop(socket);

    tokio::time::sleep(Duration::from_millis(900)).await;
    let (socket, late) = connect(addr, hello(&["resume"], Some((&session, &token)))).await;
    assert_eq!(late["resumed"], false);
    assert_ne!(text(&late, "session"), session);
    drop(socket);
}

#[tokio::test]
async fn a_client_without_the_feature_gets_no_token_and_no_resume() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone(), Duration::from_secs(10)).await;
    let (socket, welcome) = connect(addr, hello(&[], None)).await;
    assert!(welcome.get("resume_token").is_none());
    let session = text(&welcome, "session");
    drop(socket);
    let (socket, again) = connect(addr, hello(&[], Some((&session, "rt_x")))).await;
    assert!(again.get("resumed").is_none());
    assert_ne!(text(&again, "session"), session);
    drop(socket);
}

#[tokio::test]
async fn a_resume_closes_the_connection_that_is_still_open() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone(), Duration::from_secs(10)).await;
    let (mut first, welcome) = connect(addr, hello(&["resume"], None)).await;
    let session = text(&welcome, "session");
    let token = text(&welcome, "resume_token");
    let (second, resumed) = connect(addr, hello(&["resume"], Some((&session, &token)))).await;
    assert_eq!(resumed["resumed"], true);
    let ended = tokio::time::timeout(WAIT, async {
        loop {
            match first.next().await {
                Some(Ok(Message::Text(_) | Message::Ping(_) | Message::Pong(_))) => continue,
                _ => return,
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "the old connection must end");
    drop(second);
}

#[tokio::test]
async fn a_turn_that_finishes_while_detached_is_shown_after_the_resume() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone(), Duration::from_secs(10)).await;
    let (mut socket, welcome) = connect(addr, hello(&["resume"], None)).await;
    let session = text(&welcome, "session");
    let token = text(&welcome, "resume_token");
    provider.reply(ScriptedReply::new().text("Here is your gas bill.").tool(
        "t1",
        "show_ui",
        bill_surface(),
    ));
    provider.reply(ScriptedReply::new().text("Done."));
    send(
        &mut socket,
        json!({ "v": 1, "t": "user.say", "text": "show my gas bill" }),
    )
    .await;
    drop(socket);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (mut socket, welcome) = connect(addr, hello(&["resume"], Some((&session, &token)))).await;
    assert_eq!(welcome["resumed"], true);
    let mut seen = Vec::new();
    while !seen
        .iter()
        .any(|message: &Value| message["t"] == "ui.snapshot")
    {
        seen.push(next(&mut socket).await);
    }
    let snapshot = seen
        .iter()
        .find(|message| message["t"] == "ui.snapshot")
        .unwrap();
    assert_eq!(snapshot["surface"], "s1");
    assert!(
        !seen.iter().any(|message| message["t"] == "ui.open"),
        "the snapshot replaces the queued open: {seen:?}"
    );
}

#[tokio::test]
async fn an_event_stream_resumes_through_a_new_hello() {
    let provider = Arc::new(ScriptedProvider::new());
    let addr = spawn(provider.clone(), Duration::from_secs(10)).await;
    let http = reqwest::Client::new();
    let post = |body: String, session: Option<String>| {
        let url = match session {
            Some(session) => format!("http://{addr}/send?session={session}"),
            None => format!("http://{addr}/send"),
        };
        http.post(url)
            .header("content-type", "application/json")
            .body(body)
            .send()
    };
    let welcome: Value = post(hello(&["resume"], None), None)
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let session = text(&welcome, "session");
    let token = text(&welcome, "resume_token");
    let events = http
        .get(format!("http://{addr}/events?session={session}"))
        .send()
        .await
        .unwrap();
    provider.reply(ScriptedReply::new().tool("t1", "show_ui", bill_surface()));
    provider.reply(ScriptedReply::new().text("Done."));
    let accepted = post(
        json!({ "v": 1, "t": "user.say", "text": "show my gas bill" }).to_string(),
        Some(session.clone()),
    )
    .await
    .unwrap();
    assert_eq!(accepted.status(), 202);
    let mut stream = events.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut kinds = Vec::new();
    while !kinds.contains(&"agent.done".to_string()) {
        let chunk = tokio::time::timeout(WAIT, stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        for data in decoder.push(&chunk) {
            let value: Value = serde_json::from_str(&data).unwrap();
            kinds.push(value["t"].as_str().unwrap().to_string());
        }
    }
    drop(stream);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let stolen = http
        .get(format!("http://{addr}/events?session={session}"))
        .send()
        .await
        .unwrap();
    assert_eq!(stolen.status(), 404, "a detached session needs its token");

    let resumed: Value = post(hello(&["resume"], Some((&session, &token))), None)
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resumed["resumed"], true);
    assert_eq!(text(&resumed, "session"), session);
    let events = http
        .get(format!("http://{addr}/events?session={session}"))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), 200);
    let mut stream = events.bytes_stream();
    let mut decoder = SseDecoder::default();
    let first = loop {
        let chunk = tokio::time::timeout(WAIT, stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Some(data) = decoder.push(&chunk).into_iter().next() {
            break serde_json::from_str::<Value>(&data).unwrap();
        }
    };
    assert_eq!(first["t"], "ui.snapshot");
    assert_eq!(first["surface"], "s1");
}
