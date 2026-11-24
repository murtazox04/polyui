use polyui_core::protocol;
use polyui_core::sanitize;
use polyui_core::text::{self, Locale};
use polyui_core::validate::{ActionSpec, ActionTable, StaticRefs, Surface, Validator};
use polyui_core::{Catalog, CatalogSet, Client, pointer, voice};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::Path;

struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit.max(1) as u64) as usize
    }

    fn bytes(&mut self, length: usize) -> Vec<u8> {
        (0..length).map(|_| (self.next() & 0xff) as u8).collect()
    }

    fn json_noise(&mut self, length: usize) -> String {
        const ALPHABET: &[u8] = b"{}[]\":,\\/~01-tfnu eE.$abcxyz";
        (0..length)
            .map(|_| ALPHABET[self.below(ALPHABET.len())] as char)
            .collect()
    }
}

fn hello() -> String {
    json!({ "v": 1, "t": "hello", "catalogs": ["core@1", "chart@1"], "modalities": ["screen", "voice"], "locale": "uz-Latn" }).to_string()
}

fn sample_messages() -> Vec<String> {
    let mut messages = vec![
        json!({ "v": 1, "t": "ui.open", "surface": "s1", "seq": 1, "root": "card", "components": [
            { "id": "card", "type": "Card", "props": { "title": "Your bills" }, "children": ["choices", "pay", "phone"] },
            { "id": "choices", "type": "ChoiceGroup", "props": { "label": "Pick", "value": "/pick", "options": [{ "id": "gas", "label": "Gas" }, { "id": "power", "label": "Power" }], "on_change": { "action_id": "a1", "confirm": false } } },
            { "id": "pay", "type": "Button", "props": { "label": "Pay", "on_press": { "action_id": "a2", "confirm": true, "with": ["/pick"] } } },
            { "id": "phone", "type": "Input", "props": { "label": "Phone", "value": "/form/phone" } }
        ], "data": { "pick": "gas" } }),
        json!({ "v": 1, "t": "ui.patch", "surface": "s1", "seq": 2, "upsert": [{ "id": "note", "type": "Text", "props": { "text": { "$bind": "/pick" } } }], "remove": ["phone"] }),
        json!({ "v": 1, "t": "ui.data", "surface": "s1", "seq": 3, "ops": [{ "op": "set", "path": "/pick", "value": "power" }, { "op": "merge", "path": "/form", "value": { "a": 1 } }] }),
        json!({ "v": 1, "t": "ui.confirm", "surface": "s1", "action_id": "a2", "nonce": "n1", "title": "Pay" }),
        json!({ "v": 1, "t": "ui.sync", "surface": "s1", "highlight": "pay" }),
        json!({ "v": 1, "t": "ui.snapshot", "surface": "s1", "seq": 9, "root": "t", "components": [{ "id": "t", "type": "Text", "props": { "text": "x" } }], "data": {} }),
        json!({ "v": 1, "t": "ui.close", "surface": "s1", "seq": 10 }),
    ]
    .into_iter()
    .map(|message| message.to_string())
    .collect::<Vec<_>>();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/client");
    if let Ok(entries) = fs::read_dir(fixtures) {
        for entry in entries.flatten() {
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok(case) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            for step in case["steps"].as_array().cloned().unwrap_or_default() {
                if let Some(message) = step.get("receive") {
                    messages.push(message.to_string());
                }
            }
        }
    }
    messages
}

fn sample_tool_inputs() -> Vec<String> {
    let mut inputs = Vec::new();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/validate");
    if let Ok(entries) = fs::read_dir(fixtures) {
        for entry in entries.flatten() {
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let Ok(case) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            if let Some(input) = case.get("input") {
                inputs.push(input.to_string());
            }
        }
    }
    inputs
}

fn prefixes(text: &str) -> impl Iterator<Item = &str> {
    (0..=text.len())
        .filter(|end| text.is_char_boundary(*end))
        .map(|end| &text[..end])
}

fn exercise_client(client: &mut Client, input: &str) {
    let batch = client.receive(input);
    let _ = batch.to_json();
    for surface in ["s1", "s2", ""] {
        client.say(surface);
        client.match_voice(surface, input);
        client.match_voice(surface, "ikkinchisi");
        client.match_voice(surface, "ha");
        client.set_value(surface, "/pick", input);
        client.set_value(surface, input, "\"x\"");
        client.fire(surface, "pay", "on_press", Some(input));
        client.fire(surface, "choices", "on_change", None);
        client.confirm(surface, "a2", input);
    }
}

fn validator_parts() -> (CatalogSet, ActionTable, StaticRefs) {
    let actions = ActionTable::new().with(
        "pay_bill",
        ActionSpec::from_value(&json!({ "args": { "bill": { "kind": "string", "required": true } }, "requires_confirm": true })).unwrap(),
    );
    (
        CatalogSet::builtin(),
        actions,
        StaticRefs::new().with("bill", "a", json!("x")),
    )
}

