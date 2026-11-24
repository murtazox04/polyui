use crate::{Map, Value};
use std::fmt;

pub const MAX_TOKENS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerError {
    Syntax(String),
    TooDeep,
    NotAContainer(String),
    BadIndex(String),
    RootNotObject,
}

impl fmt::Display for PointerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(pointer) => write!(
                f,
                "\"{pointer}\" is not a JSON Pointer (it must start with / and use ~0 or ~1 only)"
            ),
            Self::TooDeep => write!(f, "a JSON Pointer has more than {MAX_TOKENS} parts"),
            Self::NotAContainer(prefix) => {
                write!(f, "\"{prefix}\" already holds a value, not an object")
            }
            Self::BadIndex(token) => write!(f, "\"{token}\" is not a usable array index"),
            Self::RootNotObject => write!(f, "the data model must be an object"),
        }
    }
}

impl std::error::Error for PointerError {}

pub fn parse(pointer: &str) -> Result<Vec<String>, PointerError> {
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let Some(rest) = pointer.strip_prefix('/') else {
        return Err(PointerError::Syntax(pointer.to_string()));
    };
    let mut tokens = Vec::new();
    for raw in rest.split('/') {
        if tokens.len() == MAX_TOKENS {
            return Err(PointerError::TooDeep);
        }
        tokens.push(unescape(raw).ok_or_else(|| PointerError::Syntax(pointer.to_string()))?);
    }
    Ok(tokens)
}

fn unescape(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.next() {
                Some('0') => out.push('~'),
                Some('1') => out.push('/'),
                _ => return None,
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

pub fn escape_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

pub fn is_valid(pointer: &str) -> bool {
    parse(pointer).is_ok()
}

pub fn is_valid_target(pointer: &str) -> bool {
    !pointer.is_empty() && is_valid(pointer)
}

fn array_index(token: &str) -> Option<usize> {
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if token.len() > 1 && token.starts_with('0') {
        return None;
    }
    token.parse().ok()
}

pub fn get<'a>(value: &'a Value, pointer: &str) -> Option<&'a Value> {
    let tokens = parse(pointer).ok()?;
    let mut current = value;
    for token in &tokens {
        current = match current {
            Value::Object(map) => map.get(token)?,
            Value::Array(items) => items.get(array_index(token)?)?,
            _ => return None,
        };
    }
    Some(current)
}

fn prefix_of(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|token| format!("/{}", escape_token(token)))
        .collect()
}

fn walk_creating<'a>(
    target: &'a mut Value,
    tokens: &[String],
) -> Result<&'a mut Value, PointerError> {
    let mut current = target;
    for (depth, token) in tokens.iter().enumerate() {
        current = match current {
            Value::Object(map) => map
                .entry(token.clone())
                .or_insert_with(|| Value::Object(Map::new())),
            Value::Array(items) => {
                let index = if token == "-" {
                    items.len()
                } else {
                    array_index(token).ok_or_else(|| PointerError::BadIndex(token.clone()))?
                };
                if index == items.len() {
                    items.push(Value::Object(Map::new()));
                }
                items
                    .get_mut(index)
                    .ok_or_else(|| PointerError::BadIndex(token.clone()))?
            }
            _ => return Err(PointerError::NotAContainer(prefix_of(&tokens[..depth]))),
        };
    }
    Ok(current)
}

pub fn set(target: &mut Value, pointer: &str, value: Value) -> Result<(), PointerError> {
    let tokens = parse(pointer)?;
    let Some((last, parents)) = tokens.split_last() else {
        if !value.is_object() {
            return Err(PointerError::RootNotObject);
        }
        *target = value;
        return Ok(());
    };
    match walk_creating(target, parents)? {
        Value::Object(map) => {
            map.insert(last.clone(), value);
            Ok(())
        }
        Value::Array(items) => {
            let index = if last == "-" {
                items.len()
            } else {
                array_index(last).ok_or_else(|| PointerError::BadIndex(last.clone()))?
            };
            match index.cmp(&items.len()) {
                std::cmp::Ordering::Less => {
                    if let Some(slot) = items.get_mut(index) {
                        *slot = value;
                    }
                    Ok(())
                }
                std::cmp::Ordering::Equal => {
                    items.push(value);
                    Ok(())
                }
                std::cmp::Ordering::Greater => Err(PointerError::BadIndex(last.clone())),
            }
        }
        _ => Err(PointerError::NotAContainer(prefix_of(parents))),
    }
}

