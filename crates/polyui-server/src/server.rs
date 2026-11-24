use crate::host::User;
use crate::origin;
use crate::session::{Agent, CloseReason, Outgoing, Session};
use axum::Router;
use axum::extract::ws::{
    CloseFrame, Message as WsMessage, WebSocket, WebSocketUpgrade, close_code,
};
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::stream::{self, SplitSink};
use futures_util::{SinkExt, StreamExt};
use polyui_core::protocol::{self, ErrorReason, Hello};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tower_http::services::ServeDir;

const CLOSE_WAIT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub static_dir: Option<PathBuf>,
    pub max_sessions: usize,
    pub attach_timeout: Duration,
    pub embed_origin: Option<String>,
    pub allowed_origins: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            static_dir: None,
            max_sessions: 1000,
            attach_timeout: Duration::from_secs(60),
            embed_origin: None,
            allowed_origins: Vec::new(),
        }
    }
}

struct Slot {
    session: Arc<Session>,
    outgoing: Option<Outgoing>,
}

struct AppState {
    agent: Agent,
    config: ServerConfig,
    sessions: Mutex<HashMap<String, Slot>>,
    capacity: Arc<Semaphore>,
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

    fn admit(&self) -> Option<OwnedSemaphorePermit> {
        self.capacity.clone().try_acquire_owned().ok()
    }

    fn refuse_origin(&self, headers: &HeaderMap, uri: &Uri) -> Option<Response> {
        let sent = headers.get(header::ORIGIN)?;
        let host = headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .or_else(|| uri.authority().map(|authority| authority.as_str()));
        let fits = sent
            .to_str()
            .is_ok_and(|sent| origin::allowed(sent, host, &self.config.allowed_origins));
        (!fits).then(|| (StatusCode::FORBIDDEN, "this origin is not allowed").into_response())
    }
}

fn too_many_sessions() -> Response {
    (StatusCode::SERVICE_UNAVAILABLE, "too many sessions").into_response()
}

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
}

#[derive(Debug, Deserialize)]
struct SessionQuery {
    session: Option<String>,
}

fn checked_origins(listed: &[String]) -> Vec<String> {
    listed
        .iter()
        .filter_map(|entry| match origin::normalize(entry) {
            Ok(normal) => Some(normal),
            Err(message) => {
                eprintln!("polyui: ignoring an allowed origin: {message}");
                None
            }
        })
        .collect()
}

pub fn router(agent: Agent, mut config: ServerConfig) -> Router {
    config.allowed_origins = checked_origins(&config.allowed_origins);
    let limit = agent.config().limits.max_message_bytes;
    let static_dir = config.static_dir.clone();
    let capacity = Arc::new(Semaphore::new(
        config.max_sessions.min(Semaphore::MAX_PERMITS),
    ));
    let state = Arc::new(AppState {
        agent,
        config,
        sessions: Mutex::new(HashMap::new()),
        capacity,
    });
    let app = Router::new()
        .route("/ws", get(websocket))
        .route("/events", get(events))
        .route("/send", post(send))
        .route("/config.json", get(client_config))
        .layer(DefaultBodyLimit::max(limit))
        .with_state(state);
    match static_dir {
        Some(dir) => app.fallback_service(ServeDir::new(dir)),
        None => app,
    }
}

pub fn static_router(dir: PathBuf) -> Router {
    Router::new().fallback_service(ServeDir::new(dir))
}

pub async fn serve(listener: TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(listener, app).await
}

async fn client_config(State(state): State<Arc<AppState>>) -> Response {
    json_response(
        StatusCode::OK,
        &json!({ "embed_origin": state.config.embed_origin }),
    )
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
    uri: Uri,
    upgrade: WebSocketUpgrade,
) -> Response {
    if let Some(refused) = state.refuse_origin(&headers, &uri) {
        return refused;
    }
    let user = match state.authenticate(&headers) {
        Ok(user) => user,
        Err(message) => return (StatusCode::UNAUTHORIZED, message).into_response(),
    };
    let Some(permit) = state.admit() else {
        return too_many_sessions();
    };
    let limit = state.message_limit();
    upgrade
        .max_message_size(limit)
        .max_frame_size(limit)
        .on_upgrade(move |socket| run_socket(state, socket, user, permit))
}

async fn run_socket(
    state: Arc<AppState>,
    mut socket: WebSocket,
    user: User,
    permit: OwnedSemaphorePermit,
) {
    let wait = state.config.attach_timeout;
    let hello = match tokio::time::timeout(wait, read_hello(&state, &mut socket)).await {
        Ok(Some(hello)) => hello,
        Ok(None) => return,
        Err(_) => {
            let frame = CloseFrame {
                code: close_code::POLICY,
                reason: "no hello in time".into(),
            };
            let _ =
                tokio::time::timeout(CLOSE_WAIT, socket.send(WsMessage::Close(Some(frame)))).await;
            return;
        }
    };
    let (session, outgoing) = state.agent.open_session(hello, user);
    if socket
        .send(WsMessage::Text(session.welcome().to_string().into()))
        .await
        .is_err()
    {
        session.close(CloseReason::ClientGone);
        return;
    }
    let (sink, mut stream) = socket.split();
    let writer = tokio::spawn(write_socket(session.clone(), sink, outgoing));
    loop {
        let next = tokio::select! {
            biased;
            _ = session.closed() => break,
            next = stream.next() => next,
        };
        match next {
            Some(Ok(WsMessage::Text(text))) => session.receive(text.as_str()).await,
            Some(Ok(WsMessage::Binary(_))) => {
                session.send_bad_message("binary frames are not supported");
            }
            Some(Ok(WsMessage::Ping(_) | WsMessage::Pong(_))) => {}
            Some(Ok(WsMessage::Close(_)) | Err(_)) | None => break,
        }
    }
    session.close(CloseReason::ClientGone);
    let _ = writer.await;
    drop(permit);
}

