use crate::Value;
use std::fmt;
#[cfg(not(feature = "client-lite"))]
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_message_bytes: usize,
    pub max_depth: usize,
    pub max_array_len: usize,
    pub max_string_chars: usize,
    pub max_data_bytes: usize,
    pub max_components: usize,
    pub max_tree_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_message_bytes: 256 * 1024,
            max_depth: 32,
            max_array_len: 1000,
            max_string_chars: 4096,
            max_data_bytes: 128 * 1024,
            max_components: 256,
            max_tree_depth: 12,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitError {
    MessageTooLarge { size: usize, limit: usize },
    TooDeep { limit: usize },
    ArrayTooLong { limit: usize },
    StringTooLong { limit: usize },
    DataTooLarge { size: usize, limit: usize },
}

impl fmt::Display for LimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageTooLarge { size, limit } => {
                write!(f, "message is {size} bytes, max is {limit}")
            }
            Self::TooDeep { limit } => write!(f, "JSON nesting is deeper than {limit}"),
            Self::ArrayTooLong { limit } => write!(f, "an array has more than {limit} items"),
            Self::StringTooLong { limit } => {
                write!(f, "a string is longer than {limit} characters")
            }
            Self::DataTooLarge { size, limit } => {
                write!(f, "data model is {size} bytes, max is {limit}")
            }
        }
    }
}