pub fn remove(target: &mut Value, pointer: &str) -> Result<(), PointerError> {
    let tokens = parse(pointer)?;
    let Some((last, parents)) = tokens.split_last() else {
        *target = Value::Object(Map::new());
        return Ok(());
    };
    let mut current = target;
    for token in parents {
        let next = match current {
            Value::Object(map) => map.get_mut(token),
            Value::Array(items) => array_index(token).and_then(|index| items.get_mut(index)),
            _ => None,
        };
        match next {
            Some(value) => current = value,
            None => return Ok(()),
        }
    }
    match current {
        Value::Object(map) => {
            map.shift_remove(last);
        }
        Value::Array(items) => {
            if let Some(index) = array_index(last)
                && index < items.len()
            {
                items.remove(index);
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn merge(target: &mut Value, pointer: &str, patch: Value) -> Result<(), PointerError> {
    let tokens = parse(pointer)?;
    if tokens.is_empty() && !patch.is_object() {
        return Err(PointerError::RootNotObject);
    }
    let slot = walk_creating(target, &tokens)?;
    merge_patch(slot, patch);
    Ok(())
}

pub fn merge_patch(target: &mut Value, patch: Value) {
    let Value::Object(patch_map) = patch else {
        *target = patch;
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    if let Value::Object(target_map) = target {
        for (key, value) in patch_map {
            if value.is_null() {
                target_map.shift_remove(&key);
            } else {
                merge_patch(target_map.entry(key).or_insert(Value::Null), value);
            }
        }
    }
}

#[cfg(all(test, not(feature = "client-lite")))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_and_escapes() {
        assert_eq!(parse("").unwrap(), Vec::<String>::new());
        assert_eq!(parse("/a~1b/c~0d").unwrap(), vec!["a/b", "c~d"]);
        assert!(parse("a").is_err());
        assert!(parse("/a~2").is_err());
        assert!(parse("/a~").is_err());
        assert!(parse(&"/a".repeat(33)).is_err());
        assert!(parse(&"/a".repeat(32)).is_ok());
        assert!(!is_valid_target(""));
    }

    #[test]
    fn set_creates_objects() {
        let mut data = json!({});
        set(&mut data, "/form/phone", json!("+998")).unwrap();
        set(&mut data, "/bills/0/paid", json!(true)).unwrap();
        assert_eq!(
            data,
            json!({ "form": { "phone": "+998" }, "bills": { "0": { "paid": true } } })
        );
        assert!(set(&mut data, "/form/phone/x", json!(1)).is_err());
        assert!(set(&mut data, "", json!(1)).is_err());
    }

    #[test]
    fn arrays_by_index() {
        let mut data = json!({ "list": [1, 2] });
        set(&mut data, "/list/-", json!(3)).unwrap();
        set(&mut data, "/list/0", json!(0)).unwrap();
        assert!(set(&mut data, "/list/9", json!(0)).is_err());
        assert!(set(&mut data, "/list/01", json!(0)).is_err());
        remove(&mut data, "/list/1").unwrap();
        assert_eq!(data, json!({ "list": [0, 3] }));
        assert_eq!(get(&data, "/list/1"), Some(&json!(3)));
        assert_eq!(get(&data, "/list/x"), None);
    }

    #[test]
    fn merge_follows_rfc_7396() {
        let mut data = json!({ "a": { "b": 1, "c": 2 } });
        merge(&mut data, "/a", json!({ "b": null, "d": { "e": 1 } })).unwrap();
        assert_eq!(data, json!({ "a": { "c": 2, "d": { "e": 1 } } }));
        merge(&mut data, "/new/deep", json!({ "x": 1 })).unwrap();
        assert_eq!(get(&data, "/new/deep/x"), Some(&json!(1)));
        remove(&mut data, "/missing/path").unwrap();
    }
}
