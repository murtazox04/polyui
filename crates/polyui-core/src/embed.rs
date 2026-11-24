use crate::client::{APP_PROP, EMBED_TYPE, MCP_APP_MIME_TYPE};
use crate::limits::{LimitError, Limits};
use crate::protocol::{self, Component, Message, Patch};
use crate::validate::{Location, Problem};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const UI_SCHEME: &str = "ui://";
pub const RESOURCE_PROP: &str = "resource";
pub const MAX_APPS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct UiResource {
    pub content: Value,
    pub tool_input: Option<Map<String, Value>>,
    pub tool_result: Option<Map<String, Value>>,
}

impl UiResource {
    pub fn new(content: Value) -> Self {
        Self {
            content,
            tool_input: None,
            tool_result: None,
        }
    }

    pub fn with_tool_input(mut self, input: Map<String, Value>) -> Self {
        self.tool_input = Some(input);
        self
    }

    pub fn with_tool_result(mut self, result: Map<String, Value>) -> Self {
        self.tool_result = Some(result);
        self
    }

    pub fn from_value(value: Value) -> Result<Self, String> {
        let Value::Object(mut map) = value else {
            return Err("a UI resource must be an object".into());
        };
        let Some(content) = map.remove(RESOURCE_PROP) else {
            return Ok(Self::new(Value::Object(map)));
        };
        Ok(Self {
            content,
            tool_input: object_field(&mut map, "tool_input")?,
            tool_result: object_field(&mut map, "tool_result")?,
        })
    }

    pub fn app(&self, uri: &str) -> Result<Value, String> {
        let Value::Object(content) = &self.content else {
            return Err("the resources/read entry is not an object".into());
        };
        let found = content.get("uri").and_then(Value::as_str);
        if found != Some(uri) {
            return Err(format!(
                "the resources/read entry is for {}, not {uri}",
                found.unwrap_or("no uri")
            ));
        }
        if content.get("mimeType").and_then(Value::as_str) != Some(MCP_APP_MIME_TYPE) {
            return Err(format!("the mimeType is not {MCP_APP_MIME_TYPE}"));
        }
        let text = match (content.get("text"), content.get("blob")) {
            (Some(Value::String(text)), _) => text.clone(),
            (None, Some(Value::String(blob))) => decode(blob)?,
            _ => return Err("the entry has neither a text nor a base64 blob".into()),
        };
        let mut resource = content.clone();
        resource.remove("blob");
        resource.insert("text".into(), Value::String(text));
        let mut app = Map::new();
        app.insert(RESOURCE_PROP.into(), Value::Object(resource));
        if let Some(input) = &self.tool_input {
            app.insert("tool_input".into(), Value::Object(input.clone()));
        }
        if let Some(result) = &self.tool_result {
            app.insert("tool_result".into(), Value::Object(result.clone()));
        }
        Ok(Value::Object(app))
    }
}

fn object_field(
    map: &mut Map<String, Value>,
    key: &str,
) -> Result<Option<Map<String, Value>>, String> {
    match map.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(object)) => Ok(Some(object)),
        Some(_) => Err(format!("{key} must be an object")),
    }
}

fn decode(blob: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(blob.trim())
        .map_err(|error| format!("the blob is not base64: {error}"))?;
    String::from_utf8(bytes).map_err(|_| "the blob is not UTF-8 text".to_string())
}

pub fn literal_resource(component: &Component) -> Option<&str> {
    if component.type_name != EMBED_TYPE {
        return None;
    }
    component
        .props
        .get(RESOURCE_PROP)
        .and_then(Value::as_str)
        .filter(|uri| uri.starts_with(UI_SCHEME))
}

pub fn resources_in(input: &Value, list: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let components = input.get(list).and_then(Value::as_array);
    for component in components.into_iter().flatten() {
        if component.get("type").and_then(Value::as_str) != Some(EMBED_TYPE) {
            continue;
        }
        let uri = component
            .get("props")
            .and_then(|props| props.get(RESOURCE_PROP))
            .and_then(Value::as_str)
            .filter(|uri| uri.starts_with(UI_SCHEME));
        if let Some(uri) = uri
            && !found.iter().any(|seen| seen == uri)
        {
            found.push(uri.to_string());
        }
    }
    found
}

