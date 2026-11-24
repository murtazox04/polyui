use std::ffi::{CStr, CString};
use std::io::Read;
use std::ptr;

fn main() {
    let mut props = String::new();
    std::io::stdin()
        .read_to_string(&mut props)
        .expect("props JSON on stdin");
    let nonce = std::env::args()
        .nth(1)
        .expect("usage: embed_page NONCE < props.json");
    let props = CString::new(props).expect("props without NUL");
    let nonce = CString::new(nonce).expect("nonce without NUL");
    let mut error = ptr::null_mut();
    let html = unsafe { polyui_ffi::polyui_embed_page(props.as_ptr(), nonce.as_ptr(), &mut error) };
    if html.is_null() {
        let message = unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned();
        unsafe { polyui_ffi::polyui_string_free(error) };
        eprintln!("{message}");
        std::process::exit(1);
    }
    print!("{}", unsafe { CStr::from_ptr(html) }.to_string_lossy());
    unsafe { polyui_ffi::polyui_string_free(html) };
}
