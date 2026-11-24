use crate::charts;
use crate::host::{ActionCall, ActionOutcome, Host, User};
use crate::ids;
use crate::provider::{ModelEvent, ModelProvider, ModelRequest, StopReason, Usage};
use futures_util::StreamExt;
use polyui_chart::ChartOptions;
use polyui_core::protocol::{
    self, Action, ActionResult, ActionStatus, Close, Confirm, DataUpdate, ErrorReason, Hello,
    Incoming, Message, Modality, Open, Patch, Snapshot, Welcome,
};
use polyui_core::tools::{self, ToolOptions};
use polyui_core::validate::{
    self, Location, Operation, Problem, Problems, SealedAction, Surface, Update, Validator,
};
use polyui_core::{CatalogSet, Limits, Locale, pointer, summary};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub const SAY: &str = "user.say";
pub const DELTA: &str = "agent.delta";
pub const DONE: &str = "agent.done";
pub const ERROR: &str = "agent.error";
pub const UI_TOOLS: [&str; 3] = ["show_ui", "update_ui", "close_ui"];
pub const MAX_SAY_CHARS: usize = 4000;

const GUIDE: &str = "How to answer with UI:
- Call show_ui to put UI on the user's screen. Components are a flat list. A parent lists the ids of its children in children. Every id is unique and every component must be reachable from root.
- The tool result tells you what is on screen now, or lists numbered problems. When it lists problems, fix exactly those and call the tool again.
- Use update_ui to change a surface that is open instead of opening a new one. Use close_ui when a surface is no longer useful.
- Never write secret values such as account or card numbers into props. Write {\"$ref\": \"source:key\"} instead. The server fills in the real value for the user only, and you will only ever see [ref].
- A Button fires a host action through on_press: {\"action\": \"name\", \"args\": {...}}. Actions that move money ask the user to confirm on their own, so do not build your own confirm step for them.
- When the user uses the UI you get a message that starts with [UI event]. Answer it briefly and update the UI when the result changes what is shown.
- Keep your text short. The UI carries the details.";

#[derive(Debug, Clone, Copy)]
pub struct AgentConfig {
    pub max_tokens: u32,
    pub max_retries: u32,
    pub max_model_calls: u32,
    pub confirm_ttl: Duration,
    pub limits: Limits,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tokens: 16000,
            max_retries: 2,
            max_model_calls: 8,
            confirm_ttl: Duration::from_secs(60),
            limits: Limits::default(),
        }
    }
}

#[derive(Clone)]
pub struct Agent {
    host: Arc<dyn Host>,
    provider: Arc<dyn ModelProvider>,
    config: AgentConfig,
}

