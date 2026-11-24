use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Marker {
    Text(String),
    Token(String),
}

impl Marker {
    pub fn spelling(&self) -> &str {
        match self {
            Self::Text(text) | Self::Token(text) => text,
        }
    }

    pub(crate) fn lark(&self) -> String {
        match self {
            Self::Text(text) => lark_string(text),
            Self::Token(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFormat {
    pub open: Marker,
    pub before: String,
    pub after: String,
    pub close: Option<Marker>,
    pub name_key: String,
    pub arguments_key: String,
    pub allow_text: bool,
}

impl CallFormat {
    pub fn hermes() -> Self {
        Self {
            open: Marker::Text("<tool_call>".into()),
            before: "\n".into(),
            after: "\n".into(),
            close: Some(Marker::Text("</tool_call>".into())),
            name_key: "name".into(),
            arguments_key: "arguments".into(),
            allow_text: true,
        }
    }

    pub fn hermes_tokens() -> Self {
        Self {
            open: Marker::Token("<tool_call>".into()),
            close: Some(Marker::Token("</tool_call>".into())),
            ..Self::hermes()
        }
    }

    pub fn tool_only(self) -> Self {
        Self {
            allow_text: false,
            ..self
        }
    }
}

impl Default for CallFormat {
    fn default() -> Self {
        Self::hermes()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Layout {
    #[default]
    Spaced,
    Compact,
    Flexible,
}

impl Layout {
    pub fn x_guidance(self) -> Value {
        match self {
            Self::Spaced => json!({
                "whitespace_flexible": false,
                "item_separator": ", ",
                "key_separator": ": "
            }),
            Self::Compact => json!({
                "whitespace_flexible": false,
                "item_separator": ",",
                "key_separator": ":"
            }),
            Self::Flexible => json!({ "whitespace_flexible": true }),
        }
    }
}

pub(crate) fn lark_string(text: &str) -> String {
    Value::from(text).to_string()
}

pub(crate) fn regex_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for character in text.chars() {
        let plain = character.is_ascii_alphanumeric()
            || !character.is_ascii()
            || matches!(character, '<' | '>' | '_' | ' ');
        if character == '\n' {
            out.push_str("\\n");
        } else if character == '\t' {
            out.push_str("\\t");
        } else if character == '\r' {
            out.push_str("\\r");
        } else if plain {
            out.push(character);
        } else {
            out.push('\\');
            out.push(character);
        }
    }
    out
}
