pub fn random_id(prefix: &str, bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).expect("the system random source failed");
    let mut out = String::with_capacity(prefix.len() + bytes * 2);
    out.push_str(prefix);
    for byte in buffer {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn session_id() -> String {
    random_id("sess_", 16)
}

pub fn action_id() -> String {
    random_id("act_", 8)
}

pub fn nonce() -> String {
    random_id("n_", 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_prefixed_hex_and_differ() {
        let first = nonce();
        assert!(first.starts_with("n_"));
        assert_eq!(first.len(), 2 + 32);
        assert!(first[2..].bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(first, nonce());
        assert!(polyui_core::validate::is_valid_id(&action_id()));
    }
}
