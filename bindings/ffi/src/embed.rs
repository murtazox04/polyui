const PAGE: &str = include_str!("embed-page.html");
const NONCE_SLOT: &str = "POLYUI_NONCE_SLOT";
const PROPS_SLOT: &str = "POLYUI_PROPS_SLOT";
const SHORTEST_NONCE: usize = 16;
const LONGEST_NONCE: usize = 128;

fn script_safe(json: &str) -> String {
    let mut out = String::with_capacity(json.len() + json.len() / 16);
    for character in json.chars() {
        match character {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            other => out.push(other),
        }
    }
    out
}

pub(crate) fn embed_page(props_json: &str, nonce: &str) -> Result<String, String> {
    let nonce_fits = (SHORTEST_NONCE..=LONGEST_NONCE).contains(&nonce.len())
        && nonce.bytes().all(|byte| byte.is_ascii_alphanumeric());
    if !nonce_fits {
        return Err(format!(
            "PolyUI embed_page: the nonce must be {SHORTEST_NONCE} to {LONGEST_NONCE} ASCII letters and digits"
        ));
    }
    crate::require_json(props_json, "embed_page", "the props")?;
    if !props_json.trim_start().starts_with('{') {
        return Err("PolyUI embed_page: the props must be a JSON object".to_string());
    }
    Ok(PAGE
        .replacen(NONCE_SLOT, nonce, 1)
        .replacen(PROPS_SLOT, &script_safe(props_json), 1))
}
