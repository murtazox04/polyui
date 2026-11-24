use std::ffi::{CStr, CString, c_char};
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;

use polyui_ffi::{
    PolyuiClient, polyui_client_confirm, polyui_client_fire, polyui_client_free,
    polyui_client_match_voice, polyui_client_new, polyui_client_new_with_catalogs,
    polyui_client_receive, polyui_client_say, polyui_client_set_value, polyui_string_free,
};
use serde_json::Value;

fn c_text(text: &str) -> CString {
    CString::new(text).expect("fixture text has no NUL")
}

fn take(pointer: *mut c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .expect("output is UTF-8")
        .to_string();
    unsafe { polyui_string_free(pointer) };
    Some(text)
}

fn call(run: impl FnOnce(*mut *mut c_char) -> *mut c_char) -> Result<String, String> {
    let mut error = ptr::null_mut();
    let result = run(&mut error);
    match take(result) {
        Some(text) => {
            assert!(error.is_null(), "a result and an error at once");
            Ok(text)
        }
        None => Err(take(error).expect("a null result always has an error")),
    }
}

struct Session {
    client: *mut PolyuiClient,
}

impl Session {
    fn new(hello: &str) -> Result<Self, String> {
        let hello = c_text(hello);
        let mut error = ptr::null_mut();
        let client = unsafe { polyui_client_new(hello.as_ptr(), false, &mut error) };
        if client.is_null() {
            return Err(take(error).expect("a null client always has an error"));
        }
        assert!(error.is_null());
        Ok(Self { client })
    }

    fn with_catalogs(hello: &str, catalogs: &str) -> Result<Self, String> {
        let (hello, catalogs) = (c_text(hello), c_text(catalogs));
        let mut error = ptr::null_mut();
        let client = unsafe {
            polyui_client_new_with_catalogs(hello.as_ptr(), catalogs.as_ptr(), false, &mut error)
        };
        if client.is_null() {
            return Err(take(error).expect("a null client always has an error"));
        }
        assert!(error.is_null());
        Ok(Self { client })
    }

    fn receive(&self, message: &str) -> Result<String, String> {
        let message = c_text(message);
        call(|error| unsafe { polyui_client_receive(self.client, message.as_ptr(), error) })
    }

    fn set_value(&self, surface: &str, path: &str, value: &str) -> Result<String, String> {
        let (surface, path, value) = (c_text(surface), c_text(path), c_text(value));
        call(|error| unsafe {
            polyui_client_set_value(
                self.client,
                surface.as_ptr(),
                path.as_ptr(),
                value.as_ptr(),
                error,
            )
        })
    }

    fn fire(
        &self,
        surface: &str,
        component: &str,
        prop: &str,
        detail: Option<&str>,
    ) -> Result<String, String> {
        let (surface, component, prop) = (c_text(surface), c_text(component), c_text(prop));
        let detail = detail.map(c_text);
        let detail_pointer = detail.as_ref().map_or(ptr::null(), |text| text.as_ptr());
        call(|error| unsafe {
            polyui_client_fire(
                self.client,
                surface.as_ptr(),
                component.as_ptr(),
                prop.as_ptr(),
                detail_pointer,
                error,
            )
        })
    }

    fn confirm(&self, surface: &str, action_id: &str, nonce: &str) -> Result<String, String> {
        let (surface, action_id, nonce) = (c_text(surface), c_text(action_id), c_text(nonce));
        call(|error| unsafe {
            polyui_client_confirm(
                self.client,
                surface.as_ptr(),
                action_id.as_ptr(),
                nonce.as_ptr(),
                error,
            )
        })
    }

    fn say(&self, surface: &str) -> Result<String, String> {
        let surface = c_text(surface);
        call(|error| unsafe { polyui_client_say(self.client, surface.as_ptr(), error) })
    }

