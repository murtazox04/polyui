use crate::meter::{Budget, Metered, Pacer, Trace, lock};
use crate::prompts::{Expect, Prompt};
use crate::report::problem_kinds;
use clap::ValueEnum;
use futures_util::StreamExt;
use polyui_core::protocol::{self, Hello};
use polyui_core::tools::ToolProfile;
use polyui_decode::ToolGrammar;
use polyui_server::bills::{BillsHost, HOUSEHOLD_USER};
use polyui_server::session::UI_TOOLS;
use polyui_server::{
    Agent, AgentConfig, Host, ModelEvent, ModelProvider, ModelRequest, ToolOutcome, Usage, User,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Receiver;

pub const TURN_TIMEOUT: Duration = Duration::from_secs(240);
const TEXT_KEPT: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Variant {
    Tools,
    Constrained,
    Inline,
}

impl Variant {
    pub const ALL: [Self; 3] = [Self::Tools, Self::Constrained, Self::Inline];

    pub fn label(self) -> &'static str {
        match self {
            Self::Tools => "tool calls",
            Self::Constrained => "constrained JSON",
            Self::Inline => "inline JSON",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::Tools => "tools",
            Self::Constrained => "constr",
            Self::Inline => "inline",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Attempt {
    pub tool: String,
    pub ok: bool,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunRecord {
    pub prompt: String,
    pub locale: &'static str,
    pub category: &'static str,
    pub expect: Expect,
    pub variant: Variant,
    pub attempts: Vec<Attempt>,
    pub opened: bool,
    pub ms_to_open: Option<u64>,
    pub ms_total: u64,
    pub model_calls: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    pub ending: String,
    pub text: String,
}

impl RunRecord {
    pub fn first_valid(&self) -> Option<usize> {
        self.attempts.iter().position(|attempt| attempt.ok)
    }

    pub fn made_ui_call(&self) -> bool {
        !self.attempts.is_empty()
    }

    pub fn budget_cut(&self) -> bool {
        self.ending == "budget"
    }
}

pub struct Bench {
    pub provider: Arc<dyn ModelProvider>,
    pub constrained: Option<Arc<dyn ModelProvider>>,
    pub budget: Arc<Budget>,
    pub pacer: Arc<Pacer>,
    pub config: AgentConfig,
    pub tool_types: Vec<String>,
}

impl Bench {
    fn agent(&self, host: Arc<BillsHost>, provider: Arc<Metered>) -> Agent {
        Agent::new(host, provider)
            .with_config(self.config)
            .with_tool_types(self.tool_types.clone())
    }

    fn metered(&self, provider: Arc<dyn ModelProvider>) -> Arc<Metered> {
        Arc::new(Metered::new(
            provider,
            self.budget.clone(),
            self.pacer.clone(),
        ))
    }
}

pub fn hello(locale: &str) -> Hello {
    let text = json!({ "v": 1, "t": "hello", "client": { "name": "polyui-bench", "version": env!("CARGO_PKG_VERSION") }, "catalogs": ["core@1", "chart@1"], "modalities": ["screen"], "locale": locale })
        .to_string();
    protocol::parse_hello(&text, &Default::default()).expect("the benchmark hello is valid")
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn kept(text: &str) -> String {
    text.chars().take(TEXT_KEPT).collect()
}

fn result_text(block: &Value) -> String {
    match &block["content"] {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn tool_results(trace: &Trace) -> HashMap<String, (bool, String)> {
    let mut results = HashMap::new();
    for request in &trace.requests {
        for message in request
            .messages
            .iter()
            .filter(|message| message["role"] == "user")
        {
            for block in message["content"].as_array().into_iter().flatten() {
                if block["type"] == "tool_result"
                    && let Some(id) = block["tool_use_id"].as_str()
                {
                    results.insert(
                        id.to_string(),
                        (block["is_error"] == true, result_text(block)),
                    );
                }
            }
        }
    }
    results
}

pub fn ui_attempts(trace: &Trace, opens: usize, gave_up: bool) -> Vec<Attempt> {
    let results = tool_results(trace);
    let mut attempts = Vec::new();
    let mut shown = 0;
    for block in trace.replies.iter().flatten() {
        let Some(name) = block["name"].as_str() else {
            continue;
        };
        if block["type"] != "tool_use" || !UI_TOOLS.contains(&name) {
            continue;
        }
        let id = block["id"].as_str().unwrap_or_default();
        let attempt = match results.get(id) {
            Some((true, content)) => Attempt {
                tool: name.to_string(),
                ok: false,
                problems: problem_kinds(content),
            },
            Some((false, _)) => Attempt {
                tool: name.to_string(),
                ok: true,
                problems: Vec::new(),
            },
            None => {
                let ok = !gave_up && name == "show_ui" && opens > shown;
                Attempt {
                    tool: name.to_string(),
                    ok,
                    problems: if ok {
                        Vec::new()
                    } else {
                        vec!["result not seen, the turn ended after this call".to_string()]
                    },
                }
            }
        };
        if attempt.ok && name == "show_ui" {
            shown += 1;
        }
        attempts.push(attempt);
    }
    attempts
}

struct Outcome {
    ending: String,
    text: String,
    opens: usize,
    first_open: Option<Duration>,
}

async fn drain(outgoing: &mut Receiver<Value>, started: Instant) -> Outcome {
    let mut outcome = Outcome {
        ending: "timeout".into(),
        text: String::new(),
        opens: 0,
        first_open: None,
    };
    loop {
        let left = TURN_TIMEOUT.saturating_sub(started.elapsed());
        let message = match tokio::time::timeout(left, outgoing.recv()).await {
            Err(_) => return outcome,
            Ok(None) => {
                outcome.ending = "closed".into();
                return outcome;
            }
            Ok(Some(message)) => message,
        };
        match message["t"].as_str() {
            Some("ui.open") => {
                outcome.opens += 1;
                outcome.first_open.get_or_insert_with(|| started.elapsed());
            }
            Some("agent.delta") => {
                outcome
                    .text
                    .push_str(message["text"].as_str().unwrap_or_default());
            }
            Some("agent.done") => {
                outcome.ending = "done".into();
                return outcome;
            }
            Some("agent.error") => {
                outcome.ending =
                    format!("error: {}", message["message"].as_str().unwrap_or_default());
                return outcome;
            }
            _ => {}
        }
    }
}

fn record(
    prompt: &Prompt,
    variant: Variant,
    attempts: Vec<Attempt>,
    outcome: Outcome,
    usage: Usage,
    calls: usize,
    total: Duration,
) -> RunRecord {
    RunRecord {
        prompt: prompt.id.clone(),
        locale: prompt.locale,
        category: prompt.category,
        expect: prompt.expect,
        variant,
        attempts,
        opened: outcome.opens > 0,
        ms_to_open: outcome.first_open.map(millis),
        ms_total: millis(total),
        model_calls: calls,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_write_tokens: usage.cache_creation_input_tokens,
        cache_read_tokens: usage.cache_read_input_tokens,
        ending: outcome.ending,
        text: kept(&outcome.text),
    }
}

pub async fn run_tools(bench: &Bench, prompt: &Prompt) -> RunRecord {
    let metered = bench.metered(bench.provider.clone());
    let trace = metered.trace();
    let host = Arc::new(BillsHost::new());
    let agent = bench.agent(host.clone(), metered);
    let user = User::new(HOUSEHOLD_USER);
    let (session, mut outgoing) = agent.open_session(hello(prompt.locale), user);
    let started = Instant::now();
    session
        .receive(&json!({ "v": 1, "t": "user.say", "text": prompt.text }).to_string())
        .await;
    let mut outcome = drain(&mut outgoing, started).await;
    let total = started.elapsed();
    let trace = lock(&trace);
    if trace.refused {
        outcome.ending = "budget".into();
    }
    let gave_up = outcome.text.contains(&host.fallback_text(session.locale()));
    let attempts = ui_attempts(&trace, outcome.opens, gave_up);
    record(
        prompt,
        Variant::Tools,
        attempts,
        outcome,
        trace.usage,
        trace.requests.len(),
        total,
    )
}

pub fn inline_guide(show_ui: &Value, profile: ToolProfile) -> String {
    let schema = &show_ui["input_schema"];
    let types = match profile {
        ToolProfile::Full => String::new(),
        ToolProfile::Compact => {
            format!("\n{}", show_ui["description"].as_str().unwrap_or_default())
        }
    };
    format!(
        "Important: in this client show_ui, update_ui and close_ui are not tools you can call. To put UI on the screen, write the show_ui input as one JSON object inside a single fenced code block that starts with ```json and ends with ```. Text outside the block is shown to the user as plain text. The JSON object must follow this JSON Schema:\n{schema}{types}\nIf the next message lists problems, write the whole corrected JSON again in one fenced json block. When no UI is needed, answer in plain text with no code block."
    )
}

pub fn fenced_json(text: &str) -> Option<String> {
    text.split("```")
        .skip(1)
        .step_by(2)
        .map(|block| {
            let body = match block.split_once('\n') {
                Some((tag, rest)) if !tag.trim_start().starts_with('{') => rest,
                _ => block,
            };
            body.trim().to_string()
        })
        .find(|body| body.starts_with('{'))
}

fn tool_result_block(id: &str, content: &str, is_error: bool) -> Value {
    let mut block = json!({ "type": "tool_result", "tool_use_id": id, "content": content });
    if is_error {
        block["is_error"] = Value::Bool(true);
    }
    block
}

fn count_opens(outgoing: &mut Receiver<Value>) -> usize {
    let mut opens = 0;
    while let Ok(message) = outgoing.try_recv() {
        if message["t"] == "ui.open" {
            opens += 1;
        }
    }
    opens
}

struct Reply {
    text: String,
    calls: Vec<(String, String, Result<Value, String>)>,
    content: Vec<Value>,
    cut_off: Option<String>,
}

async fn next_reply(provider: &Metered, request: ModelRequest) -> Result<Reply, String> {
    let mut stream = provider.stream(request);
    let mut reply = Reply {
        text: String::new(),
        calls: Vec::new(),
        content: Vec::new(),
        cut_off: None,
    };
    loop {
        let event = tokio::time::timeout(TURN_TIMEOUT, stream.next())
            .await
            .map_err(|_| "timeout".to_string())?;
        match event {
            None => return Err("the stream ended before the reply finished".into()),
            Some(ModelEvent::TextDelta(delta)) => reply.text.push_str(&delta),
            Some(ModelEvent::ToolCallFinished { id, name, input }) => {
                reply.calls.push((id, name, input));
            }
            Some(ModelEvent::MessageFinished {
                stop_reason,
                content,
                ..
            }) => {
                reply.content = content;
                if stop_reason.is_cut_off() {
                    reply.cut_off = Some(stop_reason.as_str().to_string());
                }
                return Ok(reply);
            }
            Some(ModelEvent::Error(error)) => return Err(error.detail),
            Some(_) => {}
        }
    }
}

pub async fn run_inline(bench: &Bench, prompt: &Prompt) -> RunRecord {
    let metered = bench.metered(bench.provider.clone());
    let trace = metered.trace();
    let host = Arc::new(BillsHost::new());
    let agent = bench.agent(host.clone(), metered.clone());
    let (session, mut outgoing) =
        agent.open_session(hello(prompt.locale), User::new(HOUSEHOLD_USER));
    let show_ui = session
        .tools()
        .iter()
        .find(|tool| tool["name"] == "show_ui")
        .cloned()
        .unwrap_or(Value::Null);
    let system = format!(
        "{}\n\n{}",
        session.system_prompt(),
        inline_guide(&show_ui, bench.config.tool_profile)
    );
    let tools: Vec<Value> = session
        .tools()
        .iter()
        .filter(|tool| {
            tool["name"]
                .as_str()
                .is_none_or(|name| !UI_TOOLS.contains(&name))
        })
        .cloned()
        .collect();
    let config = bench.config;
    let started = Instant::now();
    let mut history =
        vec![json!({ "role": "user", "content": [{ "type": "text", "text": prompt.text }] })];
    let mut attempts = Vec::new();
    let mut said = String::new();
    let mut first_open = None;
    let mut failures = 0;
    let mut ending = "done (call limit)".to_string();
    for _ in 0..config.max_model_calls {
        let request = ModelRequest {
            system: system.clone(),
            tools: tools.clone(),
            messages: history.clone(),
            max_tokens: config.max_tokens,
        };
        let reply = match next_reply(&metered, request).await {
            Ok(reply) => reply,
            Err(error) => {
                ending = format!("error: {error}");
                break;
            }
        };
        said.push_str(&reply.text);
        if !reply.content.is_empty() {
            history.push(json!({ "role": "assistant", "content": reply.content }));
        }
        if let Some(reason) = reply.cut_off {
            ending = format!("error: cut off ({reason})");
            break;
        }
        let mut blocks = Vec::new();
        for (id, name, input) in &reply.calls {
            let outcome = match input {
                Ok(input) => host.call_tool(session.user(), name, input).await,
                Err(raw) => Err(json!({ "INVALID_JSON": raw }).to_string()),
            };
            blocks.push(match outcome {
                Ok(text) => tool_result_block(id, &text, false),
                Err(text) => tool_result_block(id, &text, true),
            });
        }
        let mut shown = false;
        let mut gave_up = false;
        if let Some(block) = fenced_json(&reply.text) {
            let outcome = match serde_json::from_str::<Value>(&block) {
                Ok(input) => session.show_ui(&input).await,
                Err(_) => ToolOutcome {
                    content: json!({ "INVALID_JSON": block }).to_string(),
                    is_error: true,
                },
            };
            attempts.push(Attempt {
                tool: "show_ui".into(),
                ok: !outcome.is_error,
                problems: if outcome.is_error {
                    problem_kinds(&outcome.content)
                } else {
                    Vec::new()
                },
            });
            if outcome.is_error {
                failures += 1;
                gave_up = failures > config.max_retries;
                blocks.push(json!({ "type": "text", "text": format!("[UI result] {}\nWrite the whole corrected show_ui JSON again in one fenced json block.", outcome.content) }));
            } else {
                shown = true;
                first_open = Some(started.elapsed());
            }
        }
        if shown || gave_up || blocks.is_empty() {
            ending = "done".into();
            break;
        }
        history.push(json!({ "role": "user", "content": blocks }));
    }
    let total = started.elapsed();
    let opens = count_opens(&mut outgoing);
    let trace = lock(&trace);
    if trace.refused {
        ending = "budget".into();
    }
    let outcome = Outcome {
        ending,
        text: said,
        opens,
        first_open,
    };
    record(
        prompt,
        Variant::Inline,
        attempts,
        outcome,
        trace.usage,
        trace.requests.len(),
        total,
    )
}

pub const NO_TOOL: &str = "none";

fn branch(tool: &str, key: &str, schema: Value) -> Value {
    json!({
        "type": "object",
        "properties": { "tool": { "const": tool }, key: schema },
        "required": ["tool", key],
        "additionalProperties": false
    })
}

pub fn constrained_schema(tools: &[Value]) -> Result<Value, String> {
    let grammar = ToolGrammar::from_tools(&Value::Array(tools.to_vec()))
        .map_err(|error| error.to_string())?;
    let mut branches: Vec<Value> = grammar
        .tools()
        .iter()
        .map(|tool| branch(&tool.name, "input", tool.input_schema.clone()))
        .collect();
    branches.push(branch(NO_TOOL, "text", json!({ "type": "string" })));
    Ok(json!({ "anyOf": branches }))
}

pub fn constrained_guide(tools: &[Value]) -> String {
    let mut listed = String::new();
    for tool in tools {
        listed.push_str(&format!(
            "\n- {}: {}\n  input schema: {}",
            tool["name"].as_str().unwrap_or_default(),
            tool["description"].as_str().unwrap_or_default(),
            tool["input_schema"]
        ));
    }
    format!(
        "Important: in this client every reply you write is exactly one JSON object, and nothing else. To use a tool, reply {{\"tool\": \"<tool name>\", \"input\": <the tool input>}}. To answer in plain text without UI, reply {{\"tool\": \"none\", \"text\": \"<your answer>\"}}. The text of a none reply is shown to the user as it is. A tool result comes back in the next message; then reply again with one JSON object. If the next message lists problems, reply with the whole corrected call. Write object keys in the order the schemas list them. These are the tools:{listed}"
    )
}

fn constrained_feedback(name: &str, outcome: &ToolOutcome) -> String {
    if UI_TOOLS.contains(&name) && outcome.is_error {
        format!(
            "[{name} result] {}\nReply again with one JSON object with the whole corrected call.",
            outcome.content
        )
    } else {
        format!("[{name} result] {}", outcome.content)
    }
}

pub async fn run_constrained(bench: &Bench, prompt: &Prompt) -> RunRecord {
    let provider = bench
        .constrained
        .clone()
        .unwrap_or_else(|| bench.provider.clone());
    let metered = bench.metered(provider);
    let trace = metered.trace();
    let host = Arc::new(BillsHost::new());
    let agent = bench.agent(host.clone(), metered.clone());
    let (session, mut outgoing) =
        agent.open_session(hello(prompt.locale), User::new(HOUSEHOLD_USER));
    let system = format!(
        "{}\n\n{}",
        session.system_prompt(),
        constrained_guide(session.tools())
    );
    let config = bench.config;
    let started = Instant::now();
    let mut history =
        vec![json!({ "role": "user", "content": [{ "type": "text", "text": prompt.text }] })];
    let mut attempts = Vec::new();
    let mut said = String::new();
    let mut first_open = None;
    let mut failures = 0;
    let mut ending = "done (call limit)".to_string();
    for _ in 0..config.max_model_calls {
        let request = ModelRequest {
            system: system.clone(),
            tools: Vec::new(),
            messages: history.clone(),
            max_tokens: config.max_tokens,
        };
        let reply = match next_reply(&metered, request).await {
            Ok(reply) => reply,
            Err(error) => {
                ending = format!("error: {error}");
                break;
            }
        };
        if !reply.content.is_empty() {
            history.push(json!({ "role": "assistant", "content": reply.content }));
        }
        if let Some(reason) = reply.cut_off {
            ending = format!("error: cut off ({reason})");
            break;
        }
        let parsed = serde_json::from_str::<Value>(reply.text.trim()).ok();
        let name = parsed
            .as_ref()
            .and_then(|call| call["tool"].as_str())
            .map(str::to_string);
        let feedback = match (&parsed, name.as_deref()) {
            (Some(call), Some(NO_TOOL)) => {
                said.push_str(call["text"].as_str().unwrap_or_default());
                ending = "done".into();
                break;
            }
            (Some(call), Some(name)) => {
                let outcome = session.call_tool(name, &call["input"]).await;
                if UI_TOOLS.contains(&name) {
                    attempts.push(Attempt {
                        tool: name.to_string(),
                        ok: !outcome.is_error,
                        problems: if outcome.is_error {
                            problem_kinds(&outcome.content)
                        } else {
                            Vec::new()
                        },
                    });
                    if !outcome.is_error {
                        if name == "show_ui" {
                            first_open = Some(started.elapsed());
                        }
                        ending = "done".into();
                        break;
                    }
                    failures += 1;
                }
                constrained_feedback(name, &outcome)
            }
            _ => {
                attempts.push(Attempt {
                    tool: "reply".into(),
                    ok: false,
                    problems: vec!["the reply is not one JSON object with a tool".to_string()],
                });
                failures += 1;
                "[problem] The reply was not one JSON object with a \"tool\" key. Reply again with one JSON object.".to_string()
            }
        };
        if failures > config.max_retries {
            ending = "done".into();
            break;
        }
        history.push(json!({ "role": "user", "content": [{ "type": "text", "text": feedback }] }));
    }
    let total = started.elapsed();
    let opens = count_opens(&mut outgoing);
    let trace = lock(&trace);
    if trace.refused {
        ending = "budget".into();
    }
    let outcome = Outcome {
        ending,
        text: said,
        opens,
        first_open,
    };
    record(
        prompt,
        Variant::Constrained,
        attempts,
        outcome,
        trace.usage,
        trace.requests.len(),
        total,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyui_server::{ScriptedProvider, ScriptedReply};

    fn bench(scripted: Arc<ScriptedProvider>) -> Bench {
        Bench {
            provider: scripted,
            constrained: None,
            budget: Budget::new(20, 0),
            pacer: Pacer::new(Duration::ZERO),
            config: AgentConfig::default(),
            tool_types: Vec::new(),
        }
    }

    fn prompt() -> Prompt {
        Prompt {
            id: "en-bills".into(),
            locale: "en",
            category: "bills",
            expect: Expect::Ui,
            text: "Show my bills",
        }
    }

    fn card() -> Value {
        json!({ "root": "c", "components": [{ "id": "c", "type": "Card", "props": { "title": "Bills" }, "children": [] }] })
    }

    #[test]
    fn fenced_blocks_are_found_with_or_without_a_tag() {
        assert_eq!(
            fenced_json("Here:\n```json\n{\"root\": \"a\"}\n```\nDone").as_deref(),
            Some("{\"root\": \"a\"}")
        );
        assert_eq!(
            fenced_json("```\n{\"a\": 1}\n```").as_deref(),
            Some("{\"a\": 1}")
        );
        assert_eq!(
            fenced_json("```{\"a\": 1}```").as_deref(),
            Some("{\"a\": 1}")
        );
        assert_eq!(fenced_json("```text\nhello\n```"), None);
        assert_eq!(fenced_json("no block"), None);
    }

    #[tokio::test]
    async fn tool_runs_count_a_retry() {
        let scripted = Arc::new(ScriptedProvider::new());
        scripted.reply(ScriptedReply::new().tool(
            "t1",
            "show_ui",
            json!({ "root": "x", "components": [{ "id": "x", "type": "Html" }] }),
        ));
        scripted.reply(ScriptedReply::new().tool("t2", "show_ui", card()));
        scripted.reply(ScriptedReply::new().text("Done."));
        let record = run_tools(&bench(scripted), &prompt()).await;
        assert_eq!(record.ending, "done");
        assert_eq!(record.model_calls, 3);
        assert_eq!(record.first_valid(), Some(1));
        assert!(record.opened);
        assert_eq!(record.attempts[0].problems.len(), 1);
    }

    #[tokio::test]
    async fn tool_runs_that_give_up_count_every_attempt_as_failed() {
        let scripted = Arc::new(ScriptedProvider::new());
        for id in ["t1", "t2", "t3"] {
            scripted.reply(ScriptedReply::new().tool(
                id,
                "show_ui",
                json!({ "root": "x", "components": [] }),
            ));
        }
        let record = run_tools(&bench(scripted), &prompt()).await;
        assert_eq!(record.attempts.len(), 3);
        assert!(record.attempts.iter().all(|attempt| !attempt.ok));
        assert!(!record.opened);
    }

    #[tokio::test]
    async fn inline_runs_validate_the_fenced_json() {
        let scripted = Arc::new(ScriptedProvider::new());
        scripted.reply(ScriptedReply::new().text("```json\n{\"root\": \"x\", \"components\": [{\"id\": \"x\", \"type\": \"Html\"}]}\n```"));
        scripted
            .reply(ScriptedReply::new().text(&format!("Here you go.\n```json\n{}\n```", card())));
        let record = run_inline(&bench(scripted.clone()), &prompt()).await;
        assert_eq!(record.ending, "done");
        assert_eq!(record.first_valid(), Some(1));
        assert!(record.opened);
        assert_eq!(record.model_calls, 2);
        let requests = scripted.requests();
        assert!(
            requests[0]
                .tools
                .iter()
                .all(|tool| tool["name"] != "show_ui")
        );
        assert!(requests[0].system.contains("```json"));
        let feedback = requests[1].messages.last().unwrap().to_string();
        assert!(feedback.contains("is not in the catalog"), "{feedback}");
    }

    #[tokio::test]
    async fn constrained_runs_parse_the_json_call_and_retry_on_problems() {
        let scripted = Arc::new(ScriptedProvider::new());
        scripted.reply(ScriptedReply::new().text(
            &json!({ "tool": "show_ui", "input": { "root": "x", "components": [{ "id": "x", "type": "Html" }] } }).to_string(),
        ));
        scripted.reply(
            ScriptedReply::new().text(&json!({ "tool": "show_ui", "input": card() }).to_string()),
        );
        let record = run_constrained(&bench(scripted.clone()), &prompt()).await;
        assert_eq!(record.variant, Variant::Constrained);
        assert_eq!(record.ending, "done");
        assert_eq!(record.first_valid(), Some(1));
        assert!(record.opened);
        assert_eq!(record.model_calls, 2);
        let requests = scripted.requests();
        assert!(requests[0].tools.is_empty());
        assert!(requests[0].system.contains("\"tool\": \"none\""));
        let feedback = requests[1].messages.last().unwrap().to_string();
        assert!(feedback.contains("is not in the catalog"), "{feedback}");
    }

    #[tokio::test]
    async fn constrained_runs_feed_host_tool_results_back_and_end_on_text() {
        let scripted = Arc::new(ScriptedProvider::new());
        scripted.reply(
            ScriptedReply::new()
                .text(&json!({ "tool": "get_usage", "input": { "service": "gas" } }).to_string()),
        );
        scripted.reply(
            ScriptedReply::new()
                .text(&json!({ "tool": "none", "text": "Gas use fell this spring." }).to_string()),
        );
        let record = run_constrained(&bench(scripted.clone()), &prompt()).await;
        assert!(!record.made_ui_call());
        assert_eq!(record.text, "Gas use fell this spring.");
        assert_eq!(record.model_calls, 2);
        let feedback = scripted.requests()[1].messages.last().unwrap().to_string();
        assert!(feedback.contains("[get_usage result]"), "{feedback}");
    }

    #[test]
    fn the_constrained_schema_is_a_union_of_closed_calls() {
        let tools = vec![
            json!({ "name": "close_ui", "description": "Close.", "input_schema": { "type": "object", "properties": { "surface": { "type": "string", "description": "id" } }, "required": ["surface"], "additionalProperties": false } }),
        ];
        let schema = constrained_schema(&tools).unwrap();
        assert_eq!(
            schema,
            json!({ "anyOf": [
                { "type": "object", "properties": { "tool": { "const": "close_ui" }, "input": { "type": "object", "properties": { "surface": { "type": "string" } }, "required": ["surface"], "additionalProperties": false } }, "required": ["tool", "input"], "additionalProperties": false },
                { "type": "object", "properties": { "tool": { "const": "none" }, "text": { "type": "string" } }, "required": ["tool", "text"], "additionalProperties": false }
            ] })
        );
    }

    #[test]
    fn the_compact_inline_guide_carries_the_type_lines() {
        let show_ui = json!({ "description": "Types: ...\nText(text!)", "input_schema": { "type": "object" } });
        let compact = inline_guide(&show_ui, ToolProfile::Compact);
        assert!(compact.contains("{\"type\":\"object\"}\nTypes: ...\nText(text!)\n"));
        assert!(!inline_guide(&show_ui, ToolProfile::Full).contains("Text(text!)"));
    }

    #[tokio::test]
    async fn inline_runs_without_a_block_show_nothing() {
        let scripted = Arc::new(ScriptedProvider::new());
        scripted.reply(ScriptedReply::new().text("kWh means kilowatt hour."));
        let record = run_inline(&bench(scripted), &prompt()).await;
        assert!(!record.made_ui_call());
        assert!(!record.opened);
        assert_eq!(record.model_calls, 1);
    }
}
