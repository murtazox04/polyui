use super::*;

const HELLO: &str = r#"{"v":1,"t":"hello","client":{"name":"ffi-test","version":"1"},"catalogs":["core@1"],"modalities":["screen"],"locale":"en"}"#;

fn c_text(text: &str) -> CString {
    CString::new(text).expect("test text has no NUL")
}

unsafe fn take(pointer: *mut c_char) -> Option<String> {
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

fn new_client() -> *mut PolyuiClient {
    let hello = c_text(HELLO);
    let mut error = ptr::null_mut();
    let client = unsafe { polyui_client_new(hello.as_ptr(), false, &mut error) };
    assert!(!client.is_null());
    assert!(error.is_null());
    client
}

#[test]
fn panics_become_errors() {
    let mut error = ptr::null_mut();
    let value: i32 = unsafe { guarded(&mut error, -1, || panic!("boom")) };
    assert_eq!(value, -1);
    let message = unsafe { take(error) }.expect("error is set");
    assert_eq!(message, "PolyUI core panicked: boom");
}

#[test]
fn panics_with_owned_payload_keep_their_message() {
    let mut error = ptr::null_mut();
    let detail = String::from("owned boom");
    let value: i32 = unsafe { guarded(&mut error, 0, move || std::panic::panic_any(detail)) };
    assert_eq!(value, 0);
    assert_eq!(
        unsafe { take(error) }.as_deref(),
        Some("PolyUI core panicked: owned boom")
    );
}

#[test]
fn a_panic_inside_a_call_poisons_the_client() {
    let client = new_client();
    let mut error = ptr::null_mut();
    let result = unsafe {
        with_client(client, &mut error, "receive", |_| -> Outcome {
            panic!("inside the core")
        })
    };
    assert!(result.is_null());
    assert!(unsafe { take(error) }.is_some_and(|message| message.contains("inside the core")));
    let surface = c_text("s1");
    let said = unsafe { polyui_client_say(client, surface.as_ptr(), &mut error) };
    assert!(said.is_null());
    assert!(unsafe { take(error) }.is_some_and(|message| message.contains("unusable")));
    unsafe { polyui_client_free(client) };
}

#[test]
fn an_error_inside_a_call_does_not_poison_the_client() {
    let client = new_client();
    let mut error = ptr::null_mut();
    let surface = c_text("s1");
    let path = c_text("/a");
    let broken = c_text("{nope");
    let result = unsafe {
        polyui_client_set_value(
            client,
            surface.as_ptr(),
            path.as_ptr(),
            broken.as_ptr(),
            &mut error,
        )
    };
    assert!(result.is_null());
    assert!(unsafe { take(error) }.is_some_and(|message| {
        message.starts_with("PolyUI set_value: the value is not valid JSON")
    }));
    let said = unsafe { polyui_client_say(client, surface.as_ptr(), &mut error) };
    assert_eq!(unsafe { take(said) }.as_deref(), Some("[]"));
    assert!(error.is_null());
    unsafe { polyui_client_free(client) };
}

#[test]
fn error_out_may_be_null() {
    let surface = c_text("s1");
    let result = unsafe { polyui_client_say(ptr::null_mut(), surface.as_ptr(), ptr::null_mut()) };
    assert!(result.is_null());
}

#[test]
fn success_clears_a_stale_error_pointer() {
    let client = new_client();
    let surface = c_text("s1");
    let mut error: *mut c_char = ptr::dangling_mut();
    let said = unsafe { polyui_client_say(client, surface.as_ptr(), &mut error) };
    assert!(error.is_null());
    assert_eq!(unsafe { take(said) }.as_deref(), Some("[]"));
    unsafe { polyui_client_free(client) };
}

#[test]
fn owned_never_fails_on_nul() {
    let pointer = owned(String::from("a\0b"));
    assert_eq!(unsafe { take(pointer) }.as_deref(), Some("a\u{fffd}b"));
}

#[test]
fn freeing_null_is_a_no_op() {
    unsafe {
        polyui_string_free(ptr::null_mut());
        polyui_client_free(ptr::null_mut());
    }
}

const NONCE: &str = "abcdefghijklmnop0123";

fn page(props: &str, nonce: &str) -> Result<String, String> {
    let props = c_text(props);
    let nonce = c_text(nonce);
    let mut error = ptr::null_mut();
    let html = unsafe {
        take(polyui_embed_page(
            props.as_ptr(),
            nonce.as_ptr(),
            &mut error,
        ))
    };
    match html {
        Some(html) => Ok(html),
        None => Err(unsafe { take(error) }.expect("error is set")),
    }
}

#[test]
fn embed_page_carries_props_and_nonce() {
    let html = page(r#"{"resource":"ui://a/b","title":"Forecast"}"#, NONCE).expect("page");
    assert!(html.contains(&format!("const nonce = \"{NONCE}\";")));
    assert!(html.contains(r#"let props = {"resource":"ui://a/b","title":"Forecast"};"#));
    assert!(!html.contains("POLYUI_NONCE_SLOT"));
    assert!(!html.contains("POLYUI_PROPS_SLOT"));
}

#[test]
fn embed_page_props_cannot_close_the_script() {
    let props = "{\"title\":\"</script><script>alert(1)</script>\",\"note\":\"a & b > c\",\"line\":\"x\u{2028}y\u{2029}z\"}";
    let html = page(props, NONCE).expect("page");
    assert_eq!(html.matches("</script>").count(), 1);
    assert!(html.contains(r"\u003c/script\u003e\u003cscript\u003ealert(1)\u003c/script\u003e"));
    assert!(html.contains(r"a \u0026 b \u003e c"));
    assert!(html.contains(r"x\u2028y\u2029z"));
    assert!(!html.contains('\u{2028}'));
    assert!(!html.contains('\u{2029}'));
}

#[test]
fn embed_page_props_slot_text_is_not_expanded() {
    let html = page(r#"{"title":"POLYUI_NONCE_SLOT POLYUI_PROPS_SLOT"}"#, NONCE).expect("page");
    assert!(html.contains(r#"{"title":"POLYUI_NONCE_SLOT POLYUI_PROPS_SLOT"}"#));
    assert!(html.contains(&format!("const nonce = \"{NONCE}\";")));
}

fn page_with(props: &str, host: &str) -> Result<String, String> {
    let props = c_text(props);
    let nonce = c_text(NONCE);
    let host = c_text(host);
    let mut error = ptr::null_mut();
    let html = unsafe {
        take(polyui_embed_page_with(
            props.as_ptr(),
            nonce.as_ptr(),
            host.as_ptr(),
            &mut error,
        ))
    };
    match html {
        Some(html) => Ok(html),
        None => Err(unsafe { take(error) }.expect("error is set")),
    }
}

#[test]
fn embed_page_takes_app_calls_only_from_the_host() {
    let off = page(r#"{"title":"Forecast"}"#, NONCE).expect("page");
    assert!(off.contains(r#"const host = {"app_calls":false};"#));
    let on = page_with(
        r#"{"title":"Forecast"}"#,
        r#"{"app_calls":true,"other":"x"}"#,
    )
    .expect("page");
    assert!(on.contains(r#"const host = {"app_calls":true};"#));
    assert!(!on.contains("POLYUI_HOST_SLOT"));
    let props_text =
        page_with(r#"{"title":"POLYUI_HOST_SLOT"}"#, r#"{"app_calls":true}"#).expect("page");
    assert!(props_text.contains(r#"{"title":"POLYUI_HOST_SLOT"}"#));
    assert!(
        page_with("{}", r#"{"app_calls":"yes"}"#)
            .unwrap_err()
            .contains("true or false")
    );
    assert!(
        page_with("{}", "[true]")
            .unwrap_err()
            .contains("JSON object")
    );
    assert!(
        page_with("{}", "{nope")
            .unwrap_err()
            .contains("not valid JSON")
    );
}

#[test]
fn embed_page_rejects_bad_input() {
    assert!(page("{}", "short").unwrap_err().contains("nonce"));
    assert!(
        page("{}", "abcdefghijklmnop\"0123")
            .unwrap_err()
            .contains("nonce")
    );
    assert!(page("{}", &"a".repeat(129)).unwrap_err().contains("nonce"));
    assert!(
        page("{not json", NONCE)
            .unwrap_err()
            .contains("not valid JSON")
    );
    assert!(page("[1]", NONCE).unwrap_err().contains("JSON object"));
    let mut error = ptr::null_mut();
    let nonce = c_text(NONCE);
    assert!(unsafe { polyui_embed_page(ptr::null(), nonce.as_ptr(), &mut error) }.is_null());
    assert!(
        unsafe { take(error) }
            .expect("error")
            .contains("the props is null")
    );
}
