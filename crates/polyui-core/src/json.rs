#[cfg(not(feature = "client-lite"))]
use serde_json::Number;

#[cfg(feature = "client-lite")]
use crate::value::Number;
use crate::{Map, Value};
use std::fmt::Write;

const RECURSION_LIMIT: u8 = 128;

const EOF_WHILE_PARSING_LIST: &str = "EOF while parsing a list";
const EOF_WHILE_PARSING_OBJECT: &str = "EOF while parsing an object";
const EOF_WHILE_PARSING_STRING: &str = "EOF while parsing a string";
const EOF_WHILE_PARSING_VALUE: &str = "EOF while parsing a value";
const EXPECTED_COLON: &str = "expected `:`";
const EXPECTED_LIST_COMMA_OR_END: &str = "expected `,` or `]`";
const EXPECTED_OBJECT_COMMA_OR_END: &str = "expected `,` or `}`";
const EXPECTED_SOME_IDENT: &str = "expected ident";
const EXPECTED_SOME_VALUE: &str = "expected value";
const INVALID_ESCAPE: &str = "invalid escape";
const INVALID_NUMBER: &str = "invalid number";
const NUMBER_OUT_OF_RANGE: &str = "number out of range";
const CONTROL_CHARACTER: &str = "control character (\\u0000-\\u001F) found while parsing a string";
const KEY_MUST_BE_A_STRING: &str = "key must be a string";
const LONE_LEADING_SURROGATE: &str = "lone leading surrogate in hex escape";
const TRAILING_COMMA: &str = "trailing comma";
const TRAILING_CHARACTERS: &str = "trailing characters";
const UNEXPECTED_END_OF_HEX_ESCAPE: &str = "unexpected end of hex escape";
const RECURSION_LIMIT_EXCEEDED: &str = "recursion limit exceeded";

struct Syntax {
    message: &'static str,
    at: usize,
}

impl Syntax {
    fn describe(&self, bytes: &[u8]) -> String {
        let before = &bytes[..self.at];
        let start_of_line = before
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map_or(0, |position| position + 1);
        let line = 1 + before[..start_of_line]
            .iter()
            .filter(|&&byte| byte == b'\n')
            .count();
        format!(
            "{} at line {line} column {}",
            self.message,
            self.at - start_of_line
        )
    }
}

type Parsed<T> = Result<T, Syntax>;

const MAX_POWER_OF_TEN: u32 = 308;

fn power_of_ten(exponent: u32) -> Option<f64> {
    if exponent > MAX_POWER_OF_TEN {
        return None;
    }
    let mut limbs: Vec<u32> = vec![1];
    for _ in 0..exponent {
        let mut carry = 0u64;
        for limb in &mut limbs {
            let product = u64::from(*limb) * 10 + carry;
            *limb = product as u32;
            carry = product >> 32;
        }
        if carry > 0 {
            limbs.push(carry as u32);
        }
    }
    let top_limb = limbs.len() - 1;
    let bits = top_limb as u32 * 32 + (32 - limbs[top_limb].leading_zeros());
    if bits <= 64 {
        let value = limbs
            .iter()
            .rev()
            .fold(0u64, |value, &limb| (value << 32) | u64::from(limb));
        return Some(value as f64);
    }
    let shift = bits - 64;
    let bit = |position: u32| (limbs[(position / 32) as usize] >> (position % 32)) & 1 == 1;
    let top = (0..64).rev().fold(0u64, |value, offset| {
        (value << 1) | u64::from(bit(shift + offset))
    });
    let sticky = (0..shift).any(bit);
    let mut mantissa = top >> 11;
    let rest = top & 0x7ff;
    if rest > 0x400 || (rest == 0x400 && (sticky || mantissa & 1 == 1)) {
        mantissa += 1;
    }
    let mut biased = u64::from(bits - 1) + 1023;
    if mantissa == 1 << 53 {
        mantissa >>= 1;
        biased += 1;
    }
    Some(f64::from_bits(
        (biased << 52) | (mantissa & ((1 << 52) - 1)),
    ))
}

