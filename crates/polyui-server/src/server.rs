use crate::host::User;
use crate::origin;
use crate::session::{Agent, CloseReason, Outgoing, RESUME_FEATURE, Session};
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
use polyui_core::protocol::{self, ErrorReason, Hello, ResumeRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};
use tower_http::services::ServeDir;

const CLOSE_WAIT: Duration = Duration::from_secs(1);
const HANDOVER_WAIT: Duration = Duration::from_secs(2);
const HANDOVER_STEP: Duration = Duration::from_millis(20);

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub static_dir: Option<PathBuf>,
    pub max_sessions: usize,
    pub attach_timeout: Duration,
    pub embed_origin: Option<String>,
    pub allowed_origins: Vec<String>,
    pub resume_window: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            static_dir: None,
            max_sessions: 1000,
            attach_timeout: Duration::from_secs(60),
            embed_origin: None,
            allowed_origins: Vec::new(),
            resume_window: Duration::from_secs(120),
        }
    }
}

struct Slot {
    session: Arc<Session>,
    outgoing: Option<Outgoing>,
    origin: Option<String>,
    detached: bool,
    epoch: u64,
    kick: Option<oneshot::Sender<()>>,
    prelude: bool,
    events: bool,
}

impl Slot {
    fn new(
        session: Arc<Session>,
        outgoing: Option<Outgoing>,
        origin: Option<String>,
        events: bool,
    ) -> Self {
        Self {
            session,
            outgoing,
            origin,
            detached: false,
            epoch: 0,
            kick: None,
            prelude: false,
            events,
        }
    }
}

struct Reclaimed {
    session: Arc<Session>,
    outgoing: Outgoing,
    kicked: oneshot::Receiver<()>,
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

    fn keep(self: &Arc<Self>, session: Arc<Session>, slot: Slot, permit: OwnedSemaphorePermit) {
        let id = session.id().to_string();
        self.sessions().insert(id.clone(), slot);
        let state = Arc::downgrade(self);
        tokio::spawn(async move {
            session.closed().await;
            if let Some(state) = state.upgrade() {
                let mut sessions = state.sessions();
                if sessions
                    .get(&id)
                    .is_some_and(|slot| Arc::ptr_eq(&slot.session, &session))
                {
                    sessions.remove(&id);
                }
            }
            drop(permit);
        });
    }

    fn detach(self: &Arc<Self>, session: &Arc<Session>, outgoing: Outgoing) {
        if session.close_reason().is_some() {
            return;
        }
        let epoch = {
            let mut sessions = self.sessions();
            let Some(slot) = sessions
                .get_mut(session.id())
                .filter(|slot| Arc::ptr_eq(&slot.session, session))
            else {
                session.close(CloseReason::ClientGone);
                return;
            };
            slot.outgoing = Some(outgoing);
            slot.detached = true;
            slot.kick = None;
            slot.prelude = false;
            slot.epoch += 1;
            slot.epoch
        };
        self.expire_later(session.clone(), epoch);
    }

    fn expire_later(self: &Arc<Self>, session: Arc<Session>, epoch: u64) {
        let state = Arc::downgrade(self);
        let window = self.config.resume_window;
        tokio::spawn(async move {
            tokio::time::sleep(window).await;
            let Some(state) = state.upgrade() else {
                return;
            };
            let expired = state.sessions().get(session.id()).is_some_and(|slot| {
                Arc::ptr_eq(&slot.session, &session) && slot.epoch == epoch && slot.detached
            });
            if expired {
                session.close(CloseReason::ClientGone);
            }
        });
    }