impl std::error::Error for LimitError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Container {
    Object,
    Array,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Token {
    Nothing,
    Open(Container),
    Close(Container),
    StringStart,
    StringEnd,
    ValueStart,
    Colon,
    Comma,
    Unbalanced,
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    container: Container,
    items: usize,
    awaiting_value: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Scanner {
    limits: Limits,
    string_depth: usize,
    stack: Vec<Frame>,
    in_string: bool,
    escaped: bool,
    hex_left: u8,
    hex_value: u32,
    string_chars: usize,
}

impl Scanner {
    pub(crate) fn new(limits: Limits) -> Self {
        Self {
            limits,
            string_depth: 2,
            stack: Vec::new(),
            in_string: false,
            escaped: false,
            hex_left: 0,
            hex_value: 0,
            string_chars: 0,
        }
    }

    #[cfg(feature = "stream")]
    pub(crate) fn capping_every_string(limits: Limits) -> Self {
        Self {
            string_depth: 1,
            ..Self::new(limits)
        }
    }

    #[cfg(feature = "stream")]
    pub(crate) fn depth(&self) -> usize {
        self.stack.len()
    }

    #[cfg(feature = "stream")]
    pub(crate) fn top(&self) -> Option<Container> {
        self.stack.last().map(|frame| frame.container)
    }

    pub(crate) fn push(&mut self, byte: u8) -> Result<Token, LimitError> {
        if self.in_string {
            return self.push_in_string(byte);
        }
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' => Ok(Token::Nothing),
            b'{' | b'[' => {
                self.count_value_start()?;
                if self.stack.len() >= self.limits.max_depth {
                    return Err(LimitError::TooDeep {
                        limit: self.limits.max_depth,
                    });
                }
                let container = if byte == b'{' {
                    Container::Object
                } else {
                    Container::Array
                };
                self.stack.push(Frame {
                    container,
                    items: 0,
                    awaiting_value: container == Container::Array,
                });
                Ok(Token::Open(container))
            }
            b'}' | b']' => {
                let expected = if byte == b'}' {
                    Container::Object
                } else {
                    Container::Array
                };
                match self.stack.pop() {
                    Some(frame) if frame.container == expected => Ok(Token::Close(expected)),
                    _ => Ok(Token::Unbalanced),
                }
            }
            b'"' => {
                self.count_value_start()?;
                self.in_string = true;
                self.escaped = false;
                self.hex_left = 0;
                self.string_chars = 0;
                Ok(Token::StringStart)
            }
            b':' => Ok(Token::Colon),
            b',' => {
                if let Some(frame) = self.stack.last_mut()
                    && frame.container == Container::Array
                {
                    frame.awaiting_value = true;
                }
                Ok(Token::Comma)
            }
            _ => {
                let starts = self.stack.last().is_some_and(|frame| {
                    frame.container == Container::Array && frame.awaiting_value
                });
                self.count_value_start()?;
                Ok(if starts {
                    Token::ValueStart
                } else {
                    Token::Nothing
                })
            }
        }
    }

    fn count_value_start(&mut self) -> Result<(), LimitError> {
        let limit = self.limits.max_array_len;
        if let Some(frame) = self.stack.last_mut()
            && frame.container == Container::Array
            && frame.awaiting_value
        {
            frame.awaiting_value = false;
            frame.items += 1;
            if frame.items > limit {
                return Err(LimitError::ArrayTooLong { limit });
            }
        }
        Ok(())
    }

    fn push_in_string(&mut self, byte: u8) -> Result<Token, LimitError> {
        if self.hex_left > 0 {
            self.hex_left -= 1;
            self.hex_value = self.hex_value * 16 + char::from(byte).to_digit(16).unwrap_or(0);
            let low_surrogate = (0xDC00..=0xDFFF).contains(&self.hex_value);
            if self.hex_left == 0 && !low_surrogate {
                return self.count_char();
            }
            return Ok(Token::Nothing);
        }
        if self.escaped {
            self.escaped = false;
            if byte == b'u' {
                self.hex_left = 4;
                self.hex_value = 0;
                return Ok(Token::Nothing);
            }
            return self.count_char();
        }
        match byte {
            b'\\' => {
                self.escaped = true;
                Ok(Token::Nothing)
            }
            b'"' => {
                self.in_string = false;
                Ok(Token::StringEnd)
            }
            continuation if continuation & 0xC0 == 0x80 => Ok(Token::Nothing),
            _ => self.count_char(),
        }
    }

    fn count_char(&mut self) -> Result<Token, LimitError> {
        self.string_chars += 1;
        if self.stack.len() >= self.string_depth && self.string_chars > self.limits.max_string_chars
        {
            return Err(LimitError::StringTooLong {
                limit: self.limits.max_string_chars,
            });
        }
        Ok(Token::Nothing)
    }
}

pub fn prescan(input: &str, limits: &Limits) -> Result<(), LimitError> {
    if input.len() > limits.max_message_bytes {
        return Err(LimitError::MessageTooLarge {
            size: input.len(),
            limit: limits.max_message_bytes,
        });
    }
    let mut scanner = Scanner::new(*limits);
    for byte in input.bytes() {
        scanner.push(byte)?;
    }
    Ok(())
}

pub fn check_value(value: &Value, limits: &Limits) -> Result<(), LimitError> {
    walk(value, 0, limits)?;
    let size = json_size(value);
    if size > limits.max_message_bytes {
        return Err(LimitError::MessageTooLarge {
            size,
            limit: limits.max_message_bytes,
        });
    }
    Ok(())
}

pub fn check_data_model(data: &Value, limits: &Limits) -> Result<(), LimitError> {
    walk(data, 1, limits)?;
    let size = json_size(data);
    if size > limits.max_data_bytes {
        return Err(LimitError::DataTooLarge {
            size,
            limit: limits.max_data_bytes,
        });
    }
    Ok(())
}

pub(crate) fn walk(value: &Value, enclosing: usize, limits: &Limits) -> Result<(), LimitError> {
    let check_string = |text: &str| {
        if enclosing >= 2 && text.chars().count() > limits.max_string_chars {
            Err(LimitError::StringTooLong {
                limit: limits.max_string_chars,
            })
        } else {
            Ok(())
        }
    };
    match value {
        Value::String(text) => check_string(text),
        Value::Array(items) => {
            if enclosing + 1 > limits.max_depth {
                return Err(LimitError::TooDeep {
                    limit: limits.max_depth,
                });
            }
            if items.len() > limits.max_array_len {
                return Err(LimitError::ArrayTooLong {
                    limit: limits.max_array_len,
                });
            }
            items
                .iter()
                .try_for_each(|item| walk(item, enclosing + 1, limits))
        }
        Value::Object(map) => {
            if enclosing + 1 > limits.max_depth {
                return Err(LimitError::TooDeep {
                    limit: limits.max_depth,
                });
            }
            map.iter().try_for_each(|(key, item)| {
                check_key(key, enclosing + 1, limits)?;
                walk(item, enclosing + 1, limits)
            })
        }
        _ => Ok(()),
    }
}

pub(crate) fn check_key(key: &str, enclosing: usize, limits: &Limits) -> Result<(), LimitError> {
    if enclosing >= 2 && key.chars().count() > limits.max_string_chars {
        return Err(LimitError::StringTooLong {
            limit: limits.max_string_chars,
        });
    }
    Ok(())
}

#[cfg(not(feature = "client-lite"))]
struct ByteCounter(usize);

#[cfg(not(feature = "client-lite"))]
impl io::Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(feature = "client-lite")]
pub fn json_size(value: &Value) -> usize {
    crate::json::to_string(value).len()
}

#[cfg(not(feature = "client-lite"))]
pub fn json_size(value: &Value) -> usize {
    let mut counter = ByteCounter(0);
    match serde_json::to_writer(&mut counter, value) {
        Ok(()) => counter.0,
        Err(_) => usize::MAX,
    }
}

pub fn json_depth(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(json_depth).max().unwrap_or(0),
        Value::Object(map) => 1 + map.values().map(json_depth).max().unwrap_or(0),
        _ => 0,
    }
}

