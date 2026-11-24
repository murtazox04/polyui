use crate::host::User;
use crate::session::{Agent, Session};
use axum::Router;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::{SinkExt, Stream, StreamExt};
use polyui_core::protocol::{self, ErrorReason};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower_http::services::ServeDir;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub static_dir: Option<PathBuf>,
    pub max_sessions: usize,
    pub attach_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            static_dir: None,
            max_sessions: 1000,
            attach_timeout: Duration::from_secs(60),
        }
    }
}

struct Slot {
    session: Arc<Session>,
    outgoing: Option<mpsc::UnboundedReceiver<Value>>,
}

struct AppState {
    agent: Agent,
    config: ServerConfig,
    sessions: Mutex<HashMap<String, Slot>>,
}

impl AppState {
    fn sessions(&self) -> std::sync::MutexGuard<'_, HashMap<String, Slot>> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn message_limit(&self) -> usize {
        self.agent.config().limits.max_message_bytes
    }

    fn authenticate(&self, headers: &HeaderMap) -> Result<User, String> {
        self.agent.host().authenticate(headers)
    }
}

#[derive(Debug, Deserialize)]
struct SessionQuery {
    session: Option<String>,
}

pub fn router(agent: Agent, config: ServerConfig) -> Router {
    let limit = agent.config().limits.max_message_bytes;
    let static_dir = config.static_dir.clone();
    let state = Arc::new(AppState {
        agent,
        config,
        sessions: Mutex::new(HashMap::new()),
    });
    let app = Router::new()
        .route("/ws", get(websocket))
        .route("/events", get(events))
        .route("/send", post(send))
        .layer(DefaultBodyLimit::max(limit))
        .with_state(state);
    match static_dir {
        Some(dir) => app.fallback_service(ServeDir::new(dir)),
        None => app,
    }
}

pub async fn serve(listener: TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(listener, app).await
}

fn json_response(status: StatusCode, value: &Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        value.to_string(),
    )
        .into_response()
}

fn bad_message(detail: String) -> Value {
    protocol::error_value(None, ErrorReason::BadMessage, Some(detail))
}

async fn websocket(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let user = match state.authenticate(&headers) {
        Ok(user) => user,
        Err(message) => return (StatusCode::UNAUTHORIZED, message).into_response(),
    };
    let limit = state.message_limit();
    upgrade
        .max_message_size(limit)
        .max_frame_size(limit)
        .on_upgrade(move |socket| run_socket(state, socket, user))
}

async fn run_socket(state: Arc<AppState>, mut socket: WebSocket, user: User) {
    let limits = state.agent.config().limits;
    let hello = loop {
        match socket.recv().await {
            Some(Ok(WsMessage::Text(text))) => {
                match protocol::parse_hello(text.as_str(), &limits) {
                    Ok(hello) => break hello,
                    Err(error) => {
                        let reply = bad_message(error.to_string()).to_string();
                        let _ = socket.send(WsMessage::Text(reply.into())).await;
                        let _ = socket.send(WsMessage::Close(None)).await;
                        return;
                    }
                }
            }
            Some(Ok(WsMessage::Ping(_) | WsMessage::Pong(_))) => continue,
            _ => return,
        }
    };
    let (session, mut outgoing) = state.agent.open_session(hello, user);
    if socket
        .send(WsMessage::Text(session.welcome().to_string().into()))
        .await
        .is_err()
    {
        return;
    }
    let (mut sink, mut stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(message) = outgoing.recv().await {
            if sink
                .send(WsMessage::Text(message.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        match message {
            WsMessage::Text(text) => session.receive(text.as_str()).await,
            WsMessage::Binary(_) => session.send_bad_message("binary frames are not supported"),
            WsMessage::Close(_) => break,
            WsMessage::Ping(_) | WsMessage::Pong(_) => {}
        }
    }
    writer.abort();
}

async fn send(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let user = match state.authenticate(&headers) {
        Ok(user) => user,
        Err(message) => return (StatusCode::UNAUTHORIZED, message).into_response(),
    };
    let Some(id) = query.session else {
        return start_session(&state, user, &body);
    };
    let session = state
        .sessions()
        .get(&id)
        .filter(|slot| slot.session.user().id == user.id)
        .map(|slot| slot.session.clone());
    let Some(session) = session else {
        return (StatusCode::NOT_FOUND, "unknown session").into_response();
    };
    session.receive(&body).await;
    StatusCode::ACCEPTED.into_response()
}

fn start_session(state: &Arc<AppState>, user: User, body: &str) -> Response {
    let limits = state.agent.config().limits;
    let hello = match protocol::parse_hello(body, &limits) {
        Ok(hello) => hello,
        Err(error) => {
            return json_response(StatusCode::BAD_REQUEST, &bad_message(error.to_string()));
        }
    };
    if state.sessions().len() >= state.config.max_sessions {
        return (StatusCode::SERVICE_UNAVAILABLE, "too many sessions").into_response();
    }
    let (session, outgoing) = state.agent.open_session(hello, user);
    let id = session.id().to_string();
    let welcome = session.welcome();
    state.sessions().insert(
        id.clone(),
        Slot {
            session,
            outgoing: Some(outgoing),
        },
    );
    let waiting = Arc::downgrade(state);
    let timeout = state.config.attach_timeout;
    tokio::spawn(async move {
        tokio::time::sleep(timeout).await;
        if let Some(state) = waiting.upgrade() {
            let mut sessions = state.sessions();
            if sessions
                .get(&id)
                .is_some_and(|slot| slot.outgoing.is_some())
            {
                sessions.remove(&id);
            }
        }
    });
    json_response(StatusCode::OK, &welcome)
}

struct SessionEvents {
    inner: UnboundedReceiverStream<Value>,
    state: Arc<AppState>,
    id: String,
}

impl Stream for SessionEvents {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner)
            .poll_next(cx)
            .map(|item| item.map(|value| Ok(Event::default().data(value.to_string()))))
    }
}

impl Drop for SessionEvents {
    fn drop(&mut self) {
        self.state.sessions().remove(&self.id);
    }
}

async fn events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
    headers: HeaderMap,
) -> Response {
    let user = match state.authenticate(&headers) {
        Ok(user) => user,
        Err(message) => return (StatusCode::UNAUTHORIZED, message).into_response(),
    };
    let Some(id) = query.session else {
        return (StatusCode::BAD_REQUEST, "missing session").into_response();
    };
    let outgoing = {
        let mut sessions = state.sessions();
        match sessions.get_mut(&id) {
            Some(slot) if slot.session.user().id == user.id => match slot.outgoing.take() {
                Some(outgoing) => outgoing,
                None => {
                    return (StatusCode::CONFLICT, "events are already streaming").into_response();
                }
            },
            _ => return (StatusCode::NOT_FOUND, "unknown session").into_response(),
        }
    };
    let stream = SessionEvents {
        inner: UnboundedReceiverStream::new(outgoing),
        state: state.clone(),
        id,
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
