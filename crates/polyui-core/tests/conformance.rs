use polyui_core::validate::{ActionSpec, ActionTable, StaticRefs, Surface, Validator};
use polyui_core::{Catalog, CatalogSet, Client, Component, Locale};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture_files(kind: &str) -> Vec<PathBuf> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/conformance")
        .join(kind);
    let mut files: Vec<PathBuf> = fs::read_dir(&directory)
        .expect("fixture directory exists")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    files
}

fn load(path: &Path) -> Value {
    let text = fs::read_to_string(path).expect("fixture is readable");
    serde_json::from_str(&text).expect("fixture is JSON")
}

fn blessing() -> bool {
    std::env::var_os("POLYUI_BLESS").is_some()
}

fn save(path: &Path, case: &Value) {
    let mut text = serde_json::to_string_pretty(case).expect("fixture serializes");
    text.push('\n');
    fs::write(path, text).expect("fixture is writable");
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn surface_from(value: &Value) -> Surface {
    Surface {
        id: None,
        root: value["root"].as_str().unwrap_or_default().to_string(),
        components: serde_json::from_value::<Vec<Component>>(value["components"].clone())
            .expect("current components parse"),
        data: value["data"].as_object().cloned().unwrap_or_default(),
    }
}

fn run_validate(case: &Value) -> Value {
    let mut catalogs = CatalogSet::builtin();
    if let Some(extension) = case.get("extension") {
        let catalog = Catalog::from_value(extension).expect("extension catalog parses");
        catalogs = catalogs
            .with(catalog)
            .expect("extension catalog is accepted");
    }
    if let Some(declared) = case.get("catalogs") {
        catalogs = catalogs.restrict(&strings(declared));
    }
    let mut actions = ActionTable::new();
    for (name, spec) in case["actions"].as_object().cloned().unwrap_or_default() {
        actions.register(
            &name,
            ActionSpec::from_value(&spec).expect("action spec parses"),
        );
    }
    let mut refs = StaticRefs::new();
    for (source, entries) in case["refs"].as_object().cloned().unwrap_or_default() {
        for (key, value) in entries.as_object().cloned().unwrap_or_default() {
            refs = refs.with(&source, &key, value);
        }
    }
    let validator = Validator::new(&catalogs, &actions, &refs)
        .allow_http(case["allow_http"].as_bool().unwrap_or(false));
    let outcome = match case["tool"].as_str().unwrap_or("show_ui") {
        "update_ui" => {
            let mut current = surface_from(&case["current"]);
            validator.update_ui(&case["input"], &current).map(|update| {
                current.apply(&update).expect("valid update applies");
                json!({ "ok": true, "summary": current.summary("s1", &catalogs, Locale::En) })
            })
        }
        "close_ui" => validator.close_ui(&case["input"]).map(
            |surface| json!({ "ok": true, "summary": polyui_core::summary::closed(&surface) }),
        ),
        _ => {
            let result = match case.get("input_text").and_then(Value::as_str) {
                Some(text) => validator.show_ui_str(text),
                None => validator.show_ui(&case["input"]),
            };
            result.map(|surface| {
                json!({
                    "ok": true,
                    "summary": surface.summary("s1", &catalogs, Locale::En),
                    "fallback": surface.fallback(&catalogs, Locale::En),
                })
            })
        }
    };
    match outcome {
        Ok(value) => value,
        Err(problems) => json!({ "problems": problems.lines() }),
    }
}

fn run_client(case: &Value) -> Vec<Value> {
    let mut client = Client::new(&case["hello"].to_string()).expect("hello is valid");
    let mut results = Vec::new();
    for step in case["steps"].as_array().cloned().unwrap_or_default() {
        let text = |key: &str| step[key].as_str().unwrap_or_default().to_string();
        let result = if let Some(message) = step.get("receive") {
            client.receive(&message.to_string()).to_value()
        } else if let Some(raw) = step.get("receive_text") {
            client.receive(raw.as_str().unwrap_or_default()).to_value()
        } else if let Some(call) = step.get("set_value") {
            client
                .set_value(
                    call["surface"].as_str().unwrap_or_default(),
                    call["path"].as_str().unwrap_or_default(),
                    &call["value"].to_string(),
                )
                .to_value()
        } else if let Some(call) = step.get("fire") {
            let detail = call.get("detail").map(Value::to_string);
            client
                .fire(
                    call["surface"].as_str().unwrap_or_default(),
                    call["component"].as_str().unwrap_or_default(),
                    call["prop"].as_str().unwrap_or_default(),
                    detail.as_deref(),
                )
                .unwrap_or(Value::Null)
        } else if let Some(call) = step.get("confirm") {
            client
                .confirm(
                    call["surface"].as_str().unwrap_or_default(),
                    call["action_id"].as_str().unwrap_or_default(),
                    call["nonce"].as_str().unwrap_or_default(),
                )
                .unwrap_or(Value::Null)
        } else if step.get("say").is_some() {
            serde_json::to_value(client.say(&text("say"))).expect("utterances serialize")
        } else if let Some(call) = step.get("match_voice") {
            serde_json::to_value(client.match_voice(
                call["surface"].as_str().unwrap_or_default(),
                call["text"].as_str().unwrap_or_default(),
            ))
            .expect("voice match serializes")
        } else {
            panic!("unknown step {step}");
        };
        results.push(result);
    }
    results
}

#[test]
fn validate_fixtures() {
    let files = fixture_files("validate");
    assert!(files.len() >= 25, "expected at least 25 validate fixtures");
    let mut failures = Vec::new();
    for path in files {
        let mut case = load(&path);
        let actual = run_validate(&case);
        if blessing() {
            case["expect"] = actual;
            save(&path, &case);
        } else if case["expect"] != actual {
            failures.push(format!(
                "{}\n  expected: {}\n  actual:   {}",
                path.display(),
                case["expect"],
                actual
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn client_fixtures() {
    let files = fixture_files("client");
    assert!(files.len() >= 10, "expected at least 10 client fixtures");
    let mut failures = Vec::new();
    for path in files {
        let mut case = load(&path);
        let actual = run_client(&case);
        if blessing() {
            if let Some(steps) = case["steps"].as_array_mut() {
                for (step, result) in steps.iter_mut().zip(actual) {
                    step["expect"] = result;
                }
            }
            save(&path, &case);
            continue;
        }
        let steps = case["steps"].as_array().cloned().unwrap_or_default();
        for (number, (step, result)) in steps.iter().zip(actual).enumerate() {
            if step["expect"] != result {
                failures.push(format!(
                    "{} step {}\n  expected: {}\n  actual:   {}",
                    path.display(),
                    number + 1,
                    step["expect"],
                    result
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_fixture_is_described_and_has_expectations() {
    for path in fixture_files("validate") {
        let case = load(&path);
        assert!(case["description"].is_string(), "{}", path.display());
        assert!(!case["expect"].is_null(), "{}", path.display());
    }
    for path in fixture_files("client") {
        let case = load(&path);
        assert!(case["description"].is_string(), "{}", path.display());
        let steps = case["steps"].as_array().cloned().unwrap_or_default();
        assert!(!steps.is_empty(), "{}", path.display());
        assert!(
            steps.iter().all(|step| step.get("expect").is_some()),
            "{}",
            path.display()
        );
    }
}