impl Agent {
    pub fn new(host: Arc<dyn Host>, provider: Arc<dyn ModelProvider>) -> Self {
        Self {
            host,
            provider,
            config: AgentConfig::default(),
        }
    }

    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub fn host(&self) -> &Arc<dyn Host> {
        &self.host
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    pub fn open_session(
        &self,
        hello: Hello,
        user: User,
    ) -> (Arc<Session>, mpsc::UnboundedReceiver<Value>) {
        Session::start(self.clone(), hello, user)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SurfaceState {
    model: Surface,
    client: Surface,
    seq: u64,
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

    fn open_list(&self) -> String {
        if self.surfaces.is_empty() {
            "none".into()
        } else {
            self.surfaces.keys().cloned().collect::<Vec<_>>().join(", ")
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

pub struct Session {
    id: String,
    user: User,
    agent: Agent,
    catalogs: CatalogSet,
    client_catalogs: CatalogSet,
    negotiated: Vec<String>,
    locale: Locale,
    chart_options: ChartOptions,
    system: String,
    tools: Vec<Value>,
    ui: Mutex<UiState>,
    usage: Mutex<Usage>,
    out: mpsc::UnboundedSender<Value>,
    turns: mpsc::UnboundedSender<TurnInput>,
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
    fn start(
        agent: Agent,
        hello: Hello,
        user: User,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<Value>) {
        let catalogs = agent.host.catalogs();
        let negotiated = catalogs.negotiate(&hello.catalogs);
        let client_catalogs = catalogs.restrict(&negotiated);
        let tag = hello.locale.clone().unwrap_or_else(|| "en".into());
        let locale = Locale::from_tag(&tag);
        let chart_options = ChartOptions::with_locale(polyui_chart::Locale::parse(&tag));
        let mut tool_list = match tools::for_client(
            &catalogs,
            &negotiated,
            agent.host.actions(),
            ToolOptions { strict: true },
        ) {
            Value::Array(items) => items,
            _ => Vec::new(),
        };
        tool_list.extend(agent.host.tools());
        tools::limit_strict(&mut tool_list);
        let system = system_prompt(
            &agent.host.system_prompt(&user, locale),
            &hello,
            &negotiated,
            locale,
        );
        let (out, outgoing) = mpsc::unbounded_channel();
        let (turns, mut incoming) = mpsc::unbounded_channel();
        let session = Arc::new(Self {
            id: ids::session_id(),
            user,
            agent,
            catalogs,
            client_catalogs,
            negotiated,
            locale,
            chart_options,
            system,
            tools: tool_list,
            ui: Mutex::new(UiState::default()),
            usage: Mutex::new(Usage::default()),
            out,
            turns,
        });
        let weak: Weak<Self> = Arc::downgrade(&session);
        tokio::spawn(async move {
            let mut history = Vec::new();
            while let Some(input) = incoming.recv().await {
                let Some(session) = weak.upgrade() else {
                    break;
                };
                session.run_turn(&mut history, input).await;
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
        Message::Welcome(Welcome {
            session: self.id.clone(),
            catalogs: self.negotiated.clone(),
        })
        .to_value()
    }

    pub fn open_surfaces(&self) -> Vec<String> {
        self.lock().surfaces.keys().cloned().collect()
    }

    pub fn model_summary(&self, surface: &str) -> Option<String> {
        self.lock()
            .surfaces
            .get(surface)
            .map(|state| state.model.summary(surface, &self.catalogs, self.locale))
    }

    fn lock(&self) -> MutexGuard<'_, UiState> {
        self.ui
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn send(&self, message: Value) {
        let _ = self.out.send(message);
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
                    Some(said) if said.trim().is_empty() => {}
                    Some(said) if said.chars().count() <= MAX_SAY_CHARS => {
                        let _ = self.turns.send(TurnInput::Say(said));
                    }
                    Some(_) => self.send_error(
                        None,
                        ErrorReason::BadMessage,
                        format!("user.say text is longer than {MAX_SAY_CHARS} characters"),
                    ),
                    None => self.send_error(
                        None,
                        ErrorReason::BadMessage,
                        "user.say needs a string \"text\"".into(),
                    ),
                }
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
        if !ui.surfaces.contains_key(&action.surface) {
            return ActionStep::Reply(
                ActionStatus::Rejected,
                "This screen is no longer open.".into(),
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
        if !handle.sealed.requires_confirm {
            return ActionStep::Run {
                sealed: Box::new(handle.sealed),
                values,
                detail: action.detail.clone(),
            };
        }
        match &action.confirm {
            None => {
                let nonce = ids::nonce();
                ui.confirms.retain(|pending| {
                    !(pending.surface == action.surface && pending.action_id == action.action_id)
                });
                ui.confirms.push(PendingConfirm {
                    nonce: nonce.clone(),
                    surface: action.surface.clone(),
                    action_id: action.action_id.clone(),
                    expires: now + ttl,
                    values,
                    detail: action.detail.clone(),
                });
                let text = self.agent.host.confirm_text(
                    &self.user,
                    &handle.sealed.action,
                    &handle.sealed.args,
                    self.locale,
                );
                ActionStep::Ask(
                    Message::Confirm(Confirm {
                        surface: action.surface.clone(),
                        action_id: action.action_id.clone(),
                        nonce,
                        title: text.title,
                        body: text.body,
                        confirm_label: text.confirm_label,
                        expires_ms: u64::try_from(ttl.as_millis()).ok(),
                        seq: None,
                    })
                    .to_value(),
                )
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

    fn store_values(&self, surface: &str, values: &Map<String, Value>) -> bool {
        if values.is_empty() {
            return true;
        }
        let mut ui = self.lock();
        let Some(state) = ui.surfaces.get_mut(surface) else {
            return false;
        };
        let mut model = state.model.clone();
        let mut client = state.client.clone();
        for (path, value) in values {
            if set_value(&mut model, path, value.clone()).is_err()
                || set_value(&mut client, path, value.clone()).is_err()
            {
                return false;
            }
        }
        if polyui_core::limits::check_data_model(&client.data_value(), &self.agent.config.limits)
            .is_err()
        {
            return false;
        }
        state.model = model;
        state.client = client;
        true
    }

    async fn on_action(&self, action: Action) {
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
        if !self.store_values(&action.surface, &values) {
            self.action_result(
                &action.surface,
                &action.action_id,
                ActionStatus::Rejected,
                Some("The form values could not be used.".into()),
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
        let _ = self.turns.send(TurnInput::Report(report));
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
            let mut finished = None;
            while let Some(event) = stream.next().await {
                match event {
                    ModelEvent::TextDelta(delta) if !delta.is_empty() => {
                        self.send(chat_text(DELTA, "text", &delta));
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
                        self.send(chat_text(ERROR, "message", error.user_message()));
                        return;
                    }
                    _ => {}
                }
            }
            let Some((stop_reason, content, usage)) = finished else {
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
                self.send(chat(DONE));
                return;
            }
            let mut results = Vec::new();
            let mut gave_up = false;
            for (id, name, input) in calls {
                let mut outcome = match input {
                    Err(raw) => ToolOutcome::error(json!({ "INVALID_JSON": raw }).to_string()),
                    Ok(input) => self.call_tool(&name, &input).await,
                };
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

    pub async fn call_tool(&self, name: &str, input: &Value) -> ToolOutcome {
        match name {
            "show_ui" => self.show_ui(input),
            "update_ui" => self.update_ui(input),
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

    fn validator<'a>(&'a self, refs: &'a dyn polyui_core::RefResolver) -> Validator<'a> {
        Validator::new(&self.catalogs, self.agent.host.actions(), refs)
            .with_limits(self.agent.config.limits)
            .allow_http(self.agent.host.allow_http())
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

    pub fn show_ui(&self, input: &Value) -> ToolOutcome {
        let refs = self.agent.host.refs(&self.user);
        let validator = self.validator(refs.as_ref());
        let mut surface = match validator.show_ui(input) {
            Ok(surface) => surface,
            Err(problems) => return ToolOutcome::error(problems.to_string()),
        };
        let mut client = surface.clone();
        client.components = match validator.resolve_refs(&surface.components, Operation::Show) {
            Ok(components) => components,
            Err(problems) => return ToolOutcome::error(problems.to_string()),
        };
        let data = client.data_value();
        let problems = charts::attach_scenes(
            &mut client.components,
            &data,
            "components",
            &self.chart_options,
        );
        if !problems.is_empty() {
            return ToolOutcome::error(
                Problems {
                    operation: Operation::Show,
                    items: problems,
                }
                .to_string(),
            );
        }
        let mut ui = self.lock();
        let id = match surface.id.clone() {
            Some(id) if ui.surfaces.contains_key(&id) => {
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
            Some(id) => id,
            None => ui.next_surface_id(),
        };
        let sealed = validate::seal_actions(
            &mut client.components,
            &self.catalogs,
            self.agent.host.actions(),
            ids::action_id,
        );
        surface.id = Some(id.clone());
        client.id = Some(id.clone());
        let view = self.client_view(&client);
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
        Self::register(&mut ui, &id, sealed);
        let summary = surface.summary(&id, &self.catalogs, self.locale);
        ui.surfaces.insert(
            id.clone(),
            SurfaceState {
                model: surface,
                client,
                seq: 1,
            },
        );
        drop(ui);
        self.send(open);
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

    pub fn update_ui(&self, input: &Value) -> ToolOutcome {
        let refs = self.agent.host.refs(&self.user);
        let validator = self.validator(refs.as_ref());
        let id = input
            .get("surface")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut ui = self.lock();
        let open = ui.open_list();
        let Some(state) = ui.surfaces.get(&id).cloned() else {
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
        let problems = charts::attach_scenes(
            &mut upsert,
            &data_after.data_value(),
            "upsert",
            &self.chart_options,
        );
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
        ui.surfaces
            .insert(id.clone(), SurfaceState { model, client, seq });
        drop(ui);
        for message in messages {
            self.send(message);
        }
        ToolOutcome::ok(summary)
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
