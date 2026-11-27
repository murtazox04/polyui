#![allow(clippy::missing_safety_doc)]

#[cfg(panic = "abort")]
compile_error!("polyui-ffi catches panics at the boundary, build it with panic = \"unwind\"");

use std::any::Any;
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use polyui_core::{CatalogSet, Client};

mod embed;

pub struct PolyuiClient {
    core: Client,
    busy: bool,
}

type Outcome = Result<String, String>;

#[cfg(feature = "lite")]
fn json_problem(input: &str) -> Option<String> {
    polyui_core::json_error(input)
}

#[cfg(not(feature = "lite"))]
fn json_problem(input: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(input)
        .err()
        .map(|error| error.to_string())
}

fn require_json(input: &str, call: &str, what: &str) -> Result<(), String> {
    match json_problem(input) {
        Some(error) => Err(format!("PolyUI {call}: {what} is not valid JSON ({error})")),
        None => Ok(()),
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string());
    format!("PolyUI core panicked: {detail}")
}

unsafe fn text_arg<'a>(pointer: *const c_char, call: &str, name: &str) -> Result<&'a str, String> {
    if pointer.is_null() {
        return Err(format!("PolyUI {call}: {name} is null"));
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map_err(|_| format!("PolyUI {call}: {name} is not valid UTF-8"))
}

unsafe fn optional_text_arg<'a>(
    pointer: *const c_char,
    call: &str,
    name: &str,
) -> Result<Option<&'a str>, String> {
    if pointer.is_null() {
        Ok(None)
    } else {
        unsafe { text_arg(pointer, call, name) }.map(Some)
    }
}

fn owned(text: String) -> *mut c_char {
    let clean = if text.contains('\0') {
        text.replace('\0', "\u{fffd}")
    } else {
        text
    };
    CString::new(clean).map_or(ptr::null_mut(), CString::into_raw)
}

unsafe fn guarded<T>(
    error_out: *mut *mut c_char,
    failed: T,
    body: impl FnOnce() -> Result<T, String>,
) -> T {
    if !error_out.is_null() {
        unsafe { error_out.write(ptr::null_mut()) };
    }
    let outcome =
        catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|payload| Err(panic_message(payload)));
    match outcome {
        Ok(value) => value,
        Err(message) => {
            if !error_out.is_null() {
                unsafe { error_out.write(owned(message)) };
            }
            failed
        }
    }
}

unsafe fn with_client(
    client: *mut PolyuiClient,
    error_out: *mut *mut c_char,
    call: &str,
    body: impl FnOnce(&mut Client) -> Outcome,
) -> *mut c_char {
    unsafe {
        guarded(error_out, ptr::null_mut(), || {
            let client = client
                .as_mut()
                .ok_or_else(|| format!("PolyUI {call}: the client is null"))?;
            if client.busy {
                return Err(format!(
                    "PolyUI {call}: the client is unusable after an earlier panic or is already in a call"
                ));
            }
            client.busy = true;
            let outcome = body(&mut client.core);
            client.busy = false;
            outcome.map(owned)
        })
    }
}

