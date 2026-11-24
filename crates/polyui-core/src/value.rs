use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, Serializer};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Repr {
    Positive(u64),
    Negative(i64),
    Float(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Number(Repr);

impl Number {
    pub fn from_f64(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(Repr::Float(value)))
    }

    pub fn is_u64(&self) -> bool {
        matches!(self.0, Repr::Positive(_))
    }

    pub fn is_i64(&self) -> bool {
        match self.0 {
            Repr::Positive(value) => i64::try_from(value).is_ok(),
            Repr::Negative(_) => true,
            Repr::Float(_) => false,
        }
    }

    pub fn is_f64(&self) -> bool {
        matches!(self.0, Repr::Float(_))
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self.0 {
            Repr::Positive(value) => Some(value),
            Repr::Negative(_) | Repr::Float(_) => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self.0 {
            Repr::Positive(value) => i64::try_from(value).ok(),
            Repr::Negative(value) => Some(value),
            Repr::Float(_) => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self.0 {
            Repr::Positive(value) => Some(value as f64),
            Repr::Negative(value) => Some(value as f64),
            Repr::Float(value) => Some(value),
        }
    }
}

impl From<u64> for Number {
    fn from(value: u64) -> Self {
        Self(Repr::Positive(value))
    }
}

impl From<i64> for Number {
    fn from(value: i64) -> Self {
        match u64::try_from(value) {
            Ok(positive) => Self(Repr::Positive(positive)),
            Err(_) => Self(Repr::Negative(value)),
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Repr::Positive(value) => write!(f, "{value}"),
            Repr::Negative(value) => write!(f, "{value}"),
            Repr::Float(value) => f.write_str(zmij::Buffer::new().format_finite(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(Number),
    String(String),
    Array(Vec<Value>),
    Object(Map<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.get(key)
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn is_boolean(&self) -> bool {
        matches!(self, Self::Bool(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Self::String(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(number) => number.as_u64(),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(number) => number.as_i64(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(number) => number.as_f64(),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Map<String, Value>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }
}

impl From<bool> for Value {
    fn from(flag: bool) -> Self {
        Self::Bool(flag)
    }
}

impl From<u64> for Value {
    fn from(number: u64) -> Self {
        Self::Number(number.into())
    }
}

impl From<i64> for Value {
    fn from(number: i64) -> Self {
        Self::Number(number.into())
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Self::String(text.to_string())
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::String(text)
    }
}

#[derive(Debug, Clone)]
pub struct Map<K, V> {
    entries: Vec<(K, V)>,
}

impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

pub type Iter<'a> = std::iter::Map<
    std::slice::Iter<'a, (String, Value)>,
    fn(&'a (String, Value)) -> (&'a String, &'a Value),
>;

pub type IterMut<'a> = std::iter::Map<
    std::slice::IterMut<'a, (String, Value)>,
    fn(&'a mut (String, Value)) -> (&'a String, &'a mut Value),
>;

pub type IntoIter = std::vec::IntoIter<(String, Value)>;

impl Map<String, Value> {
    pub fn new() -> Self {
        Self::default()
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|(existing, _)| existing == key)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.position(key).map(|index| &self.entries[index].1)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.position(key).map(|index| &mut self.entries[index].1)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    pub fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        match self.position(&key) {
            Some(index) => Some(std::mem::replace(&mut self.entries[index].1, value)),
            None => {
                self.entries.push((key, value));
                None
            }
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.position(key)
            .map(|index| self.entries.swap_remove(index).1)
    }

    pub fn shift_remove(&mut self, key: &str) -> Option<Value> {
        self.position(key).map(|index| self.entries.remove(index).1)
    }

    pub fn entry(&mut self, key: impl Into<String>) -> Entry<'_> {
        let key = key.into();
        let index = self.position(&key);
        Entry {
            map: self,
            key,
            index,
        }
    }

    pub fn iter(&self) -> Iter<'_> {
        self.entries.iter().map(|(key, value)| (key, value))
    }

    pub fn iter_mut(&mut self) -> IterMut<'_> {
        self.entries.iter_mut().map(|(key, value)| (&*key, value))
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|(key, _)| key)
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().map(|(_, value)| value)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&String, &mut Value) -> bool) {
        self.entries.retain_mut(|(key, value)| keep(key, value));
    }
}

impl PartialEq for Map<String, Value> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self
                .iter()
                .all(|(key, value)| other.get(key) == Some(value))
    }
}

pub struct Entry<'a> {
    map: &'a mut Map<String, Value>,
    key: String,
    index: Option<usize>,
}

impl<'a> Entry<'a> {
    pub fn or_insert(self, default: Value) -> &'a mut Value {
        self.or_insert_with(|| default)
    }

    pub fn or_insert_with(self, default: impl FnOnce() -> Value) -> &'a mut Value {
        let index = match self.index {
            Some(index) => index,
            None => {
                self.map.entries.push((self.key, default()));
                self.map.entries.len() - 1
            }
        };
        &mut self.map.entries[index].1
    }
}

impl Extend<(String, Value)> for Map<String, Value> {
    fn extend<I: IntoIterator<Item = (String, Value)>>(&mut self, items: I) {
        for (key, value) in items {
            self.insert(key, value);
        }
    }
}

impl FromIterator<(String, Value)> for Map<String, Value> {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(items: I) -> Self {
        let mut map = Self::new();
        map.extend(items);
        map
    }
}

impl IntoIterator for Map<String, Value> {
    type Item = (String, Value);
    type IntoIter = IntoIter;

    fn into_iter(self) -> IntoIter {
        self.entries.into_iter()
    }
}

impl<'a> IntoIterator for &'a Map<String, Value> {
    type Item = (&'a String, &'a Value);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut Map<String, Value> {
    type Item = (&'a String, &'a mut Value);
    type IntoIter = IterMut<'a>;

    fn into_iter(self) -> IterMut<'a> {
        self.iter_mut()
    }
}

impl Serialize for Number {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Repr::Positive(value) => serializer.serialize_u64(value),
            Repr::Negative(value) => serializer.serialize_i64(value),
            Repr::Float(value) => serializer.serialize_f64(value),
        }
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(flag) => serializer.serialize_bool(*flag),
            Self::Number(number) => number.serialize(serializer),
            Self::String(text) => serializer.serialize_str(text),
            Self::Array(items) => items.serialize(serializer),
            Self::Object(map) => map.serialize(serializer),
        }
    }
}

impl Serialize for Map<String, Value> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_map(Some(self.len()))?;
        for (key, value) in self {
            out.serialize_entry(key, value)?;
        }
        out.end()
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_bool<E>(self, flag: bool) -> Result<Value, E> {
        Ok(Value::Bool(flag))
    }

    fn visit_i64<E>(self, number: i64) -> Result<Value, E> {
        Ok(Value::from(number))
    }

    fn visit_u64<E>(self, number: u64) -> Result<Value, E> {
        Ok(Value::from(number))
    }

    fn visit_f64<E>(self, number: f64) -> Result<Value, E> {
        Ok(Number::from_f64(number).map_or(Value::Null, Value::Number))
    }

    fn visit_str<E>(self, text: &str) -> Result<Value, E> {
        Ok(Value::from(text))
    }

    fn visit_string<E>(self, text: String) -> Result<Value, E> {
        Ok(Value::String(text))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        Value::deserialize(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = access.next_element()? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut map = Map::new();
        while let Some((key, value)) = access.next_entry()? {
            map.insert(key, value);
        }
        Ok(Value::Object(map))
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

impl<'de> Deserialize<'de> for Map<String, Value> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::Object(map) => Ok(map),
            _ => Err(de::Error::custom("expected a map")),
        }
    }
}
