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
