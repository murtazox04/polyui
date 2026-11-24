use crate::provider::{
    ErrorKind, ModelEvent, ModelProvider, ModelRequest, ProviderError, StopReason, Usage,
};
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptedReply {
    blocks: Vec<Value>,
    raw_inputs: Vec<(String, String)>,
    stop_reason: Option<StopReason>,
    error: Option<ProviderError>,
    pace: Option<Pace>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pace {
    bytes: usize,
    every: Duration,
}

fn pieces(text: &str, bytes: Option<usize>) -> Vec<String> {
    let Some(bytes) = bytes.filter(|bytes| *bytes > 0) else {
        return vec![text.to_string()];
    };
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut cut = bytes.min(rest.len());
        while !rest.is_char_boundary(cut) {
            cut += 1;
        }
        let (head, tail) = rest.split_at(cut);
        out.push(head.to_string());
        rest = tail;
    }
    out
}

impl ScriptedReply {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(mut self, text: &str) -> Self {
        self.blocks.push(json!({ "type": "text", "text": text }));
        self
    }

    pub fn thinking(mut self, signature: &str) -> Self {
        self.blocks
            .push(json!({ "type": "thinking", "thinking": "", "signature": signature }));
        self
    }

    pub fn tool(mut self, id: &str, name: &str, input: Value) -> Self {
        self.blocks
            .push(json!({ "type": "tool_use", "id": id, "name": name, "input": input }));
        self
    }

    pub fn broken_tool(mut self, id: &str, name: &str, raw: &str) -> Self {
        self.blocks
            .push(json!({ "type": "tool_use", "id": id, "name": name, "input": {} }));
        self.raw_inputs.push((id.to_string(), raw.to_string()));
        self
    }

    pub fn stop(mut self, reason: StopReason) -> Self {
        self.stop_reason = Some(reason);
        self
    }

    pub fn error(mut self, error: ProviderError) -> Self {
        self.error = Some(error);
        self
    }

    pub fn paced(mut self, bytes: usize, every: Duration) -> Self {
        self.pace = Some(Pace { bytes, every });
        self
    }

    fn events(self) -> Vec<ModelEvent> {
        let mut events = Vec::new();
        let mut has_tool = false;
        let bytes = self.pace.map(|pace| pace.bytes);
        for block in &self.blocks {
            match block["type"].as_str() {
                Some("text") => events.extend(
                    pieces(block["text"].as_str().unwrap_or_default(), bytes)
                        .into_iter()
                        .map(ModelEvent::TextDelta),
                ),
                Some("tool_use") => {
                    has_tool = true;
                    let id = block["id"].as_str().unwrap_or_default().to_string();
                    let name = block["name"].as_str().unwrap_or_default().to_string();
                    let raw = self
                        .raw_inputs
                        .iter()
                        .find(|(candidate, _)| *candidate == id)
                        .map(|(_, raw)| raw.clone());
                    let text = raw.clone().unwrap_or_else(|| block["input"].to_string());
                    events.push(ModelEvent::ToolCallStarted {
                        id: id.clone(),
                        name: name.clone(),
                    });
                    events.extend(pieces(&text, bytes).into_iter().map(|partial_json| {
                        ModelEvent::ToolInputDelta {
                            id: id.clone(),
                            partial_json,
                        }
                    }));
                    events.push(ModelEvent::ToolCallFinished {
                        id,
                        name,
                        input: match raw {
                            Some(raw) => Err(raw),
                            None => Ok(block["input"].clone()),
                        },
                    });
                }
                _ => {}
            }
        }
        if let Some(error) = self.error {
            events.push(ModelEvent::Error(error));
            return events;
        }
        let stop_reason = self.stop_reason.unwrap_or(if has_tool {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        });
        events.push(ModelEvent::MessageFinished {
            stop_reason,
            content: self.blocks,
            usage: Usage::default(),
        });
        events
    }
}

#[derive(Debug, Default)]
pub struct ScriptedProvider {
    replies: Mutex<VecDeque<ScriptedReply>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl ScriptedProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reply(&self, reply: ScriptedReply) -> &Self {
        if let Ok(mut replies) = self.replies.lock() {
            replies.push_back(reply);
        }
        self
    }

    pub fn requests(&self) -> Vec<ModelRequest> {
        self.requests
            .lock()
            .map(|requests| requests.clone())
            .unwrap_or_default()
    }

    pub fn remaining(&self) -> usize {
        self.replies
            .lock()
            .map(|replies| replies.len())
            .unwrap_or(0)
    }
}

impl ModelProvider for ScriptedProvider {
    fn stream(&self, request: ModelRequest) -> BoxStream<'static, ModelEvent> {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request);
        }
        let next = self
            .replies
            .lock()
            .ok()
            .and_then(|mut replies| replies.pop_front());
        let Some(reply) = next else {
            return stream::iter([ModelEvent::Error(ProviderError::new(
                ErrorKind::Other,
                "the script has no more replies",
            ))])
            .boxed();
        };
        let pace = reply.pace;
        let events = stream::iter(reply.events());
        match pace {
            Some(pace) => events
                .then(move |event| async move {
                    tokio::time::sleep(pace.every).await;
                    event
                })
                .boxed(),
            None => events.boxed(),
        }
    }
}
