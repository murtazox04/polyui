use crate::provider::{
    ErrorKind, ModelEvent, ModelProvider, ModelRequest, ProviderError, StopReason, Usage,
};
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptedReply {
    blocks: Vec<Value>,
    raw_inputs: Vec<(String, String)>,
    stop_reason: Option<StopReason>,
    error: Option<ProviderError>,
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

    fn events(self) -> Vec<ModelEvent> {
        let mut events = Vec::new();
        let mut has_tool = false;
        for block in &self.blocks {
            match block["type"].as_str() {
                Some("text") => events.push(ModelEvent::TextDelta(
                    block["text"].as_str().unwrap_or_default().to_string(),
                )),
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
                    events.push(ModelEvent::ToolInputDelta {
                        id: id.clone(),
                        partial_json: text,
                    });
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
        let events = match next {
            Some(reply) => reply.events(),
            None => vec![ModelEvent::Error(ProviderError::new(
                ErrorKind::Other,
                "the script has no more replies",
            ))],
        };
        stream::iter(events).boxed()
    }
}