fn exercise_validator(validator: &Validator<'_>, input: &str) {
    let current = Surface {
        id: Some("s1".into()),
        root: "card".into(),
        components: Vec::new(),
        data: Map::new(),
    };
    let _ = validator
        .show_ui_str(input)
        .map(|surface| surface.summary("s1", validator.catalogs(), Locale::Ru));
    let _ = validator.update_ui_str(input, &current);
    let _ = validator.close_ui_str(input);
    #[cfg(feature = "stream")]
    {
        let mut stream = polyui_core::stream::ShowUiStream::new(validator);
        let mut rest = input;
        let mut step = 1;
        while !rest.is_empty() {
            let mut cut = step.min(rest.len());
            while !rest.is_char_boundary(cut) {
                cut += 1;
            }
            let (chunk, tail) = rest.split_at(cut);
            stream.feed(chunk);
            rest = tail;
            step = step % 7 + 1;
        }
        let _ = stream.finish();
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    let (catalogs, actions, refs) = validator_parts();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let mut client = Client::new(&hello()).unwrap();
    for round in 0..400 {
        let length = random.below(300);
        let input = if round % 2 == 0 {
            String::from_utf8_lossy(&random.bytes(length)).into_owned()
        } else {
            random.json_noise(length)
        };
        let _ = protocol::parse(&input);
        let _ = Client::new(&input);
        let _ = Catalog::from_json(&input);
        let _ = ActionSpec::from_json(&input);
        let _ = sanitize::check_url(&input, &["https"]);
        let _ = pointer::parse(&input);
        let mut data = json!({});
        let _ = pointer::set(&mut data, &input, json!(1));
        let _ = pointer::remove(&mut data, &input);
        let _ = pointer::merge(&mut data, &input, json!({ "a": null }));
        let _ = text::render(
            &input,
            &Map::new(),
            std::slice::from_ref(&input),
            Locale::UzCyrl,
        );
        let _ = text::label_from_alt(&input);
        let normalized = voice::normalize(&input);
        let words = voice::tokens(&normalized);
        let _ = voice::ordinal(&words);
        let _ = voice::yes_no(&words);
        let _ = voice::phrase_score(&words, &input);
        exercise_validator(&validator, &input);
        exercise_client(&mut client, &input);
    }
}

#[test]
fn truncated_messages_never_panic() {
    let mut client = Client::new(&hello()).unwrap();
    for message in sample_messages() {
        for prefix in prefixes(&message).step_by(3) {
            exercise_client(&mut client, prefix);
        }
        exercise_client(&mut client, &message);
    }
}

#[test]
fn truncated_tool_inputs_never_panic() {
    let (catalogs, actions, refs) = validator_parts();
    let validator = Validator::new(&catalogs, &actions, &refs);
    for input in sample_tool_inputs() {
        for prefix in prefixes(&input).step_by(3) {
            exercise_validator(&validator, prefix);
        }
    }
}

#[test]
fn mutated_messages_never_panic() {
    let mut random = Random(42);
    let mut client = Client::new(&hello()).unwrap();
    let (catalogs, actions, refs) = validator_parts();
    let validator = Validator::new(&catalogs, &actions, &refs);
    let messages = sample_messages();
    let inputs = sample_tool_inputs();
    for round in 0..600 {
        let source = if round % 2 == 0 {
            &messages[random.below(messages.len())]
        } else {
            &inputs[random.below(inputs.len())]
        };
        let mut bytes = source.clone().into_bytes();
        for _ in 0..1 + random.below(4) {
            if bytes.is_empty() {
                break;
            }
            let position = random.below(bytes.len());
            match random.below(3) {
                0 => bytes[position] = b"{}[]\",:01\\-"[random.below(11)],
                1 => {
                    bytes.remove(position);
                }
                _ => bytes.insert(position, b"{[\"0"[random.below(4)]),
            }
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        exercise_client(&mut client, &text);
        exercise_validator(&validator, &text);
    }
}

#[test]
fn hostile_hello_and_wrappers() {
    for hello in [
        "",
        "null",
        "{\"v\":1,\"t\":\"hello\",\"modalities\":[]}",
        "{\"v\":1,\"t\":\"hello\",\"modalities\":[\"smell\"]}",
        "{\"v\":1,\"t\":\"welcome\",\"session\":\"x\"}",
    ] {
        assert!(Client::new(hello).is_err(), "{hello}");
    }
    let mut client = Client::new(&hello()).unwrap();
    assert_eq!(client.fire_json("s1", "x", "y", ""), "null");
    assert_eq!(client.confirm_json("s1", "x", "y"), "null");
    assert_eq!(client.match_voice_json("s1", "ha"), "null");
    assert_eq!(client.say_json("nope"), "[]");
    let empty: Value = serde_json::from_str(&client.receive_json("{}")).unwrap();
    assert_eq!(empty["send"][0]["reason"], json!("bad_message"));
    let set: Value = serde_json::from_str(&client.set_value_json("s1", "/a", "{")).unwrap();
    assert_eq!(set["ops"], json!([]));
}
