use crate::apps::{self, Apps};
use crate::charts;
use crate::host::{ActionCall, ActionOutcome, AppError, AppMethod, AppRequest, Host, User};
use crate::ids;
use crate::provider::{ModelEvent, ModelProvider, ModelRequest, StopReason, Usage};
use futures_util::StreamExt;
use polyui_chart::ChartOptions;
use polyui_core::catalog::EMBED_ID;
use polyui_core::embed::{self, MAX_APPS};
use polyui_core::protocol::{
    self, Action, ActionResult, ActionStatus, Close, Component, Confirm, DataOp, DataUpdate,
    ErrorReason, Hello, Incoming, Message, Modality, Open, Patch, Snapshot, Welcome,
};
use polyui_core::stream::{ShowUiStream, StreamEvent};
use polyui_core::tools::{self, ToolOptions, ToolProfile};
use polyui_core::validate::{
    self, Location, Operation, Problem, Problems, SealedAction, Surface, Update, Validator,
};
use polyui_core::{Catalog, CatalogSet, Limits, Locale, PropKind, RefResolver, pointer, summary};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};

pub const SAY: &str = "user.say";
pub const DELTA: &str = "agent.delta";
pub const DONE: &str = "agent.done";
pub const ERROR: &str = "agent.error";
pub const UI_TOOLS: [&str; 3] = ["show_ui", "update_ui", "close_ui"];
pub const MAX_SAY_CHARS: usize = 4000;
pub const STREAM_FEATURE: &str = "stream";
pub const RESUME_FEATURE: &str = "resume";
pub const APP_CALLS_FEATURE: &str = "app_calls";
pub const APP_CALL: &str = "app.call";
pub const APP_RESULT: &str = "app.result";
pub const MAX_APP_CALL_ID_CHARS: usize = 64;
pub const APP_NOT_CONFIRMED: &str = "The person did not confirm this request.";
pub const APP_QUESTION_OPEN: &str =
    "Another request is waiting for confirmation. Please wait for it to finish.";
const COVERED_BY_SNAPSHOT: [&str; 5] = ["ui.open", "ui.patch", "ui.data", "ui.sync", "ui.confirm"];
pub const BUSY: &str =
    "The assistant is still working on your earlier messages. Please wait for it to finish.";
pub const TOO_FAST: &str =
    "You are sending messages too quickly. Please wait a moment and try again.";
pub const SAY_TOO_LONG: &str =
    "Your message is too long. Please keep it under 4000 characters and send it again.";
pub const SAY_EMPTY: &str = "Your message is empty. Please type or say something.";
pub const FORM_REFUSED: &str =
    "Some of the values in this form are not allowed here. Please check them and try again.";

const ROOM: usize = 16;

