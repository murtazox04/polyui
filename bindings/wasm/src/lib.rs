use polyui_core::Client as Core;
#[cfg(target_family = "wasm")]
use polyui_core::json_error;
use wasm_bindgen::prelude::*;

#[cfg(not(target_family = "wasm"))]
fn json_error(input: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(input)
        .err()
        .map(|error| error.to_string())
}

fn nullable(json: String) -> JsValue {
    if json == "null" {
        JsValue::NULL
    } else {
        JsValue::from_str(&json)
    }
}

fn require_json(input: &str, call: &str, what: &str) -> Result<(), JsError> {
    match json_error(input) {
        Some(error) => Err(JsError::new(&format!(
            "PolyUI {call}: {what} is not valid JSON ({error})"
        ))),
        None => Ok(()),
    }
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
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_default();
        if !detail.is_empty() {
            require_json(detail, "fire", "the detail")?;
        }
        Ok(nullable(
            self.core.fire_json(surface, component, prop, detail),
        ))
    }

    #[wasm_bindgen(unchecked_return_type = "string | null")]
    pub fn confirm(&mut self, surface: &str, action_id: &str, nonce: &str) -> JsValue {
        nullable(self.core.confirm_json(surface, action_id, nonce))
    }

    pub fn say(&self, surface: &str) -> String {
        self.core.say_json(surface)
    }

    #[wasm_bindgen(js_name = matchVoice, unchecked_return_type = "string | null")]
    pub fn match_voice(&mut self, surface: &str, text: &str) -> JsValue {
        nullable(self.core.match_voice_json(surface, text))
    }
}

#[cfg(all(not(target_feature = "atomics"), target_family = "wasm"))]
#[global_allocator]
static TALC: talc::wasm::WasmDynamicTalc = talc::wasm::new_wasm_dynamic_allocator();