fn overflows(current: u64, digit: u64, max: u64) -> bool {
    current >= max / 10 && (current > max / 10 || digit > max % 10)
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    index: usize,
    depth: u8,
}

impl Parser<'_> {
    fn error(&self, message: &'static str) -> Syntax {
        Syntax {
            message,
            at: self.index,
        }
    }

    fn peek_error(&self, message: &'static str) -> Syntax {
        Syntax {
            message,
            at: self.bytes.len().min(self.index + 1),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn peek_or_null(&self) -> u8 {
        self.peek().unwrap_or(0)
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.index += 1;
        Some(byte)
    }

    fn whitespace(&mut self) -> Option<u8> {
        while let Some(b' ' | b'\n' | b'\t' | b'\r') = self.peek() {
            self.index += 1;
        }
        self.peek()
    }

    fn document(&mut self) -> Parsed<Value> {
        let value = self.value()?;
        match self.whitespace() {
            Some(_) => Err(self.peek_error(TRAILING_CHARACTERS)),
            None => Ok(value),
        }
    }

    fn value(&mut self) -> Parsed<Value> {
        let Some(peek) = self.whitespace() else {
            return Err(self.peek_error(EOF_WHILE_PARSING_VALUE));
        };
        match peek {
            b'n' => self.ident(b"ull").map(|()| Value::Null),
            b't' => self.ident(b"rue").map(|()| Value::Bool(true)),
            b'f' => self.ident(b"alse").map(|()| Value::Bool(false)),
            b'-' => {
                self.index += 1;
                self.integer(false)
            }
            b'0'..=b'9' => self.integer(true),
            b'"' => {
                self.index += 1;
                self.string().map(Value::String)
            }
            b'[' => self.nested(Self::array).map(Value::Array),
            b'{' => self.nested(Self::object).map(Value::Object),
            _ => Err(self.peek_error(EXPECTED_SOME_VALUE)),
        }
    }

    fn nested<T>(&mut self, body: fn(&mut Self) -> Parsed<T>) -> Parsed<T> {
        self.depth -= 1;
        if self.depth == 0 {
            return Err(self.peek_error(RECURSION_LIMIT_EXCEEDED));
        }
        self.index += 1;
        let parsed = body(self);
        self.depth += 1;
        let parsed = parsed?;
        self.index += 1;
        Ok(parsed)
    }

    fn ident(&mut self, rest: &[u8]) -> Parsed<()> {
        self.index += 1;
        for &expected in rest {
            match self.next() {
                None => return Err(self.error(EOF_WHILE_PARSING_VALUE)),
                Some(found) if found != expected => return Err(self.error(EXPECTED_SOME_IDENT)),
                Some(_) => {}
            }
        }
        Ok(())
    }

    fn array(&mut self) -> Parsed<Vec<Value>> {
        let mut items = Vec::new();
        loop {
            let Some(peek) = self.whitespace() else {
                return Err(self.peek_error(EOF_WHILE_PARSING_LIST));
            };
            if peek == b']' {
                return Ok(items);
            }
            if !items.is_empty() {
                if peek != b',' {
                    return Err(self.peek_error(EXPECTED_LIST_COMMA_OR_END));
                }
                self.index += 1;
                match self.whitespace() {
                    Some(b']') => return Err(self.peek_error(TRAILING_COMMA)),
                    Some(_) => {}
                    None => return Err(self.peek_error(EOF_WHILE_PARSING_VALUE)),
                }
            }
            items.push(self.value()?);
        }
    }

    fn object(&mut self) -> Parsed<Map<String, Value>> {
        let mut map = Map::new();
        let mut first = true;
        loop {
            let Some(peek) = self.whitespace() else {
                return Err(self.peek_error(EOF_WHILE_PARSING_OBJECT));
            };
            if peek == b'}' {
                return Ok(map);
            }
            if first {
                if peek != b'"' {
                    return Err(self.peek_error(KEY_MUST_BE_A_STRING));
                }
                first = false;
            } else {
                if peek != b',' {
                    return Err(self.peek_error(EXPECTED_OBJECT_COMMA_OR_END));
                }
                self.index += 1;
                match self.whitespace() {
                    Some(b'"') => {}
                    Some(b'}') => return Err(self.peek_error(TRAILING_COMMA)),
                    Some(_) => return Err(self.peek_error(KEY_MUST_BE_A_STRING)),
                    None => return Err(self.peek_error(EOF_WHILE_PARSING_VALUE)),
                }
            }
            self.index += 1;
            let key = self.string()?;
            match self.whitespace() {
                Some(b':') => self.index += 1,
                Some(_) => return Err(self.peek_error(EXPECTED_COLON)),
                None => return Err(self.peek_error(EOF_WHILE_PARSING_OBJECT)),
            }
            let value = self.value()?;
            map.insert(key, value);
        }
    }

    fn string(&mut self) -> Parsed<String> {
        let mut out = String::new();
        let mut start = self.index;
        loop {
            while let Some(byte) = self.peek() {
                if byte == b'"' || byte == b'\\' || byte < 0x20 {
                    break;
                }
                self.index += 1;
            }
            match self.peek() {
                None => return Err(self.error(EOF_WHILE_PARSING_STRING)),
                Some(b'"') => {
                    out.push_str(&self.text[start..self.index]);
                    self.index += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    out.push_str(&self.text[start..self.index]);
                    self.index += 1;
                    self.escape(&mut out)?;
                    start = self.index;
                }
                Some(_) => {
                    self.index += 1;
                    return Err(self.error(CONTROL_CHARACTER));
                }
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Parsed<()> {
        let Some(byte) = self.next() else {
            return Err(self.error(EOF_WHILE_PARSING_STRING));
        };
        let decoded = match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => self.unicode_escape()?,
            _ => return Err(self.error(INVALID_ESCAPE)),
        };
        out.push(decoded);
        Ok(())
    }

    fn hex_escape(&mut self) -> Parsed<u32> {
        let Some(digits) = self.bytes.get(self.index..self.index + 4) else {
            self.index = self.bytes.len();
            return Err(self.error(EOF_WHILE_PARSING_STRING));
        };
        self.index += 4;
        digits
            .iter()
            .try_fold(0, |code, &digit| {
                char::from(digit)
                    .to_digit(16)
                    .map(|value| code * 16 + value)
            })
            .ok_or_else(|| self.error(INVALID_ESCAPE))
    }

    fn expect_escape_byte(&mut self, expected: u8) -> Parsed<()> {
        match self.peek() {
            None => Err(self.error(EOF_WHILE_PARSING_STRING)),
            Some(found) => {
                self.index += 1;
                if found == expected {
                    Ok(())
                } else {
                    Err(self.error(UNEXPECTED_END_OF_HEX_ESCAPE))
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Parsed<char> {
        let leading = self.hex_escape()?;
        if (0xDC00..=0xDFFF).contains(&leading) {
            return Err(self.error(LONE_LEADING_SURROGATE));
        }
        if !(0xD800..=0xDBFF).contains(&leading) {
            return char::from_u32(leading).ok_or_else(|| self.error(INVALID_ESCAPE));
        }
        self.expect_escape_byte(b'\\')?;
        self.expect_escape_byte(b'u')?;
        let trailing = self.hex_escape()?;
        if !(0xDC00..=0xDFFF).contains(&trailing) {
            return Err(self.error(LONE_LEADING_SURROGATE));
        }
        char::from_u32((((leading - 0xD800) << 10) | (trailing - 0xDC00)) + 0x1_0000)
            .ok_or_else(|| self.error(INVALID_ESCAPE))
    }

    fn integer(&mut self, positive: bool) -> Parsed<Value> {
        match self.next() {
            None => Err(self.error(EOF_WHILE_PARSING_VALUE)),
            Some(b'0') => {
                if self.peek_or_null().is_ascii_digit() {
                    Err(self.peek_error(INVALID_NUMBER))
                } else {
                    self.number(positive, 0)
                }
            }
            Some(first @ b'1'..=b'9') => {
                let mut significand = u64::from(first - b'0');
                while let digit @ b'0'..=b'9' = self.peek_or_null() {
                    let digit = u64::from(digit - b'0');
                    if overflows(significand, digit, u64::MAX) {
                        return self.long_integer(positive, significand).map(float);
                    }
                    self.index += 1;
                    significand = significand * 10 + digit;
                }
                self.number(positive, significand)
            }
            Some(_) => Err(self.error(INVALID_NUMBER)),
        }
    }

    fn number(&mut self, positive: bool, significand: u64) -> Parsed<Value> {
        match self.peek_or_null() {
            b'.' => self.decimal(positive, significand, 0).map(float),
            b'e' | b'E' => self.exponent(positive, significand, 0).map(float),
            _ if positive => Ok(Value::Number(Number::from(significand))),
            _ => {
                let negative = (significand as i64).wrapping_neg();
                if negative >= 0 {
                    Ok(float(-(significand as f64)))
                } else {
                    Ok(Value::Number(Number::from(negative)))
                }
            }
        }
    }

    fn decimal(&mut self, positive: bool, mut significand: u64, before_point: i32) -> Parsed<f64> {
        self.index += 1;
        let mut after_point = 0;
        while let digit @ b'0'..=b'9' = self.peek_or_null() {
            let digit = u64::from(digit - b'0');
            if overflows(significand, digit, u64::MAX) {
                return self.decimal_overflow(positive, significand, before_point + after_point);
            }
            self.index += 1;
            significand = significand * 10 + digit;
            after_point -= 1;
        }
        if after_point == 0 {
            return Err(match self.peek() {
                Some(_) => self.peek_error(INVALID_NUMBER),
                None => self.peek_error(EOF_WHILE_PARSING_VALUE),
            });
        }
        let exponent = before_point + after_point;
        match self.peek_or_null() {
            b'e' | b'E' => self.exponent(positive, significand, exponent),
            _ => self.compose(positive, significand, exponent),
        }
    }

    fn exponent(&mut self, positive: bool, significand: u64, starting: i32) -> Parsed<f64> {
        self.index += 1;
        let positive_exponent = match self.peek_or_null() {
            b'+' => {
                self.index += 1;
                true
            }
            b'-' => {
                self.index += 1;
                false
            }
            _ => true,
        };
        let mut exponent = match self.next() {
            None => return Err(self.error(EOF_WHILE_PARSING_VALUE)),
            Some(digit @ b'0'..=b'9') => i32::from(digit - b'0'),
            Some(_) => return Err(self.error(INVALID_NUMBER)),
        };
        while let digit @ b'0'..=b'9' = self.peek_or_null() {
            self.index += 1;
            let digit = i32::from(digit - b'0');
            if overflows(exponent as u64, digit as u64, i32::MAX as u64) {
                return self.exponent_overflow(positive, significand == 0, positive_exponent);
            }
            exponent = exponent * 10 + digit;
        }
        let total = if positive_exponent {
            starting.saturating_add(exponent)
        } else {
            starting.saturating_sub(exponent)
        };
        self.compose(positive, significand, total)
    }

    fn compose(&self, positive: bool, significand: u64, mut exponent: i32) -> Parsed<f64> {
        let mut magnitude = significand as f64;
        loop {
            match power_of_ten(exponent.unsigned_abs()) {
                Some(power) => {
                    if exponent >= 0 {
                        magnitude *= power;
                        if magnitude.is_infinite() {
                            return Err(self.error(NUMBER_OUT_OF_RANGE));
                        }
                    } else {
                        magnitude /= power;
                    }
                    break;
                }
                None => {
                    if magnitude == 0.0 {
                        break;
                    }
                    if exponent >= 0 {
                        return Err(self.error(NUMBER_OUT_OF_RANGE));
                    }
                    magnitude /= 1e308;
                    exponent += 308;
                }
            }
        }
        Ok(if positive { magnitude } else { -magnitude })
    }

    fn long_integer(&mut self, positive: bool, significand: u64) -> Parsed<f64> {
        let mut exponent = 0;
        loop {
            match self.peek_or_null() {
                b'0'..=b'9' => {
                    self.index += 1;
                    exponent += 1;
                }
                b'.' => return self.decimal(positive, significand, exponent),
                b'e' | b'E' => return self.exponent(positive, significand, exponent),
                _ => return self.compose(positive, significand, exponent),
            }
        }
    }

    fn decimal_overflow(&mut self, positive: bool, significand: u64, exponent: i32) -> Parsed<f64> {
        while self.peek_or_null().is_ascii_digit() {
            self.index += 1;
        }
        match self.peek_or_null() {
            b'e' | b'E' => self.exponent(positive, significand, exponent),
            _ => self.compose(positive, significand, exponent),
        }
    }

    fn exponent_overflow(
        &mut self,
        positive: bool,
        zero_significand: bool,
        positive_exponent: bool,
    ) -> Parsed<f64> {
        if !zero_significand && positive_exponent {
            return Err(self.error(NUMBER_OUT_OF_RANGE));
        }
        while self.peek_or_null().is_ascii_digit() {
            self.index += 1;
        }
        Ok(if positive { 0.0 } else { -0.0 })
    }
}

fn float(number: f64) -> Value {
    Number::from_f64(number).map_or(Value::Null, Value::Number)
}

pub(crate) fn parse(input: &str) -> Result<Value, String> {
    let mut parser = Parser {
        text: input,
        bytes: input.as_bytes(),
        index: 0,
        depth: RECURSION_LIMIT,
    };
    parser
        .document()
        .map_err(|error| error.describe(input.as_bytes()))
}

pub(crate) fn write_str(out: &mut String, text: &str) {
    out.push('"');
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        let escape = match byte {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            0x08 => "\\b",
            0x0c => "\\f",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0x00..=0x1f => "",
            _ => continue,
        };
        out.push_str(&text[start..index]);
        if escape.is_empty() {
            let _ = write!(out, "\\u{byte:04x}");
        } else {
            out.push_str(escape);
        }
        start = index + 1;
    }
    out.push_str(&text[start..]);
    out.push('"');
}

pub(crate) fn write_items<T>(out: &mut String, items: &[T], item: fn(&mut String, &T)) {
    out.push('[');
    for (position, entry) in items.iter().enumerate() {
        if position > 0 {
            out.push(',');
        }
        item(out, entry);
    }
    out.push(']');
}

pub(crate) fn write_map(out: &mut String, map: &Map<String, Value>) {
    let mut object = Object::open(out);
    for (key, value) in map {
        write(object.key(key), value);
    }
    object.close();
}

pub(crate) fn write(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(number) => {
            let _ = write!(out, "{number}");
        }
        Value::String(text) => write_str(out, text),
        Value::Array(items) => write_items(out, items, write),
        Value::Object(map) => write_map(out, map),
    }
}

pub(crate) fn to_string(value: &Value) -> String {
    let mut out = String::new();
    write(&mut out, value);
    out
}

pub(crate) struct Object<'a> {
    out: &'a mut String,
    empty: bool,
}

impl<'a> Object<'a> {
    pub(crate) fn open(out: &'a mut String) -> Self {
        out.push('{');
        Self { out, empty: true }
    }

    pub(crate) fn key(&mut self, key: &str) -> &mut String {
        if !self.empty {
            self.out.push(',');
        }
        self.empty = false;
        write_str(self.out, key);
        self.out.push(':');
        self.out
    }

    pub(crate) fn string(&mut self, key: &str, text: &str) {
        write_str(self.key(key), text);
    }

    pub(crate) fn optional_string(&mut self, key: &str, text: Option<&str>) {
        match text {
            Some(text) => self.string(key, text),
            None => self.key(key).push_str("null"),
        }
    }

    pub(crate) fn value(&mut self, key: &str, value: &Value) {
        write(self.key(key), value);
    }

    pub(crate) fn close(self) {
        self.out.push('}');
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, power_of_ten, to_string};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn reference(input: &str) -> Result<String, String> {
        serde_json::from_str::<serde_json::Value>(input)
            .map(|value| value.to_string())
            .map_err(|error| error.to_string())
    }

    fn lite(input: &str) -> Result<String, String> {
        parse(input).map(|value| to_string(&value))
    }

    fn assert_same(input: &str) {
        assert_eq!(lite(input), reference(input), "input {input:?}");
    }

    fn json_files(directory: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<PathBuf> = fs::read_dir(directory)
            .expect("directory is readable")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                json_files(&path, out);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                out.push(path);
            }
        }
    }

    fn fixture_texts() -> Vec<String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = Vec::new();
        json_files(&root.join("tests/conformance"), &mut files);
        json_files(&root.join("crates/polyui-core/catalogs"), &mut files);
        assert!(files.len() > 30, "expected the conformance fixtures");
        files
            .iter()
            .map(|path| fs::read_to_string(path).expect("fixture is readable"))
            .collect()
    }

    struct Noise(u64);

    impl Noise {
        fn next(&mut self, bound: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) % bound as u64) as usize
        }
    }

    #[test]
    fn powers_of_ten_match_the_correctly_rounded_literals() {
        for exponent in 0..=308 {
            let literal: f64 = format!("1e{exponent}").parse().expect("literal parses");
            assert_eq!(power_of_ten(exponent), Some(literal), "1e{exponent}");
        }
        assert_eq!(power_of_ten(309), None);
    }

    #[test]
    fn matches_serde_json_on_hand_picked_inputs() {
        let cases = [
            "",
            " ",
            "null",
            "true",
            "false",
            "nul",
            "tru",
            "fals",
            "nulx",
            "0",
            "-0",
            "-",
            "01",
            "1.",
            ".5",
            "1.5",
            "-1.25e-7",
            "1e5",
            "1E+5",
            "1e-5",
            "1e",
            "1e+",
            "1ex",
            "-a",
            "18446744073709551615",
            "18446744073709551616",
            "-9223372036854775808",
            "-9223372036854775809",
            "123456789012345678901234567890",
            "123456789012345678901234567890.5e3",
            "0.1",
            "0.30000000000000004",
            "1e308",
            "1e309",
            "1e-400",
            "-1e-400",
            "0e999999999999",
            "1e2147483648",
            "1e-2147483648",
            "1.7976931348623157e308",
            "2.2250738585072014e-308",
            "5e-324",
            "4.9e-324",
            "3.141592653589793238462643383279",
            "0.000000000000000000000000000000000000000001",
            "\"plain\"",
            "\"\"",
            "\"\\u00e9\\u0041\\n\\t\\r\\b\\f\\/\\\\\\\"\"",
            "\"\\ud83d\\ude00\"",
            "\"\\ud83d\"",
            "\"\\ud83dx\"",
            "\"\\ud83d\\x\"",
            "\"\\ud83d\\u0041\"",
            "\"\\udc00\"",
            "\"\\u12\"",
            "\"\\u12g4\"",
            "\"\\x\"",
            "\"\\",
            "\"abc",
            "\"tab\there\"",
            "\"\u{1}\"",
            "\"\u{7f}\u{80}\u{2028}\"",
            "\"caf\u{e9} \u{1f600}\"",
            "[]",
            "[ ]",
            "[1,]",
            "[1 2]",
            "[,1]",
            "[",
            "[1",
            "[1,",
            "{}",
            "{",
            "{\"a\"",
            "{\"a\":",
            "{\"a\":1",
            "{\"a\":1,",
            "{\"a\":1,}",
            "{1:2}",
            "{\"a\" 1}",
            "{\"a\":1 \"b\":2}",
            "{,}",
            "{\"a\":1,\"b\":2,\"a\":3}",
            "{\"b\":1,\"a\":{\"d\":[1,{\"c\":null}],\"c\":true}}",
            "1 2",
            "[1]x",
            "{}\n\n   x",
            "{\n  \"a\": [\n    1,\n    ]\n}",
            "\n\n\"\n\"",
            " \t\r\n[\r\n1\t]\n ",
        ];
        for case in cases {
            assert_same(case);
        }
        for depth in [1, 31, 32, 33, 126, 127, 128, 129, 300] {
            assert_same(&format!("{}{}", "[".repeat(depth), "]".repeat(depth)));
            assert_same(&"{\"a\":".repeat(depth));
        }
    }

    #[test]
    fn matches_serde_json_on_every_fixture_and_broken_input() {
        for text in fixture_texts() {
            assert_same(&text);
            let fixture: serde_json::Value = serde_json::from_str(&text).expect("fixture is JSON");
            let mut pending = vec![&fixture];
            while let Some(value) = pending.pop() {
                match value {
                    serde_json::Value::String(inner) => assert_same(inner),
                    serde_json::Value::Array(items) => pending.extend(items),
                    serde_json::Value::Object(map) => pending.extend(map.values()),
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn matches_serde_json_on_random_token_soup() {
        let tokens = [
            "{",
            "}",
            "[",
            "]",
            ",",
            ":",
            "\"",
            "\\",
            "a",
            "\u{e9}",
            "\u{1f600}",
            "0",
            "1",
            "9",
            "-",
            "+",
            ".",
            "e",
            "E",
            " ",
            "\n",
            "\t",
            "true",
            "false",
            "null",
            "\\u",
            "d83d",
            "de00",
            "00e9",
            "\u{1}",
            "x",
            "\"k\":",
            "1e400",
            "18446744073709551616",
        ];
        let mut noise = Noise(7);
        for _ in 0..40_000 {
            let length = 1 + noise.next(24);
            let input: String = (0..length)
                .map(|_| tokens[noise.next(tokens.len())])
                .collect();
            assert_same(&input);
        }
    }

    #[test]
    fn matches_serde_json_on_mutated_fixtures() {
        let texts = fixture_texts();
        let inserts = [
            "", ",", "]", "}", "\"", "\\", "x", "1", "-", ".", "e", " ", "\n",
        ];
        let mut noise = Noise(11);
        for text in &texts {
            let boundaries: Vec<usize> = text
                .char_indices()
                .map(|(index, _)| index)
                .chain([text.len()])
                .collect();
            for _ in 0..200 {
                let at = boundaries[noise.next(boundaries.len())];
                let end = boundaries
                    .iter()
                    .copied()
                    .find(|&boundary| boundary > at)
                    .unwrap_or(at);
                let cut = if noise.next(2) == 0 { end } else { at };
                let mutated = format!(
                    "{}{}{}",
                    &text[..at],
                    inserts[noise.next(inserts.len())],
                    &text[cut..]
                );
                assert_same(&mutated);
            }
        }
    }

    #[test]
    fn matches_serde_json_on_random_numbers() {
        let mut noise = Noise(3);
        for _ in 0..40_000 {
            let mut number = String::new();
            if noise.next(3) == 0 {
                number.push('-');
            }
            let digits = 1 + noise.next(30);
            for position in 0..digits {
                let digit = noise.next(10);
                number.push(char::from(
                    b'0' + if position == 0 && digits > 1 {
                        1 + digit % 9
                    } else {
                        digit
                    } as u8,
                ));
            }
            if noise.next(2) == 0 {
                number.push('.');
                for _ in 0..1 + noise.next(25) {
                    number.push(char::from(b'0' + noise.next(10) as u8));
                }
            }
            if noise.next(2) == 0 {
                number.push(['e', 'E'][noise.next(2)]);
                number.push_str(["", "+", "-"][noise.next(3)]);
                number.push_str(&noise.next(400).to_string());
            }
            assert_same(&number);
        }
    }
}
