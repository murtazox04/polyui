use crate::limits::{Container, LimitError, Scanner, Token};
use crate::protocol::Component;
use crate::validate::{Location, Operation, Problem, Problems, Surface, Validator};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Surface(String),
    Root(String),
    Component { index: usize, component: Component },
    Rejected { index: usize, problems: Problems },
    Data(Map<String, Value>),
    Invalid(Problems),
}

pub struct ShowUiStream<'a> {
    validator: &'a Validator<'a>,
    scanner: Scanner,
    buffer: String,
    expect_key: bool,
    key_start: usize,
    key: String,
    value_start: usize,
    in_components: bool,
    element_expected: bool,
    element_start: usize,
    element_index: usize,
    elements_started: usize,
    seen_ids: BTreeSet<String>,
    data: Value,
    failure: Option<String>,
}

fn single(location: Location, message: String) -> Problems {
    Problems {
        operation: Operation::Show,
        items: vec![Problem::new(location, message)],
    }
}

impl<'a> ShowUiStream<'a> {
    pub fn new(validator: &'a Validator<'a>) -> Self {
        Self {
            validator,
            scanner: Scanner::new(*validator.limits()),
            buffer: String::new(),
            expect_key: false,
            key_start: 0,
            key: String::new(),
            value_start: 0,
            in_components: false,
            element_expected: false,
            element_start: 0,
            element_index: 0,
            elements_started: 0,
            seen_ids: BTreeSet::new(),
            data: Value::Object(Map::new()),
            failure: None,
        }
    }

    pub fn failed(&self) -> bool {
        self.failure.is_some()
    }

    pub fn text(&self) -> &str {
        &self.buffer
    }

    pub fn feed(&mut self, chunk: &str) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if self.failure.is_some() || chunk.is_empty() {
            return events;
        }
        let limit = self.validator.limits().max_message_bytes;
        if self.buffer.len() + chunk.len() > limit {
            let error = LimitError::MessageTooLarge {
                size: self.buffer.len() + chunk.len(),
                limit,
            };
            self.fail(format!("{error}."), &mut events);
            return events;
        }
        let start = self.buffer.len();
        self.buffer.push_str(chunk);
        for offset in start..self.buffer.len() {
            let Some(&byte) = self.buffer.as_bytes().get(offset) else {
                break;
            };
            match self.scanner.push(byte) {
                Ok(token) => self.step(token, offset, &mut events),
                Err(error) => self.fail(format!("{error}."), &mut events),
            }
            if self.failure.is_some() {
                break;
            }
        }
        events
    }

    pub fn finish(self) -> Result<Surface, Problems> {
        if let Some(message) = self.failure {
            return Err(single(Location::Input, message));
        }
        self.validator.show_ui_str(&self.buffer)
    }

    fn fail(&mut self, message: String, events: &mut Vec<StreamEvent>) {
        events.push(StreamEvent::Invalid(single(
            Location::Input,
            message.clone(),
        )));
        self.failure = Some(message);
    }

    fn start_element(&mut self) {
        if self.in_components && self.element_expected {
            self.element_index = self.elements_started;
            self.elements_started += 1;
            self.element_expected = false;
        }
    }

    fn step(&mut self, token: Token, offset: usize, events: &mut Vec<StreamEvent>) {
        let depth = self.scanner.depth();
        match token {
            Token::Unbalanced => self.fail("JSON brackets do not match.".into(), events),
            Token::Open(container) => match depth {
                1 => self.expect_key = true,
                2 => {
                    self.value_start = offset;
                    if self.key == "components" && container == Container::Array {
                        self.in_components = true;
                        self.element_expected = true;
                    }
                }
                3 if self.in_components => {
                    self.start_element();
                    self.element_start = offset;
                }
                _ => {}
            },
            Token::Close(container) => match depth {
                1 => {
                    self.top_value_done(offset, events);
                    self.in_components = false;
                }
                2 if self.in_components && container == Container::Object => {
                    self.element_done(offset, events);
                }
                _ => {}
            },
            Token::StringStart => match depth {
                1 if self.expect_key => self.key_start = offset,
                1 => self.value_start = offset,
                2 if self.scanner.top() == Some(Container::Array) => self.start_element(),
                _ => {}
            },
            Token::StringEnd => {
                if depth == 1 {
                    if self.expect_key {
                        self.key = self
                            .buffer
                            .get(self.key_start..=offset)
                            .and_then(|text| serde_json::from_str::<String>(text).ok())
                            .unwrap_or_default();
                        self.expect_key = false;
                    } else {
                        self.top_value_done(offset, events);
                    }
                }
            }
            Token::ValueStart => {
                if depth == 2 {
                    self.start_element();
                }
            }
            Token::Comma => match depth {
                1 => self.expect_key = true,
                2 if self.in_components => self.element_expected = true,
                _ => {}
            },
            Token::Colon | Token::Nothing => {}
        }
    }

    fn top_value_done(&mut self, offset: usize, events: &mut Vec<StreamEvent>) {
        let Some(text) = self.buffer.get(self.value_start..=offset) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            return;
        };
        match (self.key.as_str(), value) {
            ("root", Value::String(root)) => events.push(StreamEvent::Root(root)),
            ("surface", Value::String(surface)) => events.push(StreamEvent::Surface(surface)),
            ("data", value) => match self.validator.data(&value) {
                Ok(map) => {
                    self.data = Value::Object(map.clone());
                    events.push(StreamEvent::Data(map));
                }
                Err(problems) => events.push(StreamEvent::Invalid(problems)),
            },
            _ => {}
        }
    }

    fn element_done(&mut self, offset: usize, events: &mut Vec<StreamEvent>) {
        let index = self.element_index;
        let parsed = self
            .buffer
            .get(self.element_start..=offset)
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        let Some(value) = parsed else {
            events.push(StreamEvent::Rejected {
                index,
                problems: single(
                    Location::Field(format!("components[{index}]")),
                    "is not valid JSON.".into(),
                ),
            });
            return;
        };
        match self
            .validator
            .component("components", index, &value, Some(&self.data))
        {
            Ok(component) => {
                if self.seen_ids.insert(component.id.clone()) {
                    events.push(StreamEvent::Component { index, component });
                } else {
                    events.push(StreamEvent::Rejected {
                        index,
                        problems: single(
                            Location::Item {
                                list: "components".into(),
                                index,
                                id: Some(component.id.clone()),
                                prop: None,
                            },
                            "duplicate id.".into(),
                        ),
                    });
                }
            }
            Err(items) => events.push(StreamEvent::Rejected {
                index,
                problems: Problems {
                    operation: Operation::Show,
                    items,
                },
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CatalogSet;
    use crate::validate::{ActionTable, StaticRefs};

    #[test]
    fn root_and_component_events() {
        let catalogs = CatalogSet::builtin();
        let actions = ActionTable::new();
        let refs = StaticRefs::new();
        let validator = Validator::new(&catalogs, &actions, &refs);
        let mut stream = ShowUiStream::new(&validator);
        let mut events = stream.feed(r#"{"root":"d","compo"#);
        assert_eq!(events, vec![StreamEvent::Root("d".into())]);
        events = stream.feed(r#"nents":[{"id":"d","type":"Divi"#);
        assert!(events.is_empty());
        events = stream.feed(r#"der"}]}"#);
        assert!(matches!(
            &events[..],
            [StreamEvent::Component { index: 0, .. }]
        ));
        assert_eq!(stream.text().len(), 55);
        assert!(stream.finish().is_ok());
    }
}