const GUIDE: &str = "How to answer with UI:
- Call show_ui to put UI on the user's screen. Components are a flat list. A parent lists the ids of its children in children. Every id is unique and every component must be reachable from root.
- The tool result tells you what is on screen now, or lists numbered problems. When it lists problems, fix exactly those and call the tool again.
- Use update_ui to change a surface that is open instead of opening a new one. Use close_ui when a surface is no longer useful.
- Never write secret values such as account or card numbers into props. Write {\"$ref\": \"source:key\"} instead. The server fills in the real value for the user only, and you will only ever see [ref].
- A Button fires a host action through on_press: {\"action\": \"name\", \"args\": {...}}. Actions that move money ask the user to confirm on their own, so do not build your own confirm step for them.
- When the user uses the UI you get a message that starts with [UI event]. Answer it briefly and update the UI when the result changes what is shown.
- Keep your text short. The UI carries the details.";

pub type Outgoing = mpsc::Receiver<Value>;

struct Layered<'a> {
    first: Option<&'a dyn RefResolver>,
    then: &'a dyn RefResolver,
}

impl RefResolver for Layered<'_> {
    fn source_kind(&self, source: &str) -> Option<PropKind> {
        self.first
            .and_then(|first| first.source_kind(source))
            .or_else(|| self.then.source_kind(source))
    }

    fn resolve(&self, source: &str, key: &str) -> Result<Value, String> {
        match self.first {
            Some(first) if first.has_source(source) => first.resolve(source, key),
            _ => self.then.resolve(source, key),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnRate {
    pub burst: u32,
    pub refill: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentConfig {
    pub max_tokens: u32,
    pub max_retries: u32,
    pub max_model_calls: u32,
    pub confirm_ttl: Duration,
    pub limits: Limits,
    pub max_open_surfaces: usize,
    pub outgoing_queue: usize,
    pub slow_consumer_after: Duration,
    pub waiting_turns: usize,
    pub turn_rate: TurnRate,
    pub tool_profile: ToolProfile,
    pub open_app_calls: usize,
    pub app_call_rate: TurnRate,
    pub app_call_timeout: Duration,
    pub max_app_params_bytes: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tokens: 16000,
            max_retries: 2,
            max_model_calls: 8,
            confirm_ttl: Duration::from_secs(60),
            limits: Limits::default(),
            max_open_surfaces: 16,
            outgoing_queue: 256,
            slow_consumer_after: Duration::from_secs(10),
            waiting_turns: 2,
            turn_rate: TurnRate {
                burst: 8,
                refill: Duration::from_secs(4),
            },
            tool_profile: ToolProfile::Compact,
            open_app_calls: 4,
            app_call_rate: TurnRate {
                burst: 30,
                refill: Duration::from_secs(2),
            },
            app_call_timeout: Duration::from_secs(30),
            max_app_params_bytes: 64 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    SlowConsumer,
    ClientGone,
}

impl CloseReason {
    pub fn detail(self) -> &'static str {
        match self {
            Self::SlowConsumer => "the client did not read server messages in time",
            Self::ClientGone => "the client went away",
        }
    }
}

#[derive(Clone)]
pub struct Agent {
    host: Arc<dyn Host>,
    provider: Arc<dyn ModelProvider>,
    config: AgentConfig,
    tool_types: Arc<[String]>,
}

impl Agent {
    pub fn new(host: Arc<dyn Host>, provider: Arc<dyn ModelProvider>) -> Self {
        Self {
            host,
            provider,
            config: AgentConfig::default(),
            tool_types: Arc::from([]),
        }
    }

    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_tool_types(mut self, types: Vec<String>) -> Self {
        self.tool_types = Arc::from(types);
        self
    }

    pub fn tool_types(&self) -> &[String] {
        &self.tool_types
    }

    pub fn host(&self) -> &Arc<dyn Host> {
        &self.host
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    pub fn open_session(&self, hello: Hello, user: User) -> (Arc<Session>, Outgoing) {
        Session::start(self.clone(), hello, user)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SurfaceState {
    model: Surface,
    client: Surface,
    seq: u64,
    pending: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct Handle {
    surface: String,
    sealed: SealedAction,
}

#[derive(Debug, Clone, PartialEq)]
struct PendingConfirm {
    nonce: String,
    surface: String,
    action_id: String,
    expires: Instant,
    values: Map<String, Value>,
    detail: Option<Map<String, Value>>,
    prompt: Confirm,
}

#[derive(Debug, Default)]
struct UiState {
    surfaces: BTreeMap<String, SurfaceState>,
    handles: HashMap<String, Handle>,
    confirms: Vec<PendingConfirm>,
    counter: u64,
}

impl UiState {
    fn next_surface_id(&mut self) -> String {
        loop {
            self.counter += 1;
            let id = format!("s{}", self.counter);
            if !self.surfaces.contains_key(&id) {
                return id;
            }
        }
    }

    fn forget_actions(&mut self, surface: &str, only: Option<&[&str]>) {
        let gone: Vec<String> = self
            .handles
            .iter()
            .filter(|(_, handle)| {
                handle.surface == surface
                    && only.is_none_or(|components| {
                        components.contains(&handle.sealed.component.as_str())
                    })
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &gone {
            self.handles.remove(id);
        }
        self.confirms
            .retain(|pending| !(pending.surface == surface && gone.contains(&pending.action_id)));
    }

    fn settled(&self, id: &str) -> Option<&SurfaceState> {
        self.surfaces.get(id).filter(|state| !state.pending)
    }

    fn open_list(&self) -> String {
        let open: Vec<&str> = self
            .surfaces
            .iter()
            .filter(|(_, state)| !state.pending)
            .map(|(id, _)| id.as_str())
            .collect();
        if open.is_empty() {
            "none".into()
        } else {
            open.join(", ")
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutcome {
    fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

enum TurnInput {
    Say(String),
    Report(String),
}

enum ActionStep {
    Reply(ActionStatus, String),
    Ask(Value),
    Run {
        sealed: Box<SealedAction>,
        values: Map<String, Value>,
        detail: Option<Map<String, Value>>,
    },
}

#[derive(Debug)]
struct Bucket {
    tokens: u32,
    updated: Instant,
}

impl Bucket {
    fn full(rate: TurnRate) -> Self {
        Self {
            tokens: rate.burst,
            updated: Instant::now(),
        }
    }

    fn take(&mut self, rate: TurnRate, now: Instant) -> bool {
        if rate.refill.is_zero() {
            return true;
        }
        if self.tokens >= rate.burst {
            self.updated = now;
        } else {
            let earned =
                now.saturating_duration_since(self.updated).as_nanos() / rate.refill.as_nanos();
            let earned = u32::try_from(earned)
                .unwrap_or(u32::MAX)
                .min(rate.burst - self.tokens);
            self.tokens += earned;
            self.updated += rate.refill * earned;
        }
        if self.tokens == 0 {
            return false;
        }
        self.tokens -= 1;
        true
    }
}

#[derive(Debug)]
struct Draft {
    stream: ShowUiStream,
    apps: Apps,
    dead: bool,
    surface: Option<String>,
    named: Option<String>,
    root: Option<String>,
    model: Vec<Component>,
    prepared: Vec<Component>,
    sealed: Vec<SealedAction>,
    sent: HashSet<String>,
}

impl Draft {
    fn new(limits: Limits) -> Self {
        Self {
            stream: ShowUiStream::new(limits),
            apps: Apps::new(),
            dead: false,
            surface: None,
            named: None,
            root: None,
            model: Vec::new(),
            prepared: Vec::new(),
            sealed: Vec::new(),
            sent: HashSet::new(),
        }
    }
}

pub struct Session {
    id: String,
    user: User,
    agent: Agent,
    catalogs: CatalogSet,
    client_catalogs: CatalogSet,
    negotiated: Vec<String>,
    locale: Locale,
    streams: bool,
    embeds: bool,
    chart_options: ChartOptions,
    system: String,
    tools: Vec<Value>,
    ui: Mutex<UiState>,
    usage: Mutex<Usage>,
    bucket: Mutex<Bucket>,
    #[cfg(feature = "query")]
    saved: Mutex<crate::dashboards::SavedQueries>,
    out: mpsc::Sender<Value>,
    turns: mpsc::Sender<TurnInput>,
    closing: watch::Sender<Option<CloseReason>>,
    resumable: bool,
    resume_token: Mutex<Option<String>>,
    app_calls: bool,
    open_app_calls: Arc<AtomicUsize>,
    app_buckets: Mutex<HashMap<(String, String), Bucket>>,
    app_question: Mutex<Option<AppQuestion>>,
    me: Weak<Session>,
}

struct AppQuestion {
    prompt: Confirm,
    expires: Instant,
    call: AppCall,
    app: String,
    method: AppMethod,
    params: Map<String, Value>,
    slot: OpenAppCall,
}

struct OpenAppCall(Arc<AtomicUsize>);

impl Drop for OpenAppCall {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct AppCall {
    surface: String,
    embed: String,
    id: String,
    method: Option<String>,
    params: Option<Value>,
}

impl AppCall {
    fn parse(text: &str) -> Option<Self> {
        let Value::Object(mut fields) = serde_json::from_str::<Value>(text).ok()? else {
            return None;
        };
        let mut text_field = |key: &str| match fields.remove(key) {
            Some(Value::String(value)) if !value.is_empty() => Some(value),
            _ => None,
        };
        let surface = text_field("surface")?;
        let embed = text_field("embed")?;
        let id = text_field("id").filter(|id| id.chars().count() <= MAX_APP_CALL_ID_CHARS)?;
        let method = text_field("method");
        Some(Self {
            surface,
            embed,
            id,
            method,
            params: fields.remove("params"),
        })
    }

    fn reply(&self, outcome: Result<Value, AppError>) -> Value {
        let mut message = json!({
            "v": protocol::VERSION,
            "t": APP_RESULT,
            "surface": self.surface,
            "embed": self.embed,
            "id": self.id,
        });
        if let Some(fields) = message.as_object_mut() {
            match outcome {
                Ok(result) => fields.insert("result".into(), result),
                Err(error) => fields.insert(
                    "error".into(),
                    json!({ "code": error.code, "message": error.message }),
                ),
            };
        }
        message
    }
}

fn app_params(
    method: AppMethod,
    params: Option<Value>,
    max_bytes: usize,
) -> Result<Map<String, Value>, AppError> {
    let params = match params {
        None => Map::new(),
        Some(Value::Object(params)) => params,
        Some(_) => return Err(AppError::invalid_params("params must be an object")),
    };
    if serde_json::to_vec(&params).map_or(usize::MAX, |bytes| bytes.len()) > max_bytes {
        return Err(AppError::invalid_params("params are too large"));
    }
    let named = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
    };
    match method {
        AppMethod::CallTool if !named("name") => {
            Err(AppError::invalid_params("name must be a non-empty string"))
        }
        AppMethod::CallTool
            if params
                .get("arguments")
                .is_some_and(|arguments| !arguments.is_object()) =>
        {
            Err(AppError::invalid_params("arguments must be an object"))
        }
        AppMethod::ReadResource if !named("uri") => {
            Err(AppError::invalid_params("uri must be a non-empty string"))
        }
        _ => Ok(params),
    }
}

fn same_secret(expected: &str, given: &str) -> bool {
    expected.len() == given.len()
        && expected
            .bytes()
            .zip(given.bytes())
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            == 0
}

fn chat(kind: &str) -> Value {
    json!({ "v": protocol::VERSION, "t": kind })
}

fn chat_text(kind: &str, key: &str, text: &str) -> Value {
    json!({ "v": protocol::VERSION, "t": kind, key: text })
}

fn tool_result(id: &str, content: &str, is_error: bool) -> Value {
    let mut block = json!({ "type": "tool_result", "tool_use_id": id, "content": content });
    if is_error {
        block["is_error"] = Value::Bool(true);
    }
    block
}

fn push_user(history: &mut Vec<Value>, blocks: Vec<Value>) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = history.last_mut()
        && last["role"] == "user"
    {
        let existing = match last["content"].take() {
            Value::Array(items) => items,
            Value::String(text) => vec![json!({ "type": "text", "text": text })],
            _ => Vec::new(),
        };
        let mut merged = existing;
        merged.extend(blocks);
        last["content"] = Value::Array(merged);
        return;
    }
    history.push(json!({ "role": "user", "content": blocks }));
}

fn set_value(surface: &mut Surface, path: &str, value: Value) -> Result<(), pointer::PointerError> {
    let mut data = surface.data_value();
    pointer::set(&mut data, path, value)?;
    if let Value::Object(map) = data {
        surface.data = map;
    }
    Ok(())
}

fn binds(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.contains_key("$bind") || map.values().any(binds),
        Value::Array(items) => items.iter().any(binds),
        _ => false,
    }
}

fn props_bind(props: &Map<String, Value>) -> bool {
    props.values().any(binds)
}

fn reachable(root: &str, components: &[Component]) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut pending = vec![root.to_string()];
    while let Some(id) = pending.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(component) = components.iter().find(|component| component.id == id) {
            pending.extend(component.children.iter().cloned());
        }
    }
    seen
}

fn reindex(problems: &mut Problems, positions: &[usize]) {
    for problem in &mut problems.items {
        if let Location::Item { index, .. } = &mut problem.location
            && let Some(position) = positions.get(*index)
        {
            *index = *position;
        }
    }
}

fn data_changes(before: &Map<String, Value>, after: &Map<String, Value>) -> Vec<DataOp> {
    let removed = before
        .keys()
        .filter(|key| !after.contains_key(*key))
        .map(|key| DataOp::Remove {
            path: format!("/{}", pointer::escape_token(key)),
        });
    let changed = after
        .iter()
        .filter(|(key, value)| before.get(*key) != Some(*value))
        .map(|(key, value)| DataOp::Set {
            path: format!("/{}", pointer::escape_token(key)),
            value: value.clone(),
        });
    removed.chain(changed).collect()
}

fn without_embed(catalogs: CatalogSet) -> CatalogSet {
    let kept: Vec<String> = catalogs
        .catalogs()
        .iter()
        .filter(|catalog| catalog.id != EMBED_ID)
        .map(Catalog::tag)
        .collect();
    catalogs.restrict(&kept)
}

fn system_prompt(
    host_prompt: &str,
    hello: &Hello,
    negotiated: &[String],
    locale: Locale,
) -> String {
    let screen = hello.modalities.contains(&Modality::Screen);
    let voice = hello.modalities.contains(&Modality::Voice);
    let device = match (screen, voice) {
        (true, true) => "a screen and voice",
        (false, true) => "voice only, so every component is read aloud; keep surfaces small",
        _ => "a screen",
    };
    format!(
        "{host_prompt}\n\n{GUIDE}\n\nThis client draws the catalogs {} and works with {device}. The user's locale is {}.",
        if negotiated.is_empty() {
            "none".to_string()
        } else {
            negotiated.join(", ")
        },
        locale.tag()
    )
}

impl Session {
    fn start(agent: Agent, hello: Hello, user: User) -> (Arc<Self>, Outgoing) {
        let offered = agent.host.catalogs();
        let negotiated = offered.negotiate(&hello.catalogs);
        let embeds = negotiated
            .iter()
            .any(|tag| tag.split_once('@').is_some_and(|(id, _)| id == EMBED_ID));
        let catalogs = if embeds {
            offered
        } else {
            without_embed(offered)
        };
        let client_catalogs = catalogs.restrict(&negotiated);
        let tag = hello.locale.clone().unwrap_or_else(|| "en".into());
        let locale = Locale::from_tag(&tag);
        let chart_options = ChartOptions::with_locale(polyui_chart::Locale::parse(&tag));
        let mut tool_list = match tools::for_client_types(
            &catalogs,
            &negotiated,
            &agent.tool_types,
            agent.host.actions(),
            ToolOptions {
                strict: true,
                profile: agent.config.tool_profile,
            },
        ) {
            Value::Array(items) => items,
            _ => Vec::new(),
        };
        tool_list.extend(agent.host.tools());
        #[cfg(feature = "query")]
        let host_prompt = match agent.host.data() {
            Some(_) => {
                tool_list.extend(crate::dashboards::tools());
                format!(
                    "{}\n\n{}",
                    agent.host.system_prompt(&user, locale),
                    crate::dashboards::GUIDE
                )
            }
            None => agent.host.system_prompt(&user, locale),
        };
        #[cfg(not(feature = "query"))]
        let host_prompt = agent.host.system_prompt(&user, locale);
        tools::limit_strict(&mut tool_list);
        let system = system_prompt(&host_prompt, &hello, &negotiated, locale);
        let streams = hello
            .features
            .iter()
            .any(|feature| feature == STREAM_FEATURE);
        let resumable = hello
            .features
            .iter()
            .any(|feature| feature == RESUME_FEATURE);
        let app_calls = embeds
            && agent.host.app_calls()
            && hello
                .features
                .iter()
                .any(|feature| feature == APP_CALLS_FEATURE);
        let config = agent.config;
        let (out, outgoing) = mpsc::channel(config.outgoing_queue.max(1));
        let (turns, mut incoming) = mpsc::channel(config.waiting_turns.max(1));
        let (closing, _) = watch::channel(None);
        let session = Arc::new_cyclic(|me| Self {
            id: ids::session_id(),
            user,
            agent,
            catalogs,
            client_catalogs,
            negotiated,
            locale,
            streams,
            embeds,
            chart_options,
            system,
            tools: tool_list,
            ui: Mutex::new(UiState::default()),
            usage: Mutex::new(Usage::default()),
            bucket: Mutex::new(Bucket::full(config.turn_rate)),
            #[cfg(feature = "query")]
            saved: Mutex::new(crate::dashboards::SavedQueries::default()),
            out,
            turns,
            closing,
            resumable,
            resume_token: Mutex::new(None),
            app_calls,
            open_app_calls: Arc::new(AtomicUsize::new(0)),
            app_buckets: Mutex::new(HashMap::new()),
            app_question: Mutex::new(None),
            me: me.clone(),
        });
        let weak: Weak<Self> = Arc::downgrade(&session);
        tokio::spawn(async move {
            let mut history = Vec::new();
            while let Some(input) = incoming.recv().await {
                let Some(session) = weak.upgrade() else {
                    break;
                };
                let finished = tokio::select! {
                    biased;
                    _ = session.closed() => false,
                    () = session.run_turn(&mut history, input) => true,
                };
                if !finished {
                    break;
                }
            }
        });
        (session, outgoing)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn user(&self) -> &User {
        &self.user
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn system_prompt(&self) -> &str {
        &self.system
    }

    pub fn tools(&self) -> &[Value] {
        &self.tools
    }

    pub fn usage(&self) -> Usage {
        self.usage.lock().map(|usage| *usage).unwrap_or_default()
    }

    pub fn welcome(&self) -> Value {
        self.welcome_with(None)
    }

    pub fn welcome_with(&self, resumed: Option<bool>) -> Value {
        let mut welcome = Message::Welcome(Welcome {
            session: self.id.clone(),
            catalogs: self.negotiated.clone(),
        })
        .to_value();
        if let Some(fields) = welcome.as_object_mut() {
            if self.resumable {
                let token = ids::random_id("rt_", 32);
                *self
                    .resume_token
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(token.clone());
                fields.insert("resume_token".into(), Value::String(token));
            }
            if let Some(resumed) = resumed {
                fields.insert("resumed".into(), Value::Bool(resumed));
            }
            let accepted: Vec<Value> = [
                (self.streams, STREAM_FEATURE),
                (self.resumable, RESUME_FEATURE),
                (self.app_calls, APP_CALLS_FEATURE),
            ]
            .into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, feature)| Value::String(feature.into()))
            .collect();
            if !accepted.is_empty() {
                fields.insert("features".into(), Value::Array(accepted));
            }
        }
        welcome
    }

    pub fn resumable(&self) -> bool {
        self.resumable
    }

    pub fn redeem(&self, token: &str) -> bool {
        let mut current = self
            .resume_token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let matches = current
            .as_deref()
            .is_some_and(|expected| same_secret(expected, token));
        if matches {
            *current = None;
        }
        matches
    }

    pub fn resume_messages(&self, queued: Vec<Value>) -> Vec<Value> {
        let now = Instant::now();
        let ui = self.lock();
        let mut messages = Vec::new();
        let mut covered = HashSet::new();
        for (id, state) in ui.surfaces.iter().filter(|(_, state)| !state.pending) {
            let view = self.client_view(&state.client);
            covered.insert(id.clone());
            messages.push(
                Message::Snapshot(Snapshot {
                    surface: id.clone(),
                    seq: state.seq,
                    root: view.root,
                    components: view.components,
                    data: view.data,
                })
                .to_value(),
            );
        }
        for pending in ui.confirms.iter().filter(|pending| pending.expires > now) {
            let mut prompt = pending.prompt.clone();
            prompt.expires_ms = u64::try_from((pending.expires - now).as_millis()).ok();
            messages.push(Message::Confirm(prompt).to_value());
        }
        drop(ui);
        if let Some(question) = self
            .app_question()
            .as_ref()
            .filter(|question| question.expires > now)
        {
            let mut prompt = question.prompt.clone();
            prompt.expires_ms = u64::try_from((question.expires - now).as_millis()).ok();
            messages.push(Message::Confirm(prompt).to_value());
        }
        messages.extend(queued.into_iter().filter(|message| {
            let kind = message.get("t").and_then(Value::as_str).unwrap_or_default();
            let surface = message.get("surface").and_then(Value::as_str);
            !(COVERED_BY_SNAPSHOT.contains(&kind)
                && surface.is_some_and(|surface| covered.contains(surface)))
        }));
        messages
    }

    pub fn open_surfaces(&self) -> Vec<String> {
        self.lock().surfaces.keys().cloned().collect()
    }

    pub fn model_summary(&self, surface: &str) -> Option<String> {
        self.lock()
            .settled(surface)
            .map(|state| state.model.summary(surface, &self.catalogs, self.locale))
    }

    pub fn close(&self, reason: CloseReason) {
        self.closing.send_if_modified(|current| {
            if current.is_some() {
                return false;
            }
            *current = Some(reason);
            true
        });
    }

    pub fn close_reason(&self) -> Option<CloseReason> {
        *self.closing.borrow()
    }

    pub async fn closed(&self) -> CloseReason {
        let mut watcher = self.closing.subscribe();
        let marked = async {
            loop {
                let current = *watcher.borrow_and_update();
                if let Some(reason) = current {
                    return reason;
                }
                if watcher.changed().await.is_err() {
                    return CloseReason::ClientGone;
                }
            }
        };
        tokio::select! {
            biased;
            reason = marked => reason,
            () = self.out.closed() => CloseReason::ClientGone,
        }
    }

    fn lock(&self) -> MutexGuard<'_, UiState> {
        self.ui
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn send(&self, message: Value) {
        if let Err(TrySendError::Full(_)) = self.out.try_send(message) {
            self.close(CloseReason::SlowConsumer);
        }
    }

    async fn wait_for_room(&self) -> bool {
        let room = ROOM.min(self.out.max_capacity());
        let patience = self.agent.config.slow_consumer_after;
        tokio::select! {
            biased;
            _ = self.closed() => false,
            reserved = tokio::time::timeout(patience, self.out.reserve_many(room)) => match reserved {
                Ok(Ok(permits)) => {
                    drop(permits);
                    true
                }
                Ok(Err(_)) => false,
                Err(_) => {
                    self.close(CloseReason::SlowConsumer);
                    false
                }
            },
        }
    }

    fn take_turn(&self) -> bool {
        let rate = self.agent.config.turn_rate;
        self.bucket
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take(rate, Instant::now())
    }

    pub fn send_bad_message(&self, detail: &str) {
        self.send_error(None, ErrorReason::BadMessage, detail.to_string());
    }

    fn send_error(&self, surface: Option<&str>, reason: ErrorReason, detail: String) {
        self.send(protocol::error_value(surface, reason, Some(detail)));
    }

    fn client_view(&self, surface: &Surface) -> Surface {
        validate::downgrade_for_client(surface, &self.catalogs, &self.client_catalogs, self.locale)
    }

    pub async fn receive(&self, text: &str) {
        if !self.wait_for_room().await {
            return;
        }
        let limits = self.agent.config.limits;
        match protocol::parse_with(text, &limits) {
            Err(error) => self.send_error(None, ErrorReason::BadMessage, error.to_string()),
            Ok(Incoming::UnsupportedVersion {
                version, surface, ..
            }) => self.send_error(
                surface.as_deref(),
                ErrorReason::UnsupportedVersion,
                format!("version {version} is not supported"),
            ),
            Ok(Incoming::UnknownType { message_type, .. }) if message_type == SAY => {
                let said = serde_json::from_str::<Value>(text).ok().and_then(|value| {
                    value
                        .get("text")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
                match said {
                    Some(said) if said.chars().count() > MAX_SAY_CHARS => {
                        self.send(chat_text(ERROR, "message", SAY_TOO_LONG));
                    }
                    Some(said) if !said.trim().is_empty() => self.queue_say(said),
                    _ => self.send(chat_text(ERROR, "message", SAY_EMPTY)),
                }
            }
            Ok(Incoming::UnknownType { message_type, .. }) if message_type == APP_CALL => {
                self.on_app_call(text);
            }
            Ok(Incoming::UnknownType {
                message_type,
                surface,
            }) => self.send_error(
                surface.as_deref(),
                ErrorReason::UnknownMessage,
                format!("unknown message type \"{message_type}\""),
            ),
            Ok(Incoming::Message(message)) => match message {
                Message::Action(action) => self.on_action(action).await,
                Message::Resync(resync) => self.on_resync(&resync.surface),
                Message::Error(report) => {
                    eprintln!(
                        "polyui: session {} client reported {:?} on {:?}: {}",
                        self.id,
                        report.reason,
                        report.surface,
                        report.detail.unwrap_or_default()
                    );
                }
                Message::Hello(_) => self.send_error(
                    None,
                    ErrorReason::BadMessage,
                    "hello was already received".into(),
                ),
                other => self.send_error(
                    other.surface(),
                    ErrorReason::BadMessage,
                    format!("{} is not a client message", other.message_type()),
                ),
            },
        }
    }

    pub fn app_calls(&self) -> bool {
        self.app_calls
    }

    fn on_app_call(&self, text: &str) {
        let Some(call) = AppCall::parse(text) else {
            self.send_error(
                None,
                ErrorReason::BadMessage,
                format!(
                    "{APP_CALL} needs non-empty string fields surface, embed and id, with id at most {MAX_APP_CALL_ID_CHARS} characters"
                ),
            );
            return;
        };
        let (app, method, params, slot) = match self.plan_app_call(&call) {
            Ok(planned) => planned,
            Err(error) => {
                self.send(call.reply(Err(error)));
                return;
            }
        };
        let request = AppRequest {
            app: &app,
            method,
            params: &params,
        };
        match self
            .agent
            .host
            .app_confirm(&self.user, request, self.locale)
        {
            None => self.run_app_call(call, app, method, params, slot),
            Some(text) => self.ask_about_app_call(AppQuestion {
                prompt: Confirm {
                    surface: call.surface.clone(),
                    action_id: ids::random_id("app_", 8),
                    nonce: ids::nonce(),
                    title: text.title,
                    body: text.body,
                    confirm_label: text.confirm_label,
                    expires_ms: u64::try_from(self.agent.config.app_call_timeout.as_millis()).ok(),
                    seq: None,
                },
                expires: Instant::now() + self.agent.config.app_call_timeout,
                call,
                app,
                method,
                params,
                slot,
            }),
        }
    }

    fn app_question(&self) -> MutexGuard<'_, Option<AppQuestion>> {
        self.app_question
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn ask_about_app_call(&self, question: AppQuestion) {
        let mut open = self.app_question();
        if open
            .as_ref()
            .is_some_and(|current| current.expires > Instant::now())
        {
            drop(open);
            self.send(
                question
                    .call
                    .reply(Err(AppError::refused(APP_QUESTION_OPEN))),
            );
            return;
        }
        let prompt = question.prompt.clone();
        let action_id = prompt.action_id.clone();
        let wait = question.expires.saturating_duration_since(Instant::now());
        if let Some(stale) = open.replace(question) {
            self.close_app_question(stale);
        }
        drop(open);
        self.send(Message::Confirm(prompt).to_value());
        let weak = self.me.clone();
        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            if let Some(session) = weak.upgrade() {
                session.expire_app_question(&action_id);
            }
        });
    }

    fn expire_app_question(&self, action_id: &str) {
        let expired = {
            let mut open = self.app_question();
            match open.as_ref() {
                Some(current) if current.prompt.action_id == action_id => open.take(),
                _ => None,
            }
        };
        if let Some(question) = expired {
            self.close_app_question(question);
        }
    }

    fn close_app_question(&self, question: AppQuestion) {
        self.action_result(
            &question.prompt.surface,
            &question.prompt.action_id,
            ActionStatus::Expired,
            Some("This confirmation expired.".into()),
        );
        self.send(
            question
                .call
                .reply(Err(AppError::refused(APP_NOT_CONFIRMED))),
        );
    }

    fn answer_app_question(&self, action: &Action) -> bool {
        let mut open = self.app_question();
        let Some(current) = open.as_ref() else {
            return false;
        };
        if current.prompt.action_id != action.action_id {
            return false;
        }
        let genuine = current.prompt.surface == action.surface
            && action
                .confirm
                .as_deref()
                .is_some_and(|nonce| same_secret(&current.prompt.nonce, nonce));
        if !genuine {
            drop(open);
            self.action_result(
                &action.surface,
                &action.action_id,
                ActionStatus::Rejected,
                Some("This confirmation is not valid. Please try again.".into()),
            );
            return true;
        }
        let Some(question) = open.take() else {
            return true;
        };
        drop(open);
        if question.expires <= Instant::now() {
            self.close_app_question(question);
            return true;
        }
        self.action_result(&action.surface, &action.action_id, ActionStatus::Ok, None);
        self.run_app_call(
            question.call,
            question.app,
            question.method,
            question.params,
            question.slot,
        );
        true
    }

    fn plan_app_call(
        &self,
        call: &AppCall,
    ) -> Result<(String, AppMethod, Map<String, Value>, OpenAppCall), AppError> {
        if !self.app_calls {
            return Err(AppError::not_available());
        }
        let method = call
            .method
            .as_deref()
            .and_then(AppMethod::parse)
            .ok_or_else(AppError::not_available)?;
        let config = self.agent.config;
        let params = app_params(method, call.params.clone(), config.max_app_params_bytes)?;
        let app = {
            let ui = self.lock();
            self.app_buckets
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .retain(|(surface, _), _| ui.surfaces.contains_key(surface));
            let state = ui
                .surfaces
                .get(&call.surface)
                .filter(|state| !state.pending)
                .ok_or_else(|| AppError::refused("This app is no longer on screen."))?;
            state
                .client
                .get(&call.embed)
                .filter(|component| component.type_name == apps::EMBED_TYPE)
                .and_then(|component| component.props.get(apps::APP_PROP))
                .and_then(|app| app.pointer("/resource/uri"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| AppError::refused("This app is no longer on screen."))?
        };
        let allowed = self
            .app_buckets
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry((call.surface.clone(), call.embed.clone()))
            .or_insert_with(|| Bucket::full(config.app_call_rate))
            .take(config.app_call_rate, Instant::now());
        if !allowed {
            return Err(AppError::refused(
                "This app is sending requests too quickly. Please wait a moment.",
            ));
        }
        let open = self.open_app_calls.fetch_add(1, Ordering::AcqRel);
        let slot = OpenAppCall(Arc::clone(&self.open_app_calls));
        if open >= config.open_app_calls {
            return Err(AppError::refused(
                "Too many app requests are still open. Please wait for one to finish.",
            ));
        }
        Ok((app, method, params, slot))
    }

    fn run_app_call(
        &self,
        call: AppCall,
        app: String,
        method: AppMethod,
        params: Map<String, Value>,
        slot: OpenAppCall,
    ) {
        let weak = self.me.clone();
        let host = Arc::clone(&self.agent.host);
        let user = self.user.clone();
        let timeout = self.agent.config.app_call_timeout;
        let max_bytes = self.agent.config.limits.max_message_bytes;
        tokio::spawn(async move {
            let request = AppRequest {
                app: &app,
                method,
                params: &params,
            };
            let outcome = tokio::time::timeout(timeout, host.app_request(&user, request))
                .await
                .unwrap_or_else(|_| Err(AppError::refused("The app request timed out.")));
            drop(slot);
            let Some(session) = weak.upgrade() else {
                return;
            };
            let mut reply = call.reply(outcome);
            if reply.to_string().len() > max_bytes {
                reply = call.reply(Err(AppError::refused(
                    "The answer is too large to send to the app.",
                )));
            }
            session.send(reply);
        });
    }

    fn queue_say(&self, text: String) {
        match self.turns.try_reserve() {
            Ok(permit) if self.take_turn() => permit.send(TurnInput::Say(text)),
            Ok(permit) => {
                drop(permit);
                self.send(chat_text(ERROR, "message", TOO_FAST));
            }
            Err(TrySendError::Full(())) => self.send(chat_text(ERROR, "message", BUSY)),
            Err(TrySendError::Closed(())) => {}
        }
    }

    fn on_resync(&self, surface: &str) {
        let snapshot = self.lock().surfaces.get(surface).map(|state| {
            let view = self.client_view(&state.client);
            Message::Snapshot(Snapshot {
                surface: surface.to_string(),
                seq: state.seq,
                root: view.root,
                components: view.components,
                data: view.data,
            })
            .to_value()
        });
        match snapshot {
            Some(message) => self.send(message),
            None => self.send_error(
                Some(surface),
                ErrorReason::BadMessage,
                "this surface is not open".into(),
            ),
        }
    }

    fn action_result(
        &self,
        surface: &str,
        action_id: &str,
        status: ActionStatus,
        message: Option<String>,
    ) {
        self.send(
            Message::ActionResult(ActionResult {
                surface: surface.to_string(),
                action_id: action_id.to_string(),
                status,
                message,
                seq: None,
            })
            .to_value(),
        );
    }

    fn plan_action(&self, action: &Action) -> ActionStep {
        let now = Instant::now();
        let ttl = self.agent.config.confirm_ttl;
        let mut ui = self.lock();
        ui.confirms
            .retain(|pending| pending.expires + ttl * 4 > now);
        let Some(state) = ui.surfaces.get(&action.surface) else {
            return ActionStep::Reply(
                ActionStatus::Rejected,
                "This screen is no longer open.".into(),
            );
        };
        if state.pending {
            return ActionStep::Reply(
                ActionStatus::Rejected,
                "This screen is still loading. Please try again in a moment.".into(),
            );
        }
        let Some(handle) = ui
            .handles
            .get(&action.action_id)
            .filter(|handle| handle.surface == action.surface)
            .cloned()
        else {
            return ActionStep::Reply(
                ActionStatus::Rejected,
                "This button is no longer available.".into(),
            );
        };
        let values: Map<String, Value> = action
            .values
            .iter()
            .filter(|(path, _)| handle.sealed.with.contains(path))
            .map(|(path, value)| (path.clone(), value.clone()))
            .collect();
        if !values.is_empty() && self.with_values(state, &values).is_none() {
            return ActionStep::Reply(ActionStatus::Rejected, FORM_REFUSED.into());
        }
        if !handle.sealed.requires_confirm {
            return ActionStep::Run {
                sealed: Box::new(handle.sealed),
                values,
                detail: action.detail.clone(),
            };
        }
        match &action.confirm {
            None => {
                ui.confirms.retain(|pending| {
                    !(pending.surface == action.surface && pending.action_id == action.action_id)
                });
                let text = self.agent.host.confirm_text(
                    &self.user,
                    &handle.sealed.action,
                    &handle.sealed.args,
                    self.locale,
                );
                let prompt = Confirm {
                    surface: action.surface.clone(),
                    action_id: action.action_id.clone(),
                    nonce: ids::nonce(),
                    title: text.title,
                    body: text.body,
                    confirm_label: text.confirm_label,
                    expires_ms: u64::try_from(ttl.as_millis()).ok(),
                    seq: None,
                };
                ui.confirms.push(PendingConfirm {
                    nonce: prompt.nonce.clone(),
                    surface: action.surface.clone(),
                    action_id: action.action_id.clone(),
                    expires: now + ttl,
                    values,
                    detail: action.detail.clone(),
                    prompt: prompt.clone(),
                });
                ActionStep::Ask(Message::Confirm(prompt).to_value())
            }
            Some(nonce) => {
                let found = ui.confirms.iter().position(|pending| {
                    pending.nonce == *nonce
                        && pending.action_id == action.action_id
                        && pending.surface == action.surface
                });
                let Some(position) = found else {
                    return ActionStep::Reply(
                        ActionStatus::Rejected,
                        "This confirmation is not valid. Please try again.".into(),
                    );
                };
                let pending = ui.confirms.remove(position);
                if pending.expires <= now {
                    return ActionStep::Reply(
                        ActionStatus::Expired,
                        "This confirmation expired. Please try again.".into(),
                    );
                }
                ActionStep::Run {
                    sealed: Box::new(handle.sealed),
                    values: pending.values,
                    detail: pending.detail,
                }
            }
        }
    }

    fn with_values(
        &self,
        state: &SurfaceState,
        values: &Map<String, Value>,
    ) -> Option<(Surface, Surface)> {
        let mut model = state.model.clone();
        let mut client = state.client.clone();
        for (path, value) in values {
            set_value(&mut model, path, value.clone()).ok()?;
            set_value(&mut client, path, value.clone()).ok()?;
        }
        let data = client.data_value();
        polyui_core::limits::check_data_model(&data, &self.agent.config.limits).ok()?;
        let host_refs = self.agent.host.refs(&self.user);
        #[cfg(feature = "query")]
        let declared = crate::dashboards::QueryRefs::new(&self.catalogs);
        #[cfg(feature = "query")]
        let first: Option<&dyn RefResolver> = Some(&declared);
        #[cfg(not(feature = "query"))]
        let first: Option<&dyn RefResolver> = None;
        let refs = Layered {
            first,
            then: host_refs.as_ref(),
        };
        let validator = self.validator(&refs);
        for (index, component) in model.components.iter().enumerate() {
            if !props_bind(&component.props) {
                continue;
            }
            let raw = serde_json::to_value(component).ok()?;
            validator
                .component("components", index, &raw, Some(&data))
                .ok()?;
        }
        for component in client.components.iter_mut() {
            if component.type_name != charts::CHART_TYPE || !props_bind(&component.props) {
                continue;
            }
            let mut chart = component.clone();
            chart.props.remove(charts::SCENE_PROP);
            chart.say = model
                .get(&component.id)
                .and_then(|written| written.say.clone());
            let mut redrawn = [chart];
            if !charts::attach_scenes(&mut redrawn, &data, "components", &self.chart_options)
                .is_empty()
            {
                return None;
            }
            let [chart] = redrawn;
            *component = chart;
        }
        Some((model, client))
    }

    fn store_values(&self, surface: &str, values: &Map<String, Value>) -> bool {
        if values.is_empty() {
            return true;
        }
        let mut ui = self.lock();
        let Some(state) = ui.settled(surface).cloned() else {
            return false;
        };
        let Some((model, client)) = self.with_values(&state, values) else {
            return false;
        };
        let before = self.client_view(&state.client);
        let after = self.client_view(&client);
        let redrawn: Vec<Component> = after
            .components
            .iter()
            .filter(|component| before.get(&component.id) != Some(*component))
            .cloned()
            .collect();
        let mut seq = state.seq;
        let patch = (!redrawn.is_empty()).then(|| {
            seq += 1;
            Message::Patch(Patch {
                surface: surface.to_string(),
                seq,
                upsert: redrawn,
                remove: Vec::new(),
                root: None,
            })
            .to_value()
        });
        ui.surfaces.insert(
            surface.to_string(),
            SurfaceState {
                model,
                client,
                seq,
                pending: false,
            },
        );
        drop(ui);
        if let Some(patch) = patch {
            self.send(patch);
        }
        true
    }

    async fn on_action(&self, action: Action) {
        if self.answer_app_question(&action) {
            return;
        }
        let (sealed, values, detail) = match self.plan_action(&action) {
            ActionStep::Reply(status, message) => {
                self.action_result(&action.surface, &action.action_id, status, Some(message));
                return;
            }
            ActionStep::Ask(confirm) => {
                self.send(confirm);
                return;
            }
            ActionStep::Run {
                sealed,
                values,
                detail,
            } => (sealed, values, detail),
        };
        let refusal = match self.turns.try_reserve() {
            Ok(permit) if self.take_turn() => Ok(permit),
            Ok(permit) => {
                drop(permit);
                Err(TOO_FAST)
            }
            Err(TrySendError::Full(())) => Err(BUSY),
            Err(TrySendError::Closed(())) => return,
        };
        let permit = match refusal {
            Ok(permit) => permit,
            Err(message) => {
                self.action_result(
                    &action.surface,
                    &action.action_id,
                    ActionStatus::Rejected,
                    Some(message.into()),
                );
                return;
            }
        };
        if !self.store_values(&action.surface, &values) {
            self.action_result(
                &action.surface,
                &action.action_id,
                ActionStatus::Rejected,
                Some(FORM_REFUSED.into()),
            );
            return;
        }
        let outcome: ActionOutcome = self
            .agent
            .host
            .run_action(
                &self.user,
                ActionCall {
                    action: &sealed.action,
                    args: &sealed.args,
                    values: &values,
                    detail: detail.as_ref(),
                    source: action.source,
                    locale: self.locale,
                },
            )
            .await;
        let status = if outcome.ok {
            ActionStatus::Ok
        } else {
            ActionStatus::Rejected
        };
        self.action_result(
            &action.surface,
            &action.action_id,
            status,
            outcome.message.clone(),
        );
        let report = self.report(&action, &sealed, &values, detail.as_ref(), &outcome);
        permit.send(TurnInput::Report(report));
    }

    fn report(
        &self,
        action: &Action,
        sealed: &SealedAction,
        values: &Map<String, Value>,
        detail: Option<&Map<String, Value>>,
        outcome: &ActionOutcome,
    ) -> String {
        let source = match action.source {
            protocol::ActionSource::Tap => "tap",
            protocol::ActionSource::Voice => "voice",
            protocol::ActionSource::Keyboard => "keyboard",
        };
        let mut lines = vec![format!(
            "[UI event] The user used component \"{}\" ({}) on surface {} by {source}. Action: {} {}.",
            sealed.component,
            sealed.prop,
            action.surface,
            sealed.action,
            Value::Object(sealed.args.clone())
        )];
        if !values.is_empty() {
            lines.push(format!("Values: {}", Value::Object(values.clone())));
        }
        if let Some(detail) = detail {
            lines.push(format!("Detail: {}", Value::Object(detail.clone())));
        }
        let mut result = if outcome.ok {
            "Result: ok.".to_string()
        } else {
            "Result: rejected.".to_string()
        };
        if let Some(message) = &outcome.message {
            result.push_str(&format!(" Message shown to the user: {message}"));
        }
        if let Some(note) = &outcome.note {
            result.push_str(&format!(" Note: {note}"));
        }
        lines.push(result);
        let screen = self
            .model_summary(&action.surface)
            .unwrap_or_else(|| summary::closed(&action.surface));
        lines.push(format!("On screen now: {screen}"));
        lines.join("\n")
    }

    async fn run_turn(&self, history: &mut Vec<Value>, input: TurnInput) {
        let text = match input {
            TurnInput::Say(text) | TurnInput::Report(text) => text,
        };
        push_user(history, vec![json!({ "type": "text", "text": text })]);
        let config = self.agent.config;
        let mut failures: HashMap<String, u32> = HashMap::new();
        for _ in 0..config.max_model_calls {
            let request = ModelRequest {
                system: self.system.clone(),
                tools: self.tools.clone(),
                messages: history.clone(),
                max_tokens: config.max_tokens,
            };
            let mut stream = self.agent.provider.stream(request);
            let mut calls = Vec::new();
            let mut drafts: HashMap<String, Draft> = HashMap::new();
            let mut finished = None;
            while let Some(event) = stream.next().await {
                if !self.wait_for_room().await {
                    return;
                }
                match event {
                    ModelEvent::TextDelta(delta) if !delta.is_empty() => {
                        self.send(chat_text(DELTA, "text", &delta));
                    }
                    ModelEvent::ToolCallStarted { id, name }
                        if self.streams && name == "show_ui" =>
                    {
                        drafts.insert(id, Draft::new(config.limits));
                    }
                    ModelEvent::ToolInputDelta { id, partial_json } => {
                        if let Some(draft) = drafts.get_mut(&id) {
                            self.grow(draft, &partial_json).await;
                        }
                    }
                    ModelEvent::ToolCallFinished { id, name, input } => {
                        calls.push((id, name, input));
                    }
                    ModelEvent::MessageFinished {
                        stop_reason,
                        content,
                        usage,
                    } => {
                        finished = Some((stop_reason, content, usage));
                        break;
                    }
                    ModelEvent::Error(error) => {
                        eprintln!("polyui: session {} model error: {error}", self.id);
                        self.close_pending(None);
                        self.send(chat_text(ERROR, "message", error.user_message()));
                        return;
                    }
                    _ => {}
                }
            }
            drop(stream);
            let Some((stop_reason, content, usage)) = finished else {
                self.close_pending(None);
                self.send(chat_text(
                    ERROR,
                    "message",
                    "The assistant stopped before finishing. Please try again.",
                ));
                return;
            };
            if let Ok(mut total) = self.usage.lock() {
                total.add(&usage);
            }
            let tool_ids: Vec<String> = content
                .iter()
                .filter(|block| block["type"] == "tool_use")
                .filter_map(|block| block["id"].as_str().map(str::to_string))
                .collect();
            if !content.is_empty() {
                history.push(json!({ "role": "assistant", "content": content }));
            }
            if stop_reason.is_cut_off() {
                self.close_pending(None);
                push_user(
                    history,
                    tool_ids
                        .iter()
                        .map(|id| tool_result(id, "Not run: the reply was cut off.", true))
                        .collect(),
                );
                let message = if stop_reason == StopReason::Refusal {
                    "The assistant could not help with that request."
                } else {
                    "The answer was too long and was cut off. Please ask for less at once."
                };
                self.send(chat_text(ERROR, "message", message));
                return;
            }
            if calls.is_empty() {
                self.close_pending(None);
                self.send(chat(DONE));
                return;
            }
            let mut results = Vec::new();
            let mut gave_up = false;
            for (id, name, input) in calls {
                if !self.wait_for_room().await {
                    return;
                }
                let draft = drafts.remove(&id);
                let mut outcome = match input {
                    Err(raw) => ToolOutcome::error(json!({ "INVALID_JSON": raw }).to_string()),
                    Ok(input) => self.dispatch(&name, &input, draft.as_ref()).await,
                };
                if let Some(surface) = draft.as_ref().and_then(|draft| draft.surface.as_deref()) {
                    self.close_pending(Some(surface));
                }
                if UI_TOOLS.contains(&name.as_str()) {
                    if outcome.is_error {
                        let count = failures.entry(name.clone()).or_default();
                        *count += 1;
                        if *count > config.max_retries {
                            gave_up = true;
                            outcome.content.push_str(&format!(
                                "\nThis failed {count} times, so the user was told in plain text. Do not call {name} again for this request."
                            ));
                        }
                    } else {
                        failures.remove(&name);
                    }
                }
                results.push(tool_result(&id, &outcome.content, outcome.is_error));
            }
            self.close_pending(None);
            push_user(history, results);
            if gave_up {
                self.send(chat_text(
                    DELTA,
                    "text",
                    &self.agent.host.fallback_text(self.locale),
                ));
                self.send(chat(DONE));
                return;
            }
        }
        self.send(chat(DONE));
    }

    async fn grow(&self, draft: &mut Draft, chunk: &str) {
        if draft.dead {
            return;
        }
        let Some(fresh) = self.feed(draft, chunk) else {
            draft.dead = true;
            return;
        };
        if !fresh.is_empty() {
            if self.embeds {
                let uris: Vec<String> = fresh
                    .iter()
                    .filter_map(embed::literal_resource)
                    .map(str::to_string)
                    .collect();
                apps::read_apps(self.agent.host.as_ref(), &self.user, uris, &mut draft.apps).await;
            }
            if !self.accept(draft, fresh) {
                draft.dead = true;
                return;
            }
        }
        if !self.publish(draft) {
            draft.dead = true;
        }
    }

    fn feed(&self, draft: &mut Draft, chunk: &str) -> Option<Vec<Component>> {
        let host_refs = self.agent.host.refs(&self.user);
        let validator = self.validator(host_refs.as_ref());
        let mut fresh = Vec::new();
        for event in draft.stream.feed(&validator, chunk) {
            match event {
                StreamEvent::Surface(id) => draft.named = Some(id),
                StreamEvent::Root(id) => draft.root = Some(id),
                StreamEvent::Component { component, .. } => fresh.push(component),
                StreamEvent::Rejected { .. } | StreamEvent::Invalid(_) => return None,
                StreamEvent::Data(_) => {}
            }
        }
        Some(fresh)
    }

    fn accept(&self, draft: &mut Draft, fresh: Vec<Component>) -> bool {
        let host_refs = self.agent.host.refs(&self.user);
        let validator = self.validator(host_refs.as_ref());
        let mut prepared = fresh.clone();
        match self.prepare(
            &validator,
            &mut prepared,
            &Value::Object(Map::new()),
            Operation::Show,
            &draft.apps,
        ) {
            Ok(sealed) => {
                draft.model.extend(fresh);
                draft.prepared.extend(prepared);
                draft.sealed.extend(sealed);
                true
            }
            Err(_) => false,
        }
    }

    fn publish(&self, draft: &mut Draft) -> bool {
        let mut ui = self.lock();
        let Some(id) = draft.surface.clone() else {
            let Some(root) = draft.root.clone() else {
                return true;
            };
            if !draft.model.iter().any(|component| component.id == root) {
                return true;
            }
            if ui.surfaces.len() >= self.agent.config.max_open_surfaces {
                return false;
            }
            let id = match &draft.named {
                Some(named) if ui.surfaces.contains_key(named) => return false,
                Some(named) => named.clone(),
                None => ui.next_surface_id(),
            };
            let shown = reachable(&root, &draft.prepared);
            let client = Surface {
                id: Some(id.clone()),
                root: root.clone(),
                components: draft
                    .prepared
                    .iter()
                    .filter(|component| shown.contains(&component.id))
                    .cloned()
                    .collect(),
                data: Map::new(),
            };
            let view = self.client_view(&client);
            let open = Message::Open(Open {
                surface: id.clone(),
                seq: 1,
                root: view.root,
                components: view.components,
                data: view.data,
                fallback: Some(client.fallback(&self.catalogs, self.locale)),
            })
            .to_value();
            if self.fits_client(Operation::Show, &[&open]).is_err() {
                return false;
            }
            draft.sent = client
                .components
                .iter()
                .map(|component| component.id.clone())
                .collect();
            let model = Surface {
                id: Some(id.clone()),
                root,
                components: draft.model.clone(),
                data: Map::new(),
            };
            ui.surfaces.insert(
                id.clone(),
                SurfaceState {
                    model,
                    client,
                    seq: 1,
                    pending: true,
                },
            );
            draft.surface = Some(id);
            drop(ui);
            self.send(open);
            return true;
        };
        let Some(state) = ui.surfaces.get_mut(&id).filter(|state| state.pending) else {
            return false;
        };
        let fresh: Vec<Component> = draft
            .prepared
            .iter()
            .filter(|component| !draft.sent.contains(&component.id))
            .cloned()
            .collect();
        if fresh.is_empty() {
            return true;
        }
        let mut client = state.client.clone();
        client.components.extend(fresh.iter().cloned());
        let view = self.client_view(&client);
        let upsert: Vec<Component> = view
            .components
            .iter()
            .filter(|component| fresh.iter().any(|added| added.id == component.id))
            .cloned()
            .collect();
        let seq = state.seq + 1;
        let patch = Message::Patch(Patch {
            surface: id.clone(),
            seq,
            upsert,
            remove: Vec::new(),
            root: None,
        })
        .to_value();
        let snapshot = Message::Snapshot(Snapshot {
            surface: id.clone(),
            seq,
            root: view.root.clone(),
            components: view.components.clone(),
            data: view.data.clone(),
        })
        .to_value();
        if self
            .fits_client(Operation::Show, &[&patch, &snapshot])
            .is_err()
        {
            return false;
        }
        state.client = client;
        state.model.components = draft.model.clone();
        state.seq = seq;
        draft
            .sent
            .extend(fresh.into_iter().map(|component| component.id));
        drop(ui);
        self.send(patch);
        true
    }

    fn close_pending(&self, only: Option<&str>) {
        let mut ui = self.lock();
        let gone: Vec<(String, u64)> = ui
            .surfaces
            .iter()
            .filter(|(id, state)| state.pending && only.is_none_or(|only| only == id.as_str()))
            .map(|(id, state)| (id.clone(), state.seq))
            .collect();
        for (id, _) in &gone {
            ui.surfaces.remove(id);
        }
        drop(ui);
        for (surface, seq) in gone {
            self.send(
                Message::Close(Close {
                    surface,
                    seq: seq + 1,
                })
                .to_value(),
            );
        }
    }

    pub async fn call_tool(&self, name: &str, input: &Value) -> ToolOutcome {
        self.dispatch(name, input, None).await
    }

    async fn apps_for(
        &self,
        operation: Operation,
        input: &Value,
        draft: Option<&Draft>,
    ) -> Result<Apps, ToolOutcome> {
        let mut apps = draft.map(|draft| draft.apps.clone()).unwrap_or_default();
        if !self.embeds {
            return Ok(apps);
        }
        let list = if operation == Operation::Update {
            "upsert"
        } else {
            "components"
        };
        let uris = embed::resources_in(input, list);
        if uris.len() > MAX_APPS {
            return Err(ToolOutcome::error(
                Problems {
                    operation,
                    items: vec![Problem::new(Location::Input, embed::too_many_apps())],
                }
                .to_string(),
            ));
        }
        apps::read_apps(self.agent.host.as_ref(), &self.user, uris, &mut apps).await;
        Ok(apps)
    }

    async fn dispatch(&self, name: &str, input: &Value, draft: Option<&Draft>) -> ToolOutcome {
        let operation = match name {
            "show_ui" => Some(Operation::Show),
            "update_ui" => Some(Operation::Update),
            _ => None,
        };
        let apps = match operation {
            Some(operation) => match self.apps_for(operation, input, draft).await {
                Ok(apps) => apps,
                Err(outcome) => return outcome,
            },
            None => Apps::new(),
        };
        #[cfg(feature = "query")]
        if let Some(outcome) = self.call_data_tool(name, input, draft, &apps).await {
            return outcome;
        }
        match name {
            "show_ui" => self.show_ui_with(input, None, draft, &apps),
            "update_ui" => self.update_ui_with(input, None, &apps),
            "close_ui" => self.close_ui(input),
            _ => {
                let known = self
                    .tools
                    .iter()
                    .any(|tool| tool["name"].as_str() == Some(name));
                if !known {
                    return ToolOutcome::error(format!("unknown tool \"{name}\""));
                }
                match self.agent.host.call_tool(&self.user, name, input).await {
                    Ok(text) => ToolOutcome::ok(text),
                    Err(text) => ToolOutcome::error(text),
                }
            }
        }
    }

    fn validator<'a>(&'a self, refs: &'a dyn RefResolver) -> Validator<'a> {
        Validator::new(&self.catalogs, self.agent.host.actions(), refs)
            .with_limits(self.agent.config.limits)
            .allow_http(self.agent.host.allow_http())
    }

    fn prepare(
        &self,
        validator: &Validator<'_>,
        components: &mut Vec<Component>,
        data: &Value,
        operation: Operation,
        apps: &Apps,
    ) -> Result<Vec<SealedAction>, Problems> {
        let list = if operation == Operation::Update {
            "upsert"
        } else {
            "components"
        };
        *components = validator.resolve_refs(components, operation)?;
        let mut problems = charts::attach_scenes(components, data, list, &self.chart_options);
        if self.embeds {
            problems.extend(apps::attach_apps(
                components,
                apps,
                list,
                &self.agent.config.limits,
            ));
        }
        if !problems.is_empty() {
            return Err(Problems {
                operation,
                items: problems,
            });
        }
        Ok(validate::seal_actions(
            components,
            &self.catalogs,
            self.agent.host.actions(),
            ids::action_id,
        ))
    }

    fn register(ui: &mut UiState, surface: &str, sealed: Vec<SealedAction>) {
        for action in sealed {
            ui.handles.insert(
                action.action_id.clone(),
                Handle {
                    surface: surface.to_string(),
                    sealed: action,
                },
            );
        }
    }

    pub async fn show_ui(&self, input: &Value) -> ToolOutcome {
        match self.apps_for(Operation::Show, input, None).await {
            Ok(apps) => self.show_ui_with(input, None, None, &apps),
            Err(outcome) => outcome,
        }
    }

    fn too_many_surfaces(&self, ui: &UiState) -> ToolOutcome {
        let max = self.agent.config.max_open_surfaces;
        ToolOutcome::error(
            Problems {
                operation: Operation::Show,
                items: vec![Problem::new(
                    Location::Input,
                    format!(
                        "{max} surfaces are already open, which is the most one conversation can have. Close one with close_ui or change one with update_ui. Open surfaces: {}.",
                        ui.open_list()
                    ),
                )],
            }
            .to_string(),
        )
    }

    fn show_ui_with(
        &self,
        input: &Value,
        extra: Option<&dyn RefResolver>,
        draft: Option<&Draft>,
        apps: &Apps,
    ) -> ToolOutcome {
        let host_refs = self.agent.host.refs(&self.user);
        let refs = Layered {
            first: extra,
            then: host_refs.as_ref(),
        };
        let validator = self.validator(&refs);
        let mut surface = match validator.show_ui(input) {
            Ok(surface) => surface,
            Err(problems) => return ToolOutcome::error(problems.to_string()),
        };
        let streamed = draft
            .and_then(|draft| draft.surface.as_ref().map(|id| (draft, id.clone())))
            .filter(|(_, id)| surface.id.as_ref().is_none_or(|named| named == id));
        let data = surface.data_value();
        let mut client = surface.clone();
        let mut sealed = Vec::new();
        let mut positions = Vec::new();
        let mut fresh = Vec::new();
        for (index, component) in surface.components.iter().enumerate() {
            let earlier = streamed.as_ref().and_then(|(draft, _)| {
                let at = draft.model.iter().position(|seen| seen == component)?;
                (component.type_name != charts::CHART_TYPE || surface.data.is_empty())
                    .then_some((draft, at))
            });
            match earlier {
                Some((draft, at)) => {
                    client.components[index] = draft.prepared[at].clone();
                    sealed.extend(
                        draft
                            .sealed
                            .iter()
                            .filter(|action| action.component == component.id)
                            .cloned(),
                    );
                }
                None => {
                    positions.push(index);
                    fresh.push(component.clone());
                }
            }
        }
        if !fresh.is_empty() {
            match self.prepare(&validator, &mut fresh, &data, Operation::Show, apps) {
                Ok(more) => sealed.extend(more),
                Err(mut problems) => {
                    reindex(&mut problems, &positions);
                    return ToolOutcome::error(problems.to_string());
                }
            }
            for (index, component) in positions.into_iter().zip(fresh) {
                client.components[index] = component;
            }
        }
        let mut ui = self.lock();
        let own = streamed.as_ref().map(|(_, id)| id.clone());
        let others = ui
            .surfaces
            .keys()
            .filter(|key| own.as_ref() != Some(*key))
            .count();
        if others >= self.agent.config.max_open_surfaces {
            return self.too_many_surfaces(&ui);
        }
        let id = match (own, surface.id.clone()) {
            (Some(own), _) => own,
            (None, Some(id)) if ui.surfaces.contains_key(&id) => {
                return ToolOutcome::error(
                    Problems {
                        operation: Operation::Show,
                        items: vec![Problem::new(
                            Location::Field("surface".into()),
                            format!("\"{id}\" is already open. Change it with update_ui or close it with close_ui first."),
                        )],
                    }
                    .to_string(),
                );
            }
            (None, Some(id)) => id,
            (None, None) => ui.next_surface_id(),
        };
        surface.id = Some(id.clone());
        client.id = Some(id.clone());
        let view = self.client_view(&client);
        let earlier = streamed
            .as_ref()
            .and_then(|_| ui.surfaces.get(&id))
            .filter(|state| state.pending)
            .cloned();
        let (messages, seq) = match earlier {
            None => {
                let fallback = client.fallback(&self.catalogs, self.locale);
                let open = Message::Open(Open {
                    surface: id.clone(),
                    seq: 1,
                    root: view.root,
                    components: view.components,
                    data: view.data,
                    fallback: Some(fallback),
                })
                .to_value();
                if let Err(outcome) = self.fits_client(Operation::Show, &[&open]) {
                    return outcome;
                }
                (vec![open], 1)
            }
            Some(state) => {
                let before = self.client_view(&state.client);
                let mut seq = state.seq;
                let mut messages = Vec::new();
                let ops = data_changes(&before.data, &view.data);
                if !ops.is_empty() {
                    seq += 1;
                    messages.push(
                        Message::Data(DataUpdate {
                            surface: id.clone(),
                            seq,
                            ops,
                        })
                        .to_value(),
                    );
                }
                let upsert: Vec<Component> = view
                    .components
                    .iter()
                    .filter(|component| before.get(&component.id) != Some(*component))
                    .cloned()
                    .collect();
                let remove: Vec<String> = before
                    .components
                    .iter()
                    .filter(|component| view.get(&component.id).is_none())
                    .map(|component| component.id.clone())
                    .collect();
                let root = (before.root != view.root).then(|| view.root.clone());
                if !upsert.is_empty() || !remove.is_empty() || root.is_some() {
                    seq += 1;
                    messages.push(
                        Message::Patch(Patch {
                            surface: id.clone(),
                            seq,
                            upsert,
                            remove,
                            root,
                        })
                        .to_value(),
                    );
                }
                let snapshot = Message::Snapshot(Snapshot {
                    surface: id.clone(),
                    seq,
                    root: view.root,
                    components: view.components,
                    data: view.data,
                })
                .to_value();
                let outgoing: Vec<&Value> = messages.iter().chain([&snapshot]).collect();
                if let Err(outcome) = self.fits_client(Operation::Show, &outgoing) {
                    return outcome;
                }
                (messages, seq)
            }
        };
        Self::register(&mut ui, &id, sealed);
        let summary = surface.summary(&id, &self.catalogs, self.locale);
        ui.surfaces.insert(
            id.clone(),
            SurfaceState {
                model: surface,
                client,
                seq,
                pending: false,
            },
        );
        drop(ui);
        for message in messages {
            self.send(message);
        }
        ToolOutcome::ok(summary)
    }

    fn fits_client(&self, operation: Operation, messages: &[&Value]) -> Result<(), ToolOutcome> {
        let limits = self.agent.config.limits;
        for message in messages {
            if let Err(error) = protocol::check_for_client(message, &limits) {
                return Err(ToolOutcome::error(
                    Problems {
                        operation,
                        items: vec![Problem::new(
                            Location::Input,
                            format!(
                                "the {} message for the client would break a limit: {error}. Show less at once.",
                                message["t"].as_str().unwrap_or("ui")
                            ),
                        )],
                    }
                    .to_string(),
                ));
            }
        }
        Ok(())
    }

    fn not_open(&self, operation: Operation, id: &str, open: &str) -> ToolOutcome {
        ToolOutcome::error(
            Problems {
                operation,
                items: vec![Problem::new(
                    Location::Field("surface".into()),
                    format!("\"{id}\" is not open. Open surfaces: {open}."),
                )],
            }
            .to_string(),
        )
    }

    pub async fn update_ui(&self, input: &Value) -> ToolOutcome {
        match self.apps_for(Operation::Update, input, None).await {
            Ok(apps) => self.update_ui_with(input, None, &apps),
            Err(outcome) => outcome,
        }
    }

    fn update_ui_with(
        &self,
        input: &Value,
        extra: Option<&dyn RefResolver>,
        apps: &Apps,
    ) -> ToolOutcome {
        let host_refs = self.agent.host.refs(&self.user);
        let refs = Layered {
            first: extra,
            then: host_refs.as_ref(),
        };
        let validator = self.validator(&refs);
        let id = input
            .get("surface")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut ui = self.lock();
        let open = ui.open_list();
        let Some(state) = ui.settled(&id).cloned() else {
            return self.not_open(Operation::Update, &id, &open);
        };
        let update = match validator.update_ui(input, &state.model) {
            Ok(update) => update,
            Err(problems) => return ToolOutcome::error(problems.to_string()),
        };
        let mut upsert = match validator.resolve_refs(&update.upsert, Operation::Update) {
            Ok(components) => components,
            Err(problems) => return ToolOutcome::error(problems.to_string()),
        };
        let mut data_after = state.client.clone();
        let data_only = Update {
            surface: id.clone(),
            data: update.data.clone(),
            ..Update::default()
        };
        if let Err(error) = data_after.apply(&data_only) {
            return ToolOutcome::error(format!(
                "1 problem, nothing was changed:\n1. data: {error}."
            ));
        }
        let mut problems = charts::attach_scenes(
            &mut upsert,
            &data_after.data_value(),
            "upsert",
            &self.chart_options,
        );
        if self.embeds {
            problems.extend(apps::attach_apps(
                &mut upsert,
                apps,
                "upsert",
                &self.agent.config.limits,
            ));
        }
        if !problems.is_empty() {
            return ToolOutcome::error(
                Problems {
                    operation: Operation::Update,
                    items: problems,
                }
                .to_string(),
            );
        }
        let mut model = state.model.clone();
        let mut client = state.client.clone();
        let sealed = validate::seal_actions(
            &mut upsert,
            &self.catalogs,
            self.agent.host.actions(),
            ids::action_id,
        );
        let client_update = Update {
            upsert,
            ..update.clone()
        };
        if let Err(error) = model
            .apply(&update)
            .and_then(|()| client.apply(&client_update))
        {
            return ToolOutcome::error(format!(
                "1 problem, nothing was changed:\n1. data: {error}."
            ));
        }
        let before = self.client_view(&state.client);
        let after = self.client_view(&client);
        let changed: Vec<_> = after
            .components
            .iter()
            .filter(|component| !before.components.contains(component))
            .cloned()
            .collect();
        let gone: Vec<String> = before
            .components
            .iter()
            .filter(|component| after.get(&component.id).is_none())
            .map(|component| component.id.clone())
            .collect();
        let touched: Vec<&str> = update
            .upsert
            .iter()
            .map(|component| component.id.as_str())
            .chain(update.remove.iter().map(String::as_str))
            .collect();
        let mut seq = state.seq;
        let mut messages = Vec::new();
        if !update.data.is_empty() {
            seq += 1;
            messages.push(
                Message::Data(DataUpdate {
                    surface: id.clone(),
                    seq,
                    ops: update.data.clone(),
                })
                .to_value(),
            );
        }
        if !changed.is_empty() || !gone.is_empty() || update.root.is_some() {
            seq += 1;
            messages.push(
                Message::Patch(Patch {
                    surface: id.clone(),
                    seq,
                    upsert: changed,
                    remove: gone,
                    root: update.root.clone(),
                })
                .to_value(),
            );
        }
        let snapshot = Message::Snapshot(Snapshot {
            surface: id.clone(),
            seq,
            root: after.root.clone(),
            components: after.components.clone(),
            data: after.data.clone(),
        })
        .to_value();
        let outgoing: Vec<&Value> = messages.iter().chain([&snapshot]).collect();
        if let Err(outcome) = self.fits_client(Operation::Update, &outgoing) {
            return outcome;
        }
        ui.forget_actions(&id, Some(&touched));
        Self::register(&mut ui, &id, sealed);
        let summary = model.summary(&id, &self.catalogs, self.locale);
        ui.surfaces.insert(
            id.clone(),
            SurfaceState {
                model,
                client,
                seq,
                pending: false,
            },
        );
        drop(ui);
        for message in messages {
            self.send(message);
        }
        ToolOutcome::ok(summary)
    }

    #[cfg(feature = "query")]
    fn saved(&self) -> MutexGuard<'_, crate::dashboards::SavedQueries> {
        self.saved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(feature = "query")]
    fn check_only(
        &self,
        name: &str,
        input: &Value,
        extra: &dyn RefResolver,
    ) -> Result<(), ToolOutcome> {
        let host_refs = self.agent.host.refs(&self.user);
        let refs = Layered {
            first: Some(extra),
            then: host_refs.as_ref(),
        };
        let validator = self.validator(&refs);
        let checked = if name == "show_ui" {
            validator.show_ui(input).map(|_| ())
        } else {
            let id = input
                .get("surface")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let model = self.lock().settled(id).map(|state| state.model.clone());
            match model {
                Some(model) => validator.update_ui(input, &model).map(|_| ()),
                None => Ok(()),
            }
        };
        checked.map_err(|problems| ToolOutcome::error(problems.to_string()))
    }

    #[cfg(feature = "query")]
    async fn call_data_tool(
        &self,
        name: &str,
        input: &Value,
        draft: Option<&Draft>,
        apps: &Apps,
    ) -> Option<ToolOutcome> {
        use crate::dashboards::{self, QueryRefs};
        let engine = self.agent.host.data()?;
        match name {
            dashboards::DESCRIBE_TOOL => Some(ToolOutcome::ok(engine.describe())),
            dashboards::QUERY_TOOL => {
                let Some(sql) = input.get("sql").and_then(Value::as_str) else {
                    return Some(ToolOutcome::error(
                        "run_query needs a string \"sql\" with one SELECT statement.",
                    ));
                };
                let result = match engine.run(sql).await {
                    Ok(result) => result,
                    Err(error) => {
                        return Some(ToolOutcome::error(format!("query failed: {error}")));
                    }
                };
                let Some(id) = self.saved().save(sql) else {
                    return Some(ToolOutcome::error(format!(
                        "at most {} queries can be saved in one conversation.",
                        dashboards::MAX_SAVED
                    )));
                };
                Some(ToolOutcome::ok(
                    QueryRefs::new(&self.catalogs).summary(&id, &result),
                ))
            }
            "show_ui" | "update_ui" => {
                let (list, operation) = if name == "show_ui" {
                    ("components", Operation::Show)
                } else {
                    ("upsert", Operation::Update)
                };
                let uses = dashboards::query_refs(input, list);
                let declared = QueryRefs::new(&self.catalogs);
                if !uses.is_empty()
                    && let Err(outcome) = self.check_only(name, input, &declared)
                {
                    return Some(outcome);
                }
                let saved = self.saved().entries().clone();
                let refs = match declared
                    .fill(engine, &saved, &uses, self.chart_options.locale)
                    .await
                {
                    Ok(refs) => refs,
                    Err(items) => {
                        return Some(ToolOutcome::error(
                            Problems { operation, items }.to_string(),
                        ));
                    }
                };
                Some(if name == "show_ui" {
                    self.show_ui_with(input, Some(&refs), draft, apps)
                } else {
                    self.update_ui_with(input, Some(&refs), apps)
                })
            }
            _ => None,
        }
    }

    pub fn close_ui(&self, input: &Value) -> ToolOutcome {
        let refs = self.agent.host.refs(&self.user);
        let validator = self.validator(refs.as_ref());
        let id = match validator.close_ui(input) {
            Ok(id) => id,
            Err(problems) => return ToolOutcome::error(problems.to_string()),
        };
        let mut ui = self.lock();
        let open = ui.open_list();
        if ui.settled(&id).is_none() {
            return self.not_open(Operation::Close, &id, &open);
        }
        let Some(state) = ui.surfaces.remove(&id) else {
            return self.not_open(Operation::Close, &id, &open);
        };
        ui.forget_actions(&id, None);
        drop(ui);
        self.send(
            Message::Close(Close {
                surface: id.clone(),
                seq: state.seq + 1,
            })
            .to_value(),
        );
        ToolOutcome::ok(summary::closed(&id))
    }
}