fn start_client(
    hello: &str,
    catalogs: CatalogSet,
    allow_http: bool,
) -> Result<*mut PolyuiClient, String> {
    let core = Client::with_catalogs(hello, catalogs)
        .map_err(|error| format!("PolyUI could not start a client from this hello: {error}"))?;
    Ok(Box::into_raw(Box::new(PolyuiClient {
        core: core.with_allow_http(allow_http),
        busy: false,
    })))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_client_new(
    hello_json: *const c_char,
    allow_http: bool,
    error_out: *mut *mut c_char,
) -> *mut PolyuiClient {
    unsafe {
        guarded(error_out, ptr::null_mut(), || {
            let hello = text_arg(hello_json, "client_new", "the hello")?;
            start_client(hello, CatalogSet::builtin().with_embed(), allow_http)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_client_new_with_catalogs(
    hello_json: *const c_char,
    catalogs_json: *const c_char,
    allow_http: bool,
    error_out: *mut *mut c_char,
) -> *mut PolyuiClient {
    const CALL: &str = "client_new_with_catalogs";
    unsafe {
        guarded(error_out, ptr::null_mut(), || {
            let hello = text_arg(hello_json, CALL, "the hello")?;
            let extensions = text_arg(catalogs_json, CALL, "the catalog list")?;
            let catalogs = CatalogSet::builtin()
                .with_embed()
                .with_extensions_json(extensions)
                .map_err(|error| {
                    format!("PolyUI {CALL}: an extension catalog was rejected: {error}")
                })?;
            start_client(hello, catalogs, allow_http)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_client_receive(
    client: *mut PolyuiClient,
    message: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        with_client(client, error_out, "receive", |core| {
            let message = text_arg(message, "receive", "the message")?;
            Ok(core.receive_json(message))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_client_set_value(
    client: *mut PolyuiClient,
    surface: *const c_char,
    path: *const c_char,
    value_json: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        with_client(client, error_out, "set_value", |core| {
            let surface = text_arg(surface, "set_value", "the surface")?;
            let path = text_arg(path, "set_value", "the path")?;
            let value = text_arg(value_json, "set_value", "the value")?;
            require_json(value, "set_value", "the value")?;
            Ok(core.set_value_json(surface, path, value))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_client_fire(
    client: *mut PolyuiClient,
    surface: *const c_char,
    component: *const c_char,
    prop: *const c_char,
    detail_json: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        with_client(client, error_out, "fire", |core| {
            let surface = text_arg(surface, "fire", "the surface")?;
            let component = text_arg(component, "fire", "the component")?;
            let prop = text_arg(prop, "fire", "the prop")?;
            let detail = optional_text_arg(detail_json, "fire", "the detail")?
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_default();
            if !detail.is_empty() {
                require_json(detail, "fire", "the detail")?;
            }
            Ok(core.fire_json(surface, component, prop, detail))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_client_confirm(
    client: *mut PolyuiClient,
    surface: *const c_char,
    action_id: *const c_char,
    nonce: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        with_client(client, error_out, "confirm", |core| {
            let surface = text_arg(surface, "confirm", "the surface")?;
            let action_id = text_arg(action_id, "confirm", "the action id")?;
            let nonce = text_arg(nonce, "confirm", "the nonce")?;
            Ok(core.confirm_json(surface, action_id, nonce))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_client_say(
    client: *mut PolyuiClient,
    surface: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        with_client(client, error_out, "say", |core| {
            let surface = text_arg(surface, "say", "the surface")?;
            Ok(core.say_json(surface))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_client_match_voice(
    client: *mut PolyuiClient,
    surface: *const c_char,
    text: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        with_client(client, error_out, "match_voice", |core| {
            let surface = text_arg(surface, "match_voice", "the surface")?;
            let spoken = text_arg(text, "match_voice", "the text")?;
            Ok(core.match_voice_json(surface, spoken))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_embed_page(
    props_json: *const c_char,
    nonce: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        guarded(error_out, ptr::null_mut(), || {
            let props = text_arg(props_json, "embed_page", "the props")?;
            let nonce = text_arg(nonce, "embed_page", "the nonce")?;
            embed::embed_page(props, nonce).map(owned)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_embed_page_with(
    props_json: *const c_char,
    nonce: *const c_char,
    host_json: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    unsafe {
        guarded(error_out, ptr::null_mut(), || {
            let props = text_arg(props_json, "embed_page", "the props")?;
            let nonce = text_arg(nonce, "embed_page", "the nonce")?;
            let host = text_arg(host_json, "embed_page", "the host options")?;
            embed::embed_page_with(props, nonce, host).map(owned)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_string_free(text: *mut c_char) {
    if text.is_null() {
        return;
    }
    let reclaimed = unsafe { CString::from_raw(text) };
    let _ = catch_unwind(AssertUnwindSafe(move || drop(reclaimed)));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn polyui_client_free(client: *mut PolyuiClient) {
    if client.is_null() {
        return;
    }
    let reclaimed = unsafe { Box::from_raw(client) };
    let _ = catch_unwind(AssertUnwindSafe(move || drop(reclaimed)));
}

#[cfg(test)]
mod tests;
