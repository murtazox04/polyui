use crate::grammar::ToolGrammar;
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct Chat {
    pub model: Option<String>,
    pub messages: Vec<Value>,
    pub max_tokens: u32,
    pub temperature: Option<f64>,
}

impl Chat {
    pub fn new(messages: Vec<Value>) -> Self {
        Self {
            model: None,
            messages,
            max_tokens: 2048,
            temperature: None,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_system(mut self, text: &str) -> Self {
        match self.messages.first_mut() {
            Some(Value::Object(first)) if first.get("role") == Some(&json!("system")) => {
                let content = first
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let merged = if content.is_empty() {
                    text.to_string()
                } else {
                    format!("{content}\n\n{text}")
                };
                first.insert("content".into(), Value::from(merged));
            }
            _ => self
                .messages
                .insert(0, json!({ "role": "system", "content": text })),
        }
        self
    }

    pub fn with_tools_prompt(self, grammar: &ToolGrammar) -> Self {
        let prompt = grammar.tools_prompt();
        self.with_system(&prompt)
    }

    fn base(&self) -> Map<String, Value> {
        let mut body = Map::new();
        if let Some(model) = &self.model {
            body.insert("model".into(), Value::from(model.as_str()));
        }
        body.insert("messages".into(), Value::Array(self.messages.clone()));
        body.insert("max_tokens".into(), Value::from(self.max_tokens));
        if let Some(temperature) = self.temperature {
            body.insert("temperature".into(), Value::from(temperature));
        }
        body
    }
}

pub fn llama_completion(grammar: &ToolGrammar, prompt: &str, n_predict: u32) -> Value {
    json!({
        "prompt": prompt,
        "n_predict": n_predict,
        "grammar": grammar.lark()
    })
}

pub fn llama_chat_grammar(grammar: &ToolGrammar, chat: &Chat) -> Value {
    let mut body = chat.base();
    body.insert("grammar".into(), Value::from(grammar.lark()));
    Value::Object(body)
}

pub fn llama_chat_json(grammar: &ToolGrammar, tool: &str, chat: &Chat) -> Option<Value> {
    let schema = grammar.input_schema(tool)?.clone();
    let mut body = chat.base();
    body.insert(
        "response_format".into(),
        json!({
            "type": "json_schema",
            "json_schema": { "name": tool, "schema": schema }
        }),
    );
    Some(Value::Object(body))
}

pub fn vllm_chat_grammar(grammar: &ToolGrammar, chat: &Chat) -> Value {
    let mut body = chat.base();
    body.insert(
        "structured_outputs".into(),
        json!({ "grammar": grammar.lark() }),
    );
    Value::Object(body)
}

pub fn vllm_chat_json(grammar: &ToolGrammar, tool: &str, chat: &Chat) -> Option<Value> {
    let schema = grammar.input_schema(tool)?.clone();
    let mut body = chat.base();
    body.insert("structured_outputs".into(), json!({ "json": schema }));
    Some(Value::Object(body))
}