#[cfg(all(test, not(feature = "client-lite")))]
mod tests {
    use super::*;
    use serde_json::json;

    fn nested(depth: usize) -> String {
        let mut text = String::new();
        for _ in 0..depth {
            text.push('[');
        }
        for _ in 0..depth {
            text.push(']');
        }
        text
    }

    #[test]
    fn depth_32_passes_and_33_fails() {
        let limits = Limits::default();
        assert!(prescan(&nested(32), &limits).is_ok());
        assert_eq!(
            prescan(&nested(33), &limits),
            Err(LimitError::TooDeep { limit: 32 })
        );
    }

    #[test]
    fn array_length_is_counted() {
        let limits = Limits::default();
        let ok = format!("{{\"a\":[{}]}}", vec!["1"; 1000].join(","));
        let long = format!("{{\"a\":[{}]}}", vec!["1"; 1001].join(","));
        assert!(prescan(&ok, &limits).is_ok());
        assert_eq!(
            prescan(&long, &limits),
            Err(LimitError::ArrayTooLong { limit: 1000 })
        );
        assert!(prescan("{\"a\":[]}", &limits).is_ok());
        let nested_arrays = format!("{{\"a\":[{}]}}", vec!["[]"; 1001].join(","));
        assert!(prescan(&nested_arrays, &limits).is_err());
    }

    #[test]
    fn nested_strings_are_limited_but_top_level_is_not() {
        let limits = Limits::default();
        let big = "x".repeat(5000);
        assert!(prescan(&format!("{{\"fallback\":\"{big}\"}}"), &limits).is_ok());
        assert_eq!(
            prescan(&format!("{{\"a\":{{\"b\":\"{big}\"}}}}"), &limits),
            Err(LimitError::StringTooLong { limit: 4096 })
        );
        let escaped = "\\u0041".repeat(4096);
        assert!(prescan(&format!("{{\"a\":[\"{escaped}\"]}}"), &limits).is_ok());
        let cyrillic = "ж".repeat(4000);
        assert!(prescan(&format!("{{\"a\":[\"{cyrillic}\"]}}"), &limits).is_ok());
    }

    #[test]
    fn strings_are_counted_in_characters_not_bytes_or_escapes() {
        let limits = Limits::default();
        let wrap = |text: &str| format!("{{\"a\":{{\"b\":\"{text}\"}}}}");
        let check = |text: &str| {
            let parsed: Value = serde_json::from_str(&wrap(text)).unwrap();
            (prescan(&wrap(text), &limits), check_value(&parsed, &limits))
        };
        let fits = Ok(());
        let long = Err(LimitError::StringTooLong { limit: 4096 });
        for (letter, bytes) in [("ж", 2), ("o\u{2bb}", 3), ("\u{1F600}", 4)] {
            let at_limit = letter.repeat(4096 / letter.chars().count());
            assert_eq!(at_limit.len(), 4096 * bytes / letter.chars().count());
            assert_eq!(check(&at_limit), (fits.clone(), fits.clone()), "{letter}");
            let over = format!("{at_limit}x");
            assert_eq!(check(&over), (long.clone(), long.clone()), "{letter}");
        }
        let escaped_emoji = "\\ud83d\\ude00".repeat(4096);
        assert_eq!(check(&escaped_emoji), (fits.clone(), fits.clone()));
        let one_more = format!("{escaped_emoji}\\u0041");
        assert_eq!(check(&one_more), (long.clone(), long));
    }

    #[test]
    fn message_size_is_checked_first() {
        let limits = Limits::default();
        let text = " ".repeat(256 * 1024 + 1);
        assert!(matches!(
            prescan(&text, &limits),
            Err(LimitError::MessageTooLarge { .. })
        ));
    }

    #[test]
    fn check_value_mirrors_prescan() {
        let limits = Limits::default();
        let mut deep = json!(1);
        for _ in 0..33 {
            deep = json!([deep]);
        }
        assert_eq!(
            check_value(&deep, &limits),
            Err(LimitError::TooDeep { limit: 32 })
        );
        let long: Vec<u32> = (0..1001).collect();
        assert!(check_value(&json!({ "a": long }), &limits).is_err());
        assert!(check_value(&json!({ "a": { "b": "x".repeat(4097) } }), &limits).is_err());
        assert!(check_value(&json!({ "a": "x".repeat(4097) }), &limits).is_ok());
        assert_eq!(json_depth(&json!({ "a": [1] })), 2);
        assert_eq!(json_size(&json!({ "a": 1 })), 7);
    }

    #[test]
    fn garbage_does_not_panic() {
        let limits = Limits::default();
        for input in [
            "]]]]",
            "\"\\",
            "\"\\u12",
            "{\"a\":\"\u{1F600}",
            "\u{0}\u{1}",
        ] {
            let _ = prescan(input, &limits);
        }
    }
}
