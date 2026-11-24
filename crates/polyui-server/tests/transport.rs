use futures_util::{SinkExt, StreamExt};
use polyui_server::anthropic::SseDecoder;
use polyui_server::bills::BillsHost;
use polyui_server::{Agent, ScriptedProvider, ScriptedReply, ServerConfig};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

fn show_card() -> Value {
    json!({
        "root": "card",
        "components": [
            { "id": "card", "type": "Card", "props": { "title": "Gas" }, "children": ["due"] },
            { "id": "due", "type": "Stat", "props": { "label": "Due", "value": "29 322,19", "unit": "so'm" } }
        ]
    })
}

async fn spawn(provider: Arc<ScriptedProvider>, static_dir: Option<PathBuf>) -> SocketAddr {
    let agent = Agent::new(Arc::new(BillsHost::new()), provider);
    let app = polyui_server::router(
        agent,
        ServerConfig {
            static_dir,
            ..ServerConfig::default()
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(polyui_server::serve(listener, app));
    addr
}

fn hello() -> String {
    json!({ "v": 1, "t": "hello", "client": { "name": "test", "version": "0" }, "catalogs": ["core@1", "chart@1"], "modalities": ["screen"], "locale": "uz-Latn" })
        .to_string()
}

#[tokio::test]
async fn websocket_end_to_end() {
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(
        ScriptedReply::new()
            .text("Mana.")
            .tool("t1", "show_ui", show_card()),
    );
    provider.reply(ScriptedReply::new().text("Tayyor."));
    let addr = spawn(provider.clone(), None).await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    socket.send(Message::text(hello())).await.unwrap();
    let read =
        |message: Message| -> Value { serde_json::from_str(message.to_text().unwrap()).unwrap() };
    let welcome = read(socket.next().await.unwrap().unwrap());
    assert_eq!(welcome["t"], "welcome");
    assert_eq!(welcome["catalogs"], json!(["core@1", "chart@1"]));
    assert!(welcome["session"].as_str().unwrap().starts_with("sess_"));

    socket
        .send(Message::text(
            json!({ "v": 1, "t": "user.say", "text": "Gaz hisobimni ko'rsat" }).to_string(),
        ))
        .await
        .unwrap();
    let mut kinds = Vec::new();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let value = read(message);
        let kind = value["t"].as_str().unwrap().to_string();
        if kind == "ui.open" {
            assert_eq!(value["fallback"], "Gas. Due: 29 322,19 so'm");
        }
        kinds.push(kind.clone());
        if kind == "agent.done" {
            break;
        }
    }
    assert_eq!(
        kinds,
        ["agent.delta", "ui.open", "agent.delta", "agent.done"]
    );

    socket
        .send(Message::text(r#"{"v":1,"t":"ui.resync","surface":"s1"}"#))
        .await
        .unwrap();
    let snapshot = read(socket.next().await.unwrap().unwrap());
    assert_eq!(snapshot["t"], "ui.snapshot");

    let huge = format!(
        "{{\"v\":1,\"t\":\"user.say\",\"text\":\"{}\"}}",
        "a".repeat(300 * 1024)
    );
    let _ = socket.send(Message::text(huge)).await;
    let after = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap();
    assert!(
        !matches!(after, Some(Ok(Message::Text(_)))),
        "an oversized frame must not be processed: {after:?}"
    );
    assert_eq!(provider.requests().len(), 2);
}

#[tokio::test]
async fn websocket_requires_hello_first() {
    let addr = spawn(Arc::new(ScriptedProvider::new()), None).await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    socket
        .send(Message::text(r#"{"v":1,"t":"user.say","text":"hi"}"#))
        .await
        .unwrap();
    let reply: Value =
        serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(reply["t"], "ui.error");
    assert_eq!(reply["reason"], "bad_message");
}

#[tokio::test]
async fn sse_and_post_end_to_end() {
    let provider = Arc::new(ScriptedProvider::new());
    provider.reply(ScriptedReply::new().tool("t1", "show_ui", show_card()));
    provider.reply(ScriptedReply::new().text("Done."));
    let addr = spawn(provider.clone(), None).await;
    let http = reqwest::Client::new();
    let welcome: Value = http
        .post(format!("http://{addr}/send"))
        .header("content-type", "application/json")
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
    assert_eq!(events.status(), 200);
    let second = http
        .get(format!("http://{addr}/events?session={session}"))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 409);

    let accepted = http
        .post(format!("http://{addr}/send?session={session}"))
        .header("content-type", "application/json")
        .body(json!({ "v": 1, "t": "user.say", "text": "show gas" }).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), 202);

    let mut stream = events.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut kinds = Vec::new();
    while !kinds.contains(&"agent.done".to_string()) {
        let chunk = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        for data in decoder.push(&chunk) {
            let value: Value = serde_json::from_str(&data).unwrap();
            kinds.push(value["t"].as_str().unwrap().to_string());
        }
    }
    assert_eq!(kinds, ["ui.open", "agent.delta", "agent.done"]);

    let too_big = http
        .post(format!("http://{addr}/send?session={session}"))
        .header("content-type", "application/json")
        .body("x".repeat(300 * 1024))
        .send()
        .await
        .unwrap();
    assert_eq!(too_big.status(), 413);
    let unknown = http
        .post(format!("http://{addr}/send?session=sess_nope"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 404);
}

#[tokio::test]
async fn static_files_are_served() {
    let dir = std::env::temp_dir().join(format!("polyui-static-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.html"), "<p>polyui</p>").unwrap();
    let addr = spawn(Arc::new(ScriptedProvider::new()), Some(dir.clone())).await;
    let body = reqwest::get(format!("http://{addr}/"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "<p>polyui</p>");
    let missing = reqwest::get(format!("http://{addr}/nope.js"))
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn config_names_the_embed_sandbox_and_the_sandbox_has_its_own_server() {
    let plain = spawn(Arc::new(ScriptedProvider::new()), None).await;
    let config: Value = reqwest::get(format!("http://{plain}/config.json"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(config, json!({ "embed_origin": null }));

    let agent = Agent::new(
        Arc::new(BillsHost::new().with_tariff_app()),
        Arc::new(ScriptedProvider::new()),
    );
    let app = polyui_server::router(
        agent,
        ServerConfig {
            embed_origin: Some("http://localhost:8788/".into()),
            ..ServerConfig::default()
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(polyui_server::serve(listener, app));
    let response = reqwest::get(format!("http://{addr}/config.json"))
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], "application/json");
    let config: Value = response.json().await.unwrap();
    assert_eq!(config["embed_origin"], "http://localhost:8788/");

    let dir = std::env::temp_dir().join(format!("polyui-sandbox-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.html"), "<p>sandbox</p>").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let sandbox = listener.local_addr().unwrap();
    tokio::spawn(polyui_server::serve(
        listener,
        polyui_server::static_router(dir.clone()),
    ));
    let response = reqwest::get(format!("http://{sandbox}/")).await.unwrap();
    assert!(response.headers().get("content-security-policy").is_none());
    assert_eq!(response.text().await.unwrap(), "<p>sandbox</p>");
    std::fs::remove_dir_all(dir).unwrap();
}
