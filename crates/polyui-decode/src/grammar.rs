use crate::format::{CallFormat, Layout, lark_string, regex_literal};
use crate::order::schema_order;
use crate::schema::{Change, SchemaError, prepare};
use polyui_core::tools::{self, ToolOptions, ToolProfile};
use polyui_core::{ActionRegistry, CatalogSet};
use serde_json::{Map, Value, json};
use std::fmt;
use std::fmt::Write;

pub const UI_TOOLS: [&str; 3] = ["show_ui", "update_ui", "close_ui"];

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub definition: Value,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DecodeError {
    NotAToolList,
    MissingField { index: usize, field: &'static str },
    Schema { tool: String, error: SchemaError },
    NoTools,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAToolList => write!(f, "expected a JSON array of tool definitions"),
            Self::MissingField { index, field } => {
                write!(f, "tool {index} has no \"{field}\"")
            }
            Self::Schema { tool, error } => write!(f, "tool {tool}: {error}"),
            Self::NoTools => write!(f, "no tools left to put in the grammar"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Text(String),
    Call { name: String, arguments: Value },
    Invalid(String),
}

const GRAMMAR_TOOLS: ToolOptions = ToolOptions {
    strict: false,
    profile: ToolProfile::Full,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolGrammar {
    tools: Vec<ToolSchema>,
    changes: Vec<(String, Change)>,
    format: CallFormat,
    layout: Layout,
}

impl ToolGrammar {
    pub fn new(catalogs: &CatalogSet, actions: &dyn ActionRegistry) -> Result<Self, DecodeError> {
        Self::from_tools(&tools::tool_definitions(catalogs, actions, GRAMMAR_TOOLS))
    }

    pub fn for_client(
        catalogs: &CatalogSet,
        declared: &[String],
        actions: &dyn ActionRegistry,
    ) -> Result<Self, DecodeError> {
        Self::from_tools(&tools::for_client(
            catalogs,
            declared,
            actions,
            GRAMMAR_TOOLS,
        ))
    }

    pub fn from_tools(definitions: &Value) -> Result<Self, DecodeError> {
        let list = definitions.as_array().ok_or(DecodeError::NotAToolList)?;
        let mut tools = Vec::with_capacity(list.len());
        let mut changes = Vec::new();
        for (index, definition) in list.iter().enumerate() {
            let name = definition.get("name").and_then(Value::as_str).ok_or(
                DecodeError::MissingField {
                    index,
                    field: "name",
                },
            )?;
            let input = definition
                .get("input_schema")
                .or_else(|| definition.get("parameters"))
                .ok_or(DecodeError::MissingField {
                    index,
                    field: "input_schema",
                })?;
            let prepared = prepare(input).map_err(|error| DecodeError::Schema {
                tool: name.to_string(),
                error,
            })?;
            changes.extend(
                prepared
                    .changes
                    .into_iter()
                    .map(|change| (name.to_string(), change)),
            );
            tools.push(ToolSchema {
                name: name.to_string(),
                description: definition
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                definition: input.clone(),
                input_schema: prepared.schema,
            });
        }
        if tools.is_empty() {
            return Err(DecodeError::NoTools);
        }
        Ok(Self {
            tools,
            changes,
            format: CallFormat::default(),
            layout: Layout::default(),
        })
    }

    pub fn only(mut self, names: &[&str]) -> Result<Self, DecodeError> {
        self.tools
            .retain(|tool| names.contains(&tool.name.as_str()));
        self.changes
            .retain(|(tool, _)| names.contains(&tool.as_str()));
        if self.tools.is_empty() {
            return Err(DecodeError::NoTools);
        }
        Ok(self)
    }

    pub fn with_format(mut self, format: CallFormat) -> Self {
        self.format = format;
        self
    }

    pub fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    pub fn tools(&self) -> &[ToolSchema] {
        &self.tools
    }

    pub fn format(&self) -> &CallFormat {
        &self.format
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn changes(&self) -> &[(String, Change)] {
        &self.changes
    }

    pub fn tool(&self, name: &str) -> Option<&ToolSchema> {
        self.tools.iter().find(|tool| tool.name == name)
    }

    pub fn input_schema(&self, name: &str) -> Option<&Value> {
        self.tool(name).map(|tool| &tool.input_schema)
    }

    pub fn call_schema(&self, name: &str) -> Option<Value> {
        self.tool(name).map(|tool| self.wrap(tool))
    }

    fn wrap(&self, tool: &ToolSchema) -> Value {
        let mut properties = Map::new();
        properties.insert(self.format.name_key.clone(), json!({ "const": tool.name }));
        properties.insert(self.format.arguments_key.clone(), tool.input_schema.clone());
        json!({
            "x-guidance": self.layout.x_guidance(),
            "type": "object",
            "properties": properties,
            "required": [self.format.name_key, self.format.arguments_key],
            "additionalProperties": false
        })
    }

    pub fn lark(&self) -> String {
        let format = &self.format;
        let mut grammar = String::from("%llguidance {}\n\n");
        if format.allow_text {
            grammar.push_str("start: TEXT | call\n");
        } else {
            grammar.push_str("start: call\n");
        }
        let mut call = vec![format.open.lark()];
        if !format.before.is_empty() {
            call.push(lark_string(&format.before));
        }
        call.push("tool".into());
        if !format.after.is_empty() {
            call.push(lark_string(&format.after));
        }
        if let Some(close) = &format.close {
            call.push(close.lark());
        }
        let _ = writeln!(grammar, "call: {}", call.join(" "));
        let rules: Vec<String> = (0..self.tools.len())
            .map(|index| format!("tool_{index}"))
            .collect();
        let _ = writeln!(grammar, "tool: {}", rules.join(" | "));
        for (index, tool) in self.tools.iter().enumerate() {
            let _ = writeln!(grammar, "tool_{index}: %json {}", self.wrap(tool));
        }
        if format.allow_text {
            let _ = writeln!(
                grammar,
                "TEXT: /(?s:.+)/ & ~/(?s:.*){}(?s:.*)/",
                regex_literal(format.open.spelling())
            );
        }
        grammar
    }

    pub fn schema_order(&self, name: &str, arguments: &Value) -> Value {
        match self.input_schema(name) {
            Some(schema) => schema_order(arguments, schema),
            None => arguments.clone(),
        }
    }

    pub fn render_call(&self, name: &str, arguments: &Value) -> String {
        let mut call = Map::new();
        call.insert(self.format.name_key.clone(), Value::from(name));
        call.insert(
            self.format.arguments_key.clone(),
            self.schema_order(name, arguments),
        );
        let mut text = String::new();
        text.push_str(self.format.open.spelling());
        text.push_str(&self.format.before);
        text.push_str(&render(&Value::Object(call), self.layout));
        text.push_str(&self.format.after);
        if let Some(close) = &self.format.close {
            text.push_str(close.spelling());
        }
        text
    }

    pub fn parse_reply(&self, text: &str) -> Reply {
        let open = self.format.open.spelling();
        let Some(start) = text.find(open) else {
            return Reply::Text(text.trim().to_string());
        };
        let mut body = &text[start + open.len()..];
        if let Some(close) = &self.format.close
            && let Some(end) = body.rfind(close.spelling())
        {
            body = &body[..end];
        }
        let mut body = body.trim();
        let before = self.format.before.trim();
        let after = self.format.after.trim();
        if !before.is_empty() {
            body = body.strip_prefix(before).unwrap_or(body).trim();
        }
        if !after.is_empty() {
            body = body.strip_suffix(after).unwrap_or(body).trim();
        }
        let call: Value = match serde_json::from_str(body) {
            Ok(call) => call,
            Err(error) => return Reply::Invalid(format!("the tool call is not JSON: {error}")),
        };
        let Some(name) = call.get(&self.format.name_key).and_then(Value::as_str) else {
            return Reply::Invalid(format!("the tool call has no \"{}\"", self.format.name_key));
        };
        if self.tool(name).is_none() {
            return Reply::Invalid(format!("unknown tool \"{name}\""));
        }
        let arguments = call
            .get(&self.format.arguments_key)
            .cloned()
            .unwrap_or_else(|| Value::Object(Map::new()));
        Reply::Call {
            name: name.to_string(),
            arguments,
        }
    }

    pub fn tools_prompt(&self) -> String {
        let format = &self.format;
        let mut prompt = String::from(
            "You can call one tool in a reply. These are the tools, as JSON Schema:\n<tools>\n",
        );
        for tool in &self.tools {
            let entry = json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.definition
                }
            });
            prompt.push_str(&render(&entry, Layout::Spaced));
            prompt.push('\n');
        }
        prompt.push_str("</tools>\nTo call a tool, reply with nothing but:\n");
        prompt.push_str(format.open.spelling());
        prompt.push_str(&format.before);
        let _ = write!(
            prompt,
            "{{\"{}\": <tool name>, \"{}\": <arguments object>}}",
            format.name_key, format.arguments_key
        );
        prompt.push_str(&format.after);
        if let Some(close) = &format.close {
            prompt.push_str(close.spelling());
        }
        prompt.push_str("\nWrite object keys in the order the schema lists them.");
        if format.allow_text {
            prompt.push_str(" To answer without a tool, reply with plain text.");
        }
        prompt
    }

    pub fn json_prompt(&self, name: &str) -> Option<String> {
        let tool = self.tool(name)?;
        Some(format!(
            "Reply with nothing but the JSON arguments of the {} tool. {}\nThe arguments follow this JSON Schema, with object keys in the order it lists them:\n{}",
            tool.name,
            tool.description,
            render(&tool.definition, Layout::Spaced)
        ))
    }
}

pub fn render(value: &Value, layout: Layout) -> String {
    let (item, key) = match layout {
        Layout::Compact => (",", ":"),
        Layout::Spaced | Layout::Flexible => (", ", ": "),
    };
    let mut out = String::new();
    write_value(value, item, key, &mut out);
    out
}

fn write_value(value: &Value, item: &str, key: &str, out: &mut String) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (index, entry) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(item);
                }
                write_value(entry, item, key, out);
            }
            out.push(']');
        }
        Value::Object(fields) => {
            out.push('{');
            for (index, (name, entry)) in fields.iter().enumerate() {
                if index > 0 {
                    out.push_str(item);
                }
                out.push_str(&Value::from(name.as_str()).to_string());
                out.push_str(key);
                write_value(entry, item, key, out);
            }
            out.push('}');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}
