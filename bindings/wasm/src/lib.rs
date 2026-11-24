use polyui_core::Client as Core;
use serde_json::Value;
use wasm_bindgen::prelude::*;

fn nullable(text: Option<String>) -> JsValue {
    text.map_or(JsValue::NULL, |text| JsValue::from_str(&text))
}

fn require_json(input: &str, call: &str, what: &str) -> Result<(), JsError> {
    serde_json::from_str::<Value>(input)
        .map(drop)
        .map_err(|error| {
            JsError::new(&format!(
                "PolyUI {call}: {what} is not valid JSON ({error})"
            ))
        })
}

#[wasm_bindgen]
pub struct Client {
    core: Core,
}

#[wasm_bindgen]
impl Client {
    #[wasm_bindgen(constructor)]
    pub fn new(hello_json: &str) -> Result<Client, JsError> {
        Core::new(hello_json)
            .map(|core| Client { core })
            .map_err(|error| {
                JsError::new(&format!(
                    "PolyUI could not start a client from this hello: {error}"
                ))
            })
    }

    pub fn receive(&mut self, message: &str) -> String {
        self.core.receive_json(message)
    }

    #[wasm_bindgen(js_name = setValue)]
    pub fn set_value(
        &mut self,
        surface: &str,
        path: &str,
        value_json: &str,
    ) -> Result<String, JsError> {
        require_json(value_json, "setValue", "the value")?;
        Ok(self.core.set_value_json(surface, path, value_json))
    }

    #[wasm_bindgen(unchecked_return_type = "string | null")]
    pub fn fire(
        &self,
        surface: &str,
        component: &str,
        prop: &str,
        detail_json: Option<String>,
    ) -> Result<JsValue, JsError> {
        let detail = detail_json
            .as_deref()
            .filter(|text| !text.trim().is_empty());
        if let Some(text) = detail {
            require_json(text, "fire", "the detail")?;
        }
        Ok(nullable(
            self.core
                .fire(surface, component, prop, detail)
                .map(|action| action.to_string()),
        ))
    }

    #[wasm_bindgen(unchecked_return_type = "string | null")]
    pub fn confirm(&mut self, surface: &str, action_id: &str, nonce: &str) -> JsValue {
        nullable(
            self.core
                .confirm(surface, action_id, nonce)
                .map(|action| action.to_string()),
        )
    }

    pub fn say(&self, surface: &str) -> String {
        self.core.say_json(surface)
    }

    #[wasm_bindgen(js_name = matchVoice, unchecked_return_type = "string | null")]
    pub fn match_voice(&mut self, surface: &str, text: &str) -> JsValue {
        nullable(Some(self.core.match_voice_json(surface, text)).filter(|found| found != "null"))
    }
}

#[cfg(all(not(target_feature = "atomics"), target_family = "wasm"))]
#[global_allocator]
static TALC: talc::wasm::WasmDynamicTalc = talc::wasm::new_wasm_dynamic_allocator();