pub fn too_many_apps() -> String {
    format!(
        "the call names more than {MAX_APPS} different MCP Apps, and at most {MAX_APPS} can be shown at once."
    )
}

fn fits_alone(component: &Component, limits: &Limits) -> Result<(), LimitError> {
    let probe = Message::Patch(Patch {
        surface: "s".into(),
        seq: 1,
        upsert: vec![component.clone()],
        remove: Vec::new(),
        root: None,
    })
    .to_value();
    protocol::check_for_client(&probe, limits)
}

pub fn attach_apps(
    components: &mut [Component],
    list: &str,
    limits: &Limits,
    read: &mut dyn FnMut(&str) -> Option<UiResource>,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut apps: BTreeMap<String, Result<Value, String>> = BTreeMap::new();
    for (index, component) in components.iter_mut().enumerate() {
        if component.type_name != EMBED_TYPE {
            continue;
        }
        component.props.remove(APP_PROP);
        let Some(uri) = literal_resource(component).map(str::to_string) else {
            continue;
        };
        let location = Location::Item {
            list: list.to_string(),
            index,
            id: Some(component.id.clone()),
            prop: Some(RESOURCE_PROP.into()),
        };
        if !apps.contains_key(&uri) && apps.len() >= MAX_APPS {
            problems.push(Problem::new(location, too_many_apps()));
            continue;
        }
        let app = apps
            .entry(uri.clone())
            .or_insert_with(|| {
                read(&uri)
                    .ok_or_else(|| "the host has no such resource".to_string())
                    .and_then(|resource| resource.app(&uri))
            })
            .clone();
        let Ok(app) = app else {
            problems.push(Problem::new(
                location,
                format!(
                    "{uri} is not an MCP App this host offers. Show it with other components instead."
                ),
            ));
            continue;
        };
        component.props.insert(APP_PROP.into(), app);
        if let Err(error) = fits_alone(component, limits) {
            component.props.remove(APP_PROP);
            problems.push(Problem::new(
                location,
                format!(
                    "{uri} is too large to send to the client ({error}). Show it with other components instead."
                ),
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const URI: &str = "ui://bills/tariff";

    fn embed(id: &str, uri: &str) -> Component {
        serde_json::from_value(json!({ "id": id, "type": EMBED_TYPE, "props": { "resource": uri, "title": "Tariffs" } })).unwrap()
    }

    fn entry(uri: &str, html: &str) -> Value {
        json!({ "uri": uri, "mimeType": MCP_APP_MIME_TYPE, "text": html })
    }

    #[test]
    fn text_and_base64_blobs_become_text() {
        let blob = STANDARD.encode("<!doctype html><p>hi</p>");
        let resource = UiResource::new(
            json!({ "uri": URI, "mimeType": MCP_APP_MIME_TYPE, "blob": blob, "_meta": { "ui": {} } }),
        );
        let app = resource.app(URI).unwrap();
        assert_eq!(app["resource"]["text"], "<!doctype html><p>hi</p>");
        assert!(app["resource"].get("blob").is_none());
        assert_eq!(app["resource"]["_meta"], json!({ "ui": {} }));
        assert!(app.get("tool_input").is_none());
        let plain = UiResource::new(entry(URI, "<p>x</p>"))
            .with_tool_input(Map::from_iter([("city".into(), json!("Tashkent"))]));
        assert_eq!(plain.app(URI).unwrap()["tool_input"]["city"], "Tashkent");
    }

    #[test]
    fn wrong_mime_uri_or_body_is_refused() {
        let wrong_mime =
            UiResource::new(json!({ "uri": URI, "mimeType": "text/html", "text": "x" }));
        assert!(wrong_mime.app(URI).is_err());
        let wrong_uri = UiResource::new(entry("ui://bills/other", "x"));
        assert!(wrong_uri.app(URI).is_err());
        let bad_blob =
            UiResource::new(json!({ "uri": URI, "mimeType": MCP_APP_MIME_TYPE, "blob": "%%%" }));
        assert!(bad_blob.app(URI).is_err());
        let not_utf8 = UiResource::new(
            json!({ "uri": URI, "mimeType": MCP_APP_MIME_TYPE, "blob": STANDARD.encode([0xff, 0xfe]) }),
        );
        assert!(not_utf8.app(URI).is_err());
        let empty = UiResource::new(json!({ "uri": URI, "mimeType": MCP_APP_MIME_TYPE }));
        assert!(empty.app(URI).is_err());
    }

    #[test]
    fn from_value_takes_an_entry_or_a_wrapper() {
        let bare = UiResource::from_value(entry(URI, "x")).unwrap();
        assert_eq!(bare.content["uri"], URI);
        let wrapped = UiResource::from_value(
            json!({ "resource": entry(URI, "x"), "tool_result": { "content": [] } }),
        )
        .unwrap();
        assert_eq!(wrapped.tool_result.unwrap()["content"], json!([]));
        assert!(
            UiResource::from_value(json!({ "resource": entry(URI, "x"), "tool_input": 3 }))
                .is_err()
        );
        assert!(UiResource::from_value(json!("x")).is_err());
    }

    #[test]
    fn attach_fills_app_and_reports_problems() {
        let mut components = vec![
            embed("a", URI),
            embed("b", "ui://bills/missing"),
            embed("c", URI),
        ];
        let mut reads = Vec::new();
        let problems = attach_apps(
            &mut components,
            "components",
            &Limits::default(),
            &mut |uri| {
                reads.push(uri.to_string());
                (uri == URI).then(|| UiResource::new(entry(URI, "<p>calc</p>")))
            },
        );
        assert_eq!(
            reads,
            vec![URI.to_string(), "ui://bills/missing".to_string()]
        );
        assert_eq!(
            components[0].props[APP_PROP]["resource"]["text"],
            "<p>calc</p>"
        );
        assert_eq!(components[2].props[APP_PROP], components[0].props[APP_PROP]);
        assert!(components[1].props.get(APP_PROP).is_none());
        assert_eq!(problems.len(), 1);
        assert_eq!(
            problems[0].to_string(),
            "components[1] id \"b\" prop \"resource\": ui://bills/missing is not an MCP App this host offers. Show it with other components instead."
        );
    }

    #[test]
    fn apps_that_cannot_fit_one_message_are_refused() {
        let mut components = vec![embed("a", URI)];
        let big = "x".repeat(150 * 1024);
        let problems = attach_apps(&mut components, "upsert", &Limits::default(), &mut |_| {
            Some(
                UiResource::new(entry(URI, &big)).with_tool_result(Map::from_iter([(
                    "structuredContent".into(),
                    json!({ "padding": big }),
                )])),
            )
        });
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("too large"), "{}", problems[0]);
        assert!(components[0].props.get(APP_PROP).is_none());
        let mut components = vec![embed("a", URI)];
        let huge = "x".repeat(210 * 1024);
        let problems = attach_apps(&mut components, "upsert", &Limits::default(), &mut |_| {
            Some(UiResource::new(entry(URI, &huge)))
        });
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn distinct_apps_are_capped_and_listed_once() {
        let mut components: Vec<Component> = (0..=MAX_APPS)
            .map(|index| embed(&format!("e{index}"), &format!("ui://apps/{index}")))
            .collect();
        let problems = attach_apps(
            &mut components,
            "components",
            &Limits::default(),
            &mut |uri| Some(UiResource::new(entry(uri, "x"))),
        );
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("at most 8"));
        let input = json!({ "components": [
            { "id": "a", "type": EMBED_TYPE, "props": { "resource": URI } },
            { "id": "b", "type": EMBED_TYPE, "props": { "resource": URI } },
            { "id": "c", "type": EMBED_TYPE, "props": { "resource": "https://example.com" } },
            { "id": "d", "type": "Text", "props": { "resource": "ui://x/y" } }
        ] });
        assert_eq!(resources_in(&input, "components"), vec![URI.to_string()]);
        assert!(resources_in(&input, "upsert").is_empty());
    }
}