    async fn reclaim(
        &self,
        request: &ResumeRequest,
        user: &User,
        origin: Option<&str>,
        events: bool,
    ) -> Option<Reclaimed> {
        let session = {
            let mut sessions = self.sessions();
            let slot = sessions.get_mut(&request.session)?;
            let fits = slot.events == events
                && slot.session.user().id == user.id
                && slot.origin.as_deref() == origin
                && slot.session.close_reason().is_none()
                && slot.session.redeem(&request.token);
            if !fits {
                return None;
            }
            if let Some(kick) = slot.kick.take() {
                let _ = kick.send(());
            }
            slot.session.clone()
        };
        let deadline = tokio::time::Instant::now() + HANDOVER_WAIT;
        loop {
            {
                let mut sessions = self.sessions();
                if let Some(slot) = sessions
                    .get_mut(session.id())
                    .filter(|slot| Arc::ptr_eq(&slot.session, &session))
                    && slot.detached
                    && let Some(outgoing) = slot.outgoing.take()
                {
                    let (kick, kicked) = oneshot::channel();
                    slot.detached = false;
                    slot.kick = Some(kick);
                    slot.epoch += 1;
                    return Some(Reclaimed {
                        session: session.clone(),
                        outgoing,
                        kicked,
                    });
                }
            }
            if tokio::time::Instant::now() >= deadline || session.close_reason().is_some() {
                return None;
            }
            tokio::time::sleep(HANDOVER_STEP).await;
        }
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

fn origin_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn wants_resume(hello: &Hello) -> bool {
    hello
        .features
        .iter()
        .any(|feature| feature == RESUME_FEATURE)
}

fn drain(outgoing: &mut Outgoing) -> Vec<Value> {
    let mut queued = Vec::new();
    while let Ok(message) = outgoing.try_recv() {
        queued.push(message);
    }
    queued
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
    let origin = origin_of(&headers);
    let limit = state.message_limit();
    upgrade
        .max_message_size(limit)
        .max_frame_size(limit)
        .on_upgrade(move |socket| run_socket(state, socket, user, origin, permit))
}

async fn close_socket(socket: &mut WebSocket, reason: &'static str) {
    let frame = CloseFrame {
        code: close_code::POLICY,
        reason: reason.into(),
    };
    let _ = tokio::time::timeout(CLOSE_WAIT, socket.send(WsMessage::Close(Some(frame)))).await;
}

async fn run_socket(
    state: Arc<AppState>,
    mut socket: WebSocket,
    user: User,
    origin: Option<String>,
    permit: OwnedSemaphorePermit,
) {
    let wait = state.config.attach_timeout;
    let hello = match tokio::time::timeout(wait, read_hello(&state, &mut socket)).await {
        Ok(Some(hello)) => hello,
        Ok(None) => return,
        Err(_) => {
            close_socket(&mut socket, "no hello in time").await;
            return;
        }
    };
    let asked = wants_resume(&hello).then(|| hello.resume.clone()).flatten();
    let reclaimed = match &asked {
        Some(request) => {
            state
                .reclaim(request, &user, origin.as_deref(), false)
                .await
        }
        None => None,
    };
    let (session, outgoing, prelude, kicked, permit) = match reclaimed {
        Some(mut reclaimed) => {
            drop(permit);
            let welcome = reclaimed.session.welcome_with(Some(true));
            let mut prelude = vec![welcome];
            prelude.extend(
                reclaimed
                    .session
                    .resume_messages(drain(&mut reclaimed.outgoing)),
            );
            (
                reclaimed.session,
                reclaimed.outgoing,
                prelude,
                Some(reclaimed.kicked),
                None,
            )
        }
        None => {
            let (session, outgoing) = state.agent.open_session(hello, user);
            let welcome = session.welcome_with(asked.as_ref().map(|_| false));
            if session.resumable() {
                let (kick, kicked) = oneshot::channel();
                let mut slot = Slot::new(session.clone(), None, origin, false);
                slot.kick = Some(kick);
                state.keep(session.clone(), slot, permit);
                (session, outgoing, vec![welcome], Some(kicked), None)
            } else {
                (session, outgoing, vec![welcome], None, Some(permit))
            }
        }
    };
    let mut delivered = true;
    for message in prelude {
        if socket
            .send(WsMessage::Text(message.to_string().into()))
            .await
            .is_err()
        {
            delivered = false;
            break;
        }
    }
    if !delivered {
        if session.resumable() {
            state.detach(&session, outgoing);
        } else {
            session.close(CloseReason::ClientGone);
        }
        return;
    }
    let (sink, mut stream) = socket.split();
    let (stop, stopped) = oneshot::channel();
    let writer = tokio::spawn(write_socket(session.clone(), sink, outgoing, stopped));
    let mut kicked = kicked;
    loop {
        let next = tokio::select! {
            biased;
            _ = session.closed() => break,
            _ = async { match kicked.as_mut() { Some(kicked) => { let _ = kicked.await; } None => std::future::pending().await } } => break,
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
    if session.resumable() && session.close_reason().is_none() {
        let _ = stop.send(());
        if let Ok(Some(outgoing)) = writer.await {
            state.detach(&session, outgoing);
        } else {
            session.close(CloseReason::ClientGone);
        }
        return;
    }
    session.close(CloseReason::ClientGone);
    let _ = writer.await;
    drop(stop);
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
    mut stopped: oneshot::Receiver<()>,
) -> Option<Outgoing> {
    loop {
        let next = tokio::select! {
            biased;
            reason = session.closed() => Err(Some(reason)),
            _ = &mut stopped => Err(None),
            message = outgoing.recv() => message.ok_or(Some(CloseReason::ClientGone)),
        };
        let reason = match next {
            Ok(message) => {
                let sent = tokio::select! {
                    biased;
                    reason = session.closed() => Err(Some(reason)),
                    sent = sink.send(WsMessage::Text(message.to_string().into())) => {
                        sent.map_err(|_| None)
                    }
                };
                match sent {
                    Ok(()) => continue,
                    Err(reason) => reason,
                }
            }
            Err(reason) => reason,
        };
        let Some(reason) = reason else {
            if session.resumable() && session.close_reason().is_none() {
                return Some(outgoing);
            }
            session.close(CloseReason::ClientGone);
            return None;
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
        return None;
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
        return start_session(&state, user, origin_of(&headers), &body).await;
    };
    let session = state
        .sessions()
        .get(&id)
        .filter(|slot| slot.events && slot.session.user().id == user.id && !slot.detached)
        .map(|slot| slot.session.clone());
    let Some(session) = session else {
        return (StatusCode::NOT_FOUND, "unknown session").into_response();
    };
    session.receive(&body).await;
    StatusCode::ACCEPTED.into_response()
}

async fn start_session(
    state: &Arc<AppState>,
    user: User,
    origin: Option<String>,
    body: &str,
) -> Response {
    let limits = state.agent.config().limits;
    let hello = match protocol::parse_hello(body, &limits) {
        Ok(hello) => hello,
        Err(error) => {
            return json_response(StatusCode::BAD_REQUEST, &bad_message(error.to_string()));
        }
    };
    let asked = wants_resume(&hello).then(|| hello.resume.clone()).flatten();
    if let Some(request) = &asked
        && let Some(reclaimed) = state.reclaim(request, &user, origin.as_deref(), true).await
    {
        let welcome = reclaimed.session.welcome_with(Some(true));
        let epoch = {
            let mut sessions = state.sessions();
            let Some(slot) = sessions.get_mut(reclaimed.session.id()) else {
                return (StatusCode::NOT_FOUND, "unknown session").into_response();
            };
            slot.outgoing = Some(reclaimed.outgoing);
            slot.kick = None;
            slot.detached = true;
            slot.prelude = true;
            slot.epoch
        };
        state.expire_later(reclaimed.session.clone(), epoch);
        return json_response(StatusCode::OK, &welcome);
    }
    let Some(permit) = state.admit() else {
        return too_many_sessions();
    };
    let (session, outgoing) = state.agent.open_session(hello, user);
    let id = session.id().to_string();
    let welcome = session.welcome_with(asked.as_ref().map(|_| false));
    state.keep(
        session.clone(),
        Slot::new(session.clone(), Some(outgoing), origin, true),
        permit,
    );
    let waiting = Arc::downgrade(state);
    let timeout = state.config.attach_timeout;
    tokio::spawn(async move {
        tokio::time::sleep(timeout).await;
        if let Some(state) = waiting.upgrade() {
            let unattached = state.sessions().get(&id).is_some_and(|slot| {
                Arc::ptr_eq(&slot.session, &session)
                    && slot.outgoing.is_some()
                    && !slot.detached
                    && slot.epoch == 0
            });
            if unattached {
                session.close(CloseReason::ClientGone);
            }
        }
    });
    json_response(StatusCode::OK, &welcome)
}

struct EventFeed {
    session: Arc<Session>,
    outgoing: Option<Outgoing>,
    prelude: std::collections::VecDeque<Value>,
    kicked: oneshot::Receiver<()>,
    state: Arc<AppState>,
    finished: bool,
}

impl Drop for EventFeed {
    fn drop(&mut self) {
        match self.outgoing.take() {
            Some(outgoing) if self.session.resumable() => {
                self.state.detach(&self.session, outgoing);
            }
            _ => self.session.close(CloseReason::ClientGone),
        }
    }
}

async fn next_event(mut feed: EventFeed) -> Option<(Result<Event, Infallible>, EventFeed)> {
    if feed.finished {
        return None;
    }
    if let Some(message) = feed.prelude.pop_front() {
        return Some((Ok(Event::default().data(message.to_string())), feed));
    }
    let outgoing = feed.outgoing.as_mut()?;
    let event = tokio::select! {
        biased;
        reason = feed.session.closed() => {
            feed.finished = true;
            Event::default().event("close").data(reason.detail())
        }
        _ = &mut feed.kicked => return None,
        message = outgoing.recv() => Event::default().data(message?.to_string()),
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
    let (session, mut outgoing, prelude, kicked) = {
        let mut sessions = state.sessions();
        let Some(slot) = sessions
            .get_mut(&id)
            .filter(|slot| slot.events && slot.session.user().id == user.id)
        else {
            return (StatusCode::NOT_FOUND, "unknown session").into_response();
        };
        if slot.detached && !slot.prelude {
            return (StatusCode::NOT_FOUND, "unknown session").into_response();
        }
        let Some(outgoing) = slot.outgoing.take() else {
            return (StatusCode::CONFLICT, "events are already streaming").into_response();
        };
        let (kick, kicked) = oneshot::channel();
        let prelude = std::mem::take(&mut slot.prelude);
        slot.detached = false;
        slot.kick = Some(kick);
        slot.epoch += 1;
        (slot.session.clone(), outgoing, prelude, kicked)
    };
    let prelude = if prelude {
        session.resume_messages(drain(&mut outgoing)).into()
    } else {
        std::collections::VecDeque::new()
    };
    let feed = EventFeed {
        session,
        outgoing: Some(outgoing),
        prelude,
        kicked,
        state: state.clone(),
        finished: false,
    };
    Sse::new(stream::unfold(feed, next_event))
        .keep_alive(KeepAlive::default())
        .into_response()
}