    fn match_voice(&self, surface: &str, text: &str) -> Result<String, String> {
        let (surface, text) = (c_text(surface), c_text(text));
        call(|error| unsafe {
            polyui_client_match_voice(self.client, surface.as_ptr(), text.as_ptr(), error)
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { polyui_client_free(self.client) };
    }
}

fn fixtures() -> Vec<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/client");
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .expect("fixture folder exists")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

fn field(call: &Value, key: &str) -> String {
    call[key].as_str().unwrap_or_default().to_string()
}

fn run_step(session: &Session, step: &Value) -> String {
    let outcome = if let Some(message) = step.get("receive") {
        session.receive(&message.to_string())
    } else if let Some(raw) = step.get("receive_text") {
        session.receive(raw.as_str().unwrap_or_default())
    } else if let Some(call) = step.get("set_value") {
        session.set_value(
            &field(call, "surface"),
            &field(call, "path"),
            &call["value"].to_string(),
        )
    } else if let Some(call) = step.get("fire") {
        let detail = call.get("detail").map(Value::to_string);
        session.fire(
            &field(call, "surface"),
            &field(call, "component"),
            &field(call, "prop"),
            detail.as_deref(),
        )
    } else if let Some(call) = step.get("confirm") {
        session.confirm(
            &field(call, "surface"),
            &field(call, "action_id"),
            &field(call, "nonce"),
        )
    } else if let Some(surface) = step.get("say") {
        session.say(surface.as_str().unwrap_or_default())
    } else if let Some(call) = step.get("match_voice") {
        session.match_voice(&field(call, "surface"), &field(call, "text"))
    } else {
        panic!("unknown step {step}");
    };
    outcome.unwrap_or_else(|error| panic!("step {step} failed: {error}"))
}

#[test]
fn every_client_fixture_replays_through_the_c_abi() {
    let files = fixtures();
    assert!(files.len() >= 10, "expected the client fixtures");
    let mut failures = Vec::new();
    for path in files {
        let case: Value = serde_json::from_str(&fs::read_to_string(&path).expect("fixture reads"))
            .expect("fixture is JSON");
        let session = Session::new(&case["hello"].to_string()).expect("hello is valid");
        for (number, step) in case["steps"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let actual = run_step(&session, step);
            let expected = step["expect"].to_string();
            if actual != expected {
                failures.push(format!(
                    "{} step {}\n  expected: {expected}\n  actual:   {actual}",
                    path.display(),
                    number + 1
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const HELLO: &str = r#"{"v":1,"t":"hello","client":{"name":"ffi-test","version":"1"},"catalogs":["core@1","chart@1"],"modalities":["screen","voice"],"locale":"en"}"#;

#[test]
fn a_bad_hello_is_an_error_not_a_client() {
    let error = Session::new("{\"v\":1}").err().expect("hello is rejected");
    assert!(
        error.starts_with("PolyUI could not start a client from this hello"),
        "{error}"
    );
    let error = Session::new("not json").err().expect("hello is rejected");
    assert!(
        error.starts_with("PolyUI could not start a client"),
        "{error}"
    );
}

#[test]
fn null_hello_is_reported() {
    let mut error = ptr::null_mut();
    let client = unsafe { polyui_client_new(ptr::null(), false, &mut error) };
    assert!(client.is_null());
    assert_eq!(
        take(error).as_deref(),
        Some("PolyUI client_new: the hello is null")
    );
}

#[test]
fn null_client_is_reported_for_every_call() {
    let text = c_text("s1");
    let pointer = text.as_ptr();
    let null = ptr::null_mut();
    let errors = unsafe {
        [
            call(|error| polyui_client_receive(null, pointer, error)),
            call(|error| polyui_client_set_value(null, pointer, pointer, pointer, error)),
            call(|error| polyui_client_fire(null, pointer, pointer, pointer, ptr::null(), error)),
            call(|error| polyui_client_confirm(null, pointer, pointer, pointer, error)),
            call(|error| polyui_client_say(null, pointer, error)),
            call(|error| polyui_client_match_voice(null, pointer, pointer, error)),
        ]
    };
    for outcome in errors {
        let message = outcome.expect_err("null client fails");
        assert!(message.ends_with("the client is null"), "{message}");
    }
}

#[test]
fn null_arguments_are_named() {
    let session = Session::new(HELLO).expect("client");
    let surface = c_text("s1");
    let client = session.client;
    let receive = call(|error| unsafe { polyui_client_receive(client, ptr::null(), error) });
    assert_eq!(receive.unwrap_err(), "PolyUI receive: the message is null");
    let set_value = call(|error| unsafe {
        polyui_client_set_value(
            client,
            surface.as_ptr(),
            ptr::null(),
            surface.as_ptr(),
            error,
        )
    });
    assert_eq!(set_value.unwrap_err(), "PolyUI set_value: the path is null");
    let confirm = call(|error| unsafe {
        polyui_client_confirm(
            client,
            surface.as_ptr(),
            surface.as_ptr(),
            ptr::null(),
            error,
        )
    });
    assert_eq!(confirm.unwrap_err(), "PolyUI confirm: the nonce is null");
}

#[test]
fn invalid_utf8_is_rejected() {
    let session = Session::new(HELLO).expect("client");
    let broken = CString::new(vec![0x7b, 0xff, 0xfe, 0x7d]).expect("no NUL");
    let client = session.client;
    let outcome = call(|error| unsafe { polyui_client_receive(client, broken.as_ptr(), error) });
    assert_eq!(
        outcome.unwrap_err(),
        "PolyUI receive: the message is not valid UTF-8"
    );
    assert_eq!(session.say("s1").as_deref(), Ok("[]"));
}

#[test]
fn a_message_that_is_not_json_answers_with_an_error_message() {
    let session = Session::new(HELLO).expect("client");
    let batch: Value =
        serde_json::from_str(&session.receive("{oops").expect("batch")).expect("JSON");
    assert_eq!(batch["send"][0]["t"], "ui.error");
    assert_eq!(batch["send"][0]["reason"], "bad_message");
}

#[test]
fn fire_detail_rules_match_the_wasm_binding() {
    let session = Session::new(HELLO).expect("client");
    session
        .receive(r#"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"b","components":[{"id":"b","type":"Button","props":{"label":"Go","on_press":{"action_id":"act_go","confirm":false}}}],"data":{}}"#)
        .expect("open");
    let plain = session.fire("s1", "b", "on_press", None).expect("fires");
    let blank = session
        .fire("s1", "b", "on_press", Some("   "))
        .expect("fires");
    assert_eq!(plain, blank);
    assert!(plain.contains("\"action_id\":\"act_go\""));
    let error = session
        .fire("s1", "b", "on_press", Some("{bad"))
        .unwrap_err();
    assert!(
        error.starts_with("PolyUI fire: the detail is not valid JSON"),
        "{error}"
    );
    assert_eq!(
        session.fire("s1", "missing", "on_press", None).as_deref(),
        Ok("null")
    );
}

#[test]
fn unknown_surfaces_return_json_null_or_empty_values() {
    let session = Session::new(HELLO).expect("client");
    assert_eq!(session.say("nope").as_deref(), Ok("[]"));
    assert_eq!(session.match_voice("nope", "first").as_deref(), Ok("null"));
    assert_eq!(session.confirm("nope", "a", "n").as_deref(), Ok("null"));
    let batch: Value =
        serde_json::from_str(&session.set_value("nope", "/a", "1").expect("batch")).expect("JSON");
    assert_eq!(batch["surface"], "nope");
}

#[test]
fn large_and_deep_inputs_do_not_crash() {
    let session = Session::new(HELLO).expect("client");
    let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    let batch = session.receive(&deep).expect("a batch, not a crash");
    assert!(batch.contains("ui.error"));
    let wide = format!(
        "{{\"v\":1,\"t\":\"ui.open\",\"surface\":\"s1\",\"seq\":1,\"root\":\"t\",\"components\":[{{\"id\":\"t\",\"type\":\"Text\",\"props\":{{\"text\":\"{}\"}}}}],\"data\":{{}}}}",
        "x".repeat(2_000_000)
    );
    assert!(session.receive(&wide).is_ok());
    assert!(session.set_value("s1", "/a", &deep).is_err());
}

#[test]
fn clients_are_independent_and_can_move_between_threads() {
    let handles: Vec<_> = (0..4)
        .map(|index| {
            std::thread::spawn(move || {
                let session = Session::new(HELLO).expect("client");
                let message = format!(
                    r#"{{"v":1,"t":"ui.open","surface":"s{index}","seq":1,"root":"t","components":[{{"id":"t","type":"Text","props":{{"text":"hi {index}"}}}}],"data":{{}}}}"#
                );
                session.receive(&message).expect("batch");
                session.say(&format!("s{index}")).expect("utterances")
            })
        })
        .collect();
    for (index, handle) in handles.into_iter().enumerate() {
        let said = handle.join().expect("thread finished");
        assert!(
            said.contains(&format!("Hi {index}")) || said.contains(&format!("hi {index}")),
            "{said}"
        );
    }
}

const ACME_HELLO: &str = r#"{"v":1,"t":"hello","client":{"name":"ffi-test","version":"1"},"catalogs":["core@1","acme@1"],"modalities":["screen"],"locale":"en"}"#;

const ACME_CATALOGS: &str = r#"[{"id":"acme","version":1,"types":{"Badge2":{"description":"A status badge.","role":"status","props":{"text":{"kind":"string","required":true},"tone":{"kind":"enum","values":["good","bad"],"default":"good"}},"alt":"{text}","say":"{text}"}}}]"#;

const ACME_OPEN: &str = r#"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"card","components":[{"id":"card","type":"Card","props":{"title":"Bill"},"children":["badge"]},{"id":"badge","type":"acme:Badge2","props":{"text":"Paid"}}],"data":{}}"#;

fn op_for(batch: &str, id: &str) -> Value {
    let batch: Value = serde_json::from_str(batch).expect("batch is JSON");
    batch["ops"]
        .as_array()
        .expect("ops")
        .iter()
        .find(|op| op["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no op for {id} in {batch}"))
}

#[test]
fn extension_types_render_only_when_their_catalog_is_passed() {
    let plain = Session::new(ACME_HELLO).expect("client");
    let fallback = op_for(&plain.receive(ACME_OPEN).expect("batch"), "badge");
    assert_eq!(fallback["type"], "Text", "{fallback}");

    let extended = Session::with_catalogs(ACME_HELLO, ACME_CATALOGS).expect("client");
    let batch = extended.receive(ACME_OPEN).expect("batch");
    let badge = op_for(&batch, "badge");
    assert_eq!(badge["type"], "acme:Badge2", "{badge}");
    assert_eq!(badge["props"]["text"], "Paid", "{badge}");
    assert_eq!(badge["props"]["tone"], "good", "{badge}");
    assert!(!batch.contains("unknown_component"), "{batch}");
    assert!(extended.say("s1").expect("utterances").contains("Paid"));
}

#[test]
fn an_empty_extension_list_matches_client_new() {
    let plain = Session::new(HELLO).expect("client");
    let extended = Session::with_catalogs(HELLO, "[]").expect("client");
    let message = r#"{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"t","components":[{"id":"t","type":"Text","props":{"text":"hi"}}],"data":{}}"#;
    assert_eq!(plain.receive(message), extended.receive(message));
}

#[test]
fn an_undeclared_extension_catalog_is_dropped_like_the_core_does() {
    let session = Session::with_catalogs(HELLO, ACME_CATALOGS).expect("client");
    let fallback = op_for(&session.receive(ACME_OPEN).expect("batch"), "badge");
    assert_eq!(fallback["type"], "Text", "{fallback}");
}

#[test]
fn bad_extension_catalogs_are_named() {
    let prefix = "PolyUI client_new_with_catalogs: an extension catalog was rejected: ";
    let cases = [
        ("{nope", "invalid JSON"),
        (r#"{"id":"acme"}"#, "must be a list of catalogs"),
        (
            r#"[{"id":"acme","version":1,"types":{"badge":{"description":"x","role":"status"}}}]"#,
            "[0].types.badge: type names must be PascalCase",
        ),
        (
            r#"[{"id":"chart","version":1,"types":{}}]"#,
            r#"[0].id: "chart" is reserved"#,
        ),
        (
            r#"[{"id":"acme","version":1,"types":{}},{"id":"acme","version":1,"types":{}}]"#,
            r#"[1].id: catalog "acme" is already in the set"#,
        ),
        (
            r#"[{"id":"acme","version":1,"types":{"Badge2":{"description":"x","role":"status","props":{"tone":{"kind":"enum","values":["good"],"default":"bad"}}}}}]"#,
            "[0].types.Badge2.props.tone.default: default does not fit the prop",
        ),
    ];
    for (catalogs, expected) in cases {
        let error = Session::with_catalogs(ACME_HELLO, catalogs)
            .err()
            .unwrap_or_else(|| panic!("{catalogs} was accepted"));
        assert!(error.starts_with(prefix), "{error}");
        assert!(error.contains(expected), "{catalogs}: {error}");
    }
}

#[test]
fn a_bad_hello_is_still_reported_with_catalogs() {
    let error = Session::with_catalogs("{\"v\":1}", ACME_CATALOGS)
        .err()
        .expect("hello is rejected");
    assert!(
        error.starts_with("PolyUI could not start a client from this hello"),
        "{error}"
    );
}

#[test]
fn null_catalogs_are_reported() {
    let hello = c_text(ACME_HELLO);
    let mut error = ptr::null_mut();
    let client =
        unsafe { polyui_client_new_with_catalogs(hello.as_ptr(), ptr::null(), false, &mut error) };
    assert!(client.is_null());
    assert_eq!(
        take(error).as_deref(),
        Some("PolyUI client_new_with_catalogs: the catalog list is null")
    );
}