async fn read_hello(state: &AppState, socket: &mut WebSocket) -> Option<Hello> {
    let limits = state.agent.config().limits;
    loop {
        match socket.recv().await {
            Some(Ok(WsMessage::Text(text))) => {
                return match protocol::parse_hello(text.as_str(), &limits) {
                    Ok(hello) => Some(hello),
                    Err(error) => {
                        let reply = bad_message(error.to_string()).to_string();
                        let _ = socket.send(WsMessage::Text(reply.into())).await;
                        let _ = socket.send(WsMessage::Close(None)).await;
                        None
                    }
                };
            }
            Some(Ok(WsMessage::Ping(_) | WsMessage::Pong(_))) => continue,
            _ => return None,
        }
    }
}

async fn write_socket(
    session: Arc<Session>,
    mut sink: SplitSink<WebSocket, WsMessage>,
    mut outgoing: Outgoing,
) {
    loop {
        let next = tokio::select! {
            biased;
            reason = session.closed() => Err(reason),
            message = outgoing.recv() => message.ok_or(CloseReason::ClientGone),
        };
        let reason = match next {
            Ok(message) => {
                let sent = tokio::select! {
                    biased;
                    reason = session.closed() => Err(reason),
                    sent = sink.send(WsMessage::Text(message.to_string().into())) => {
                        sent.map_err(|_| CloseReason::ClientGone)
                    }
                };
                match sent {
                    Ok(()) => continue,
                    Err(reason) => reason,
                }
            }
            Err(reason) => reason,
        };
        if reason == CloseReason::SlowConsumer {
            let frame = CloseFrame {
                code: close_code::POLICY,
                reason: reason.detail().into(),
            };
            let _ =
                tokio::time::timeout(CLOSE_WAIT, sink.send(WsMessage::Close(Some(frame)))).await;
        }
        session.close(reason);
        return;
    }
}

async fn send(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
    headers: HeaderMap,
    uri: Uri,
    body: String,
) -> Response {
    if let Some(refused) = state.refuse_origin(&headers, &uri) {
        return refused;
    }
    if !is_json(&headers) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "send the message with content-type: application/json",
        )
            .into_response();
    }
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
    let Some(permit) = state.admit() else {
        return too_many_sessions();
    };
    let (session, outgoing) = state.agent.open_session(hello, user);
    let id = session.id().to_string();
    let welcome = session.welcome();
    let counted = session.clone();
    tokio::spawn(async move {
        counted.closed().await;
        drop(permit);
    });
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
                && let Some(slot) = sessions.remove(&id)
            {
                slot.session.close(CloseReason::ClientGone);
            }
        }
    });
    json_response(StatusCode::OK, &welcome)
}

struct EventFeed {
    session: Arc<Session>,
    outgoing: Outgoing,
    state: Arc<AppState>,
    id: String,
    finished: bool,
}

impl Drop for EventFeed {
    fn drop(&mut self) {
        self.session.close(CloseReason::ClientGone);
        self.state.sessions().remove(&self.id);
    }
}

async fn next_event(mut feed: EventFeed) -> Option<(Result<Event, Infallible>, EventFeed)> {
    if feed.finished {
        return None;
    }
    let event = tokio::select! {
        biased;
        reason = feed.session.closed() => {
            feed.finished = true;
            Event::default().event("close").data(reason.detail())
        }
        message = feed.outgoing.recv() => Event::default().data(message?.to_string()),
    };
    Some((Ok(event), feed))
}

async fn events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SessionQuery>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(refused) = state.refuse_origin(&headers, &uri) {
        return refused;
    }
    let user = match state.authenticate(&headers) {
        Ok(user) => user,
        Err(message) => return (StatusCode::UNAUTHORIZED, message).into_response(),
    };
    let Some(id) = query.session else {
        return (StatusCode::BAD_REQUEST, "missing session").into_response();
    };
    let (session, outgoing) = {
        let mut sessions = state.sessions();
        match sessions.get_mut(&id) {
            Some(slot) if slot.session.user().id == user.id => match slot.outgoing.take() {
                Some(outgoing) => (slot.session.clone(), outgoing),
                None => {
                    return (StatusCode::CONFLICT, "events are already streaming").into_response();
                }
            },
            _ => return (StatusCode::NOT_FOUND, "unknown session").into_response(),
        }
    };
    let feed = EventFeed {
        session,
        outgoing,
        state: state.clone(),
        id,
        finished: false,
    };
    Sse::new(stream::unfold(feed, next_event))
        .keep_alive(KeepAlive::default())
        .into_response()
}
