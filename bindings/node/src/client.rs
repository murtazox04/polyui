use crate::catalogs::Catalogs;
use crate::errors::protocol_error;
use napi::{Env, Result};
use napi_derive::napi;
use polyui_core::Client as Core;
use serde_json::Value;

#[napi]
pub struct Client {
    inner: Core,
}

fn json_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

#[napi]
impl Client {
    #[napi(constructor)]
    pub fn new(
        env: Env,
        hello: Value,
        catalogs: Option<&Catalogs>,
        allow_http: Option<bool>,
    ) -> Result<Self> {
        let text = json_text(&hello);
        let made = match catalogs {
            Some(catalogs) => Core::with_catalogs(&text, catalogs.inner.clone()),
            None => Core::new(&text),
        };
        made.map(|client| Self {
            inner: client.with_allow_http(allow_http.unwrap_or(false)),
        })
        .map_err(|error| {
            protocol_error(
                &env,
                format!("PolyUI could not start a client from this hello: {error}"),
            )
        })
    }

    #[napi(getter)]
    pub fn locale(&self) -> String {
        self.inner.locale().tag().to_string()
    }

    #[napi(getter)]
    pub fn session(&self) -> Option<String> {
        self.inner.session().map(str::to_string)
    }

    #[napi(getter)]
    pub fn surfaces(&self) -> Vec<String> {
        self.inner
            .surfaces()
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[napi]
    pub fn data(&self, surface: String) -> Option<Value> {
        self.inner.data(&surface).cloned()
    }

    #[napi]
    pub fn fallback(&self, surface: String) -> Option<String> {
        self.inner.fallback(&surface).map(str::to_string)
    }

    #[napi]
    pub fn receive(&mut self, message: Value) -> Value {
        self.inner.receive(&json_text(&message)).to_value()
    }

    #[napi]
    pub fn set_value(&mut self, surface: String, path: String, value: Value) -> Value {
        self.inner
            .set_value(&surface, &path, &value.to_string())
            .to_value()
    }

    #[napi]
    pub fn fire(
        &self,
        surface: String,
        component: String,
        prop: String,
        detail: Option<Value>,
    ) -> Option<Value> {
        let detail = detail.map(|value| value.to_string());
        self.inner
            .fire(&surface, &component, &prop, detail.as_deref())
            .filter(|found| !found.is_null())
    }

    #[napi]
    pub fn confirm(&mut self, surface: String, action_id: String, nonce: String) -> Option<Value> {
        self.inner
            .confirm(&surface, &action_id, &nonce)
            .filter(|found| !found.is_null())
    }

    #[napi]
    pub fn say(&self, surface: String) -> Value {
        to_value(&self.inner.say(&surface))
    }

    #[napi]
    pub fn match_voice(&mut self, surface: String, text: String) -> Option<Value> {
        self.inner
            .match_voice(&surface, &text)
            .map(|found| to_value(&found))
    }
}
