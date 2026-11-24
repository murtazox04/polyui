const LOOPBACK_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];

pub fn normalize(origin: &str) -> Result<String, String> {
    let trimmed = origin.trim();
    let lowered = trimmed
        .strip_suffix('/')
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    let authority = lowered
        .strip_prefix("https://")
        .or_else(|| lowered.strip_prefix("http://"))
        .ok_or_else(|| format!("an origin starts with https:// or http://, not {origin}"))?;
    let clean = !authority.is_empty()
        && !authority.contains(['/', '?', '#', '@', ' ', '\\'])
        && split_authority(authority).is_some();
    if !clean {
        return Err(format!(
            "an origin is a scheme, a host and an optional port, for example https://app.example.com or http://localhost:5173, not {origin}"
        ));
    }
    Ok(lowered)
}

fn split_authority(authority: &str) -> Option<(&str, Option<&str>)> {
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']')?;
        let (host, rest) = authority.split_at(end + 1);
        match rest {
            "" => (host, None),
            rest => (host, Some(rest.strip_prefix(':')?)),
        }
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    let port_ok = port.is_none_or(|port| port.parse::<u16>().is_ok());
    (!host.is_empty() && port_ok).then_some((host, port))
}

fn authority_of(origin: &str) -> Option<&str> {
    origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
}

fn is_loopback(host: &str) -> bool {
    LOOPBACK_HOSTS.contains(&host)
}

pub fn allowed(origin: &str, host: Option<&str>, extra: &[String]) -> bool {
    let Ok(origin) = normalize(origin) else {
        return false;
    };
    if extra.contains(&origin) {
        return true;
    }
    let Some(host) = host.map(str::to_ascii_lowercase) else {
        return false;
    };
    let Some(authority) = authority_of(&origin) else {
        return false;
    };
    if authority == host {
        return true;
    }
    match (split_authority(authority), split_authority(&host)) {
        (Some((from, from_port)), Some((to, to_port))) => {
            from_port == to_port && is_loopback(from) && is_loopback(to)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_are_normalized_or_refused() {
        assert_eq!(
            normalize("HTTPS://App.Example.com/"),
            Ok("https://app.example.com".to_string())
        );
        assert_eq!(
            normalize("http://[::1]:5173"),
            Ok("http://[::1]:5173".to_string())
        );
        for bad in [
            "app.example.com",
            "ftp://app.example.com",
            "https://",
            "https://app.example.com/path",
            "https://user@app.example.com",
            "https://app.example.com:99999",
            "https://[::1",
            "null",
        ] {
            assert!(normalize(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_server_origin_and_its_loopback_twins_are_allowed() {
        let none: [String; 0] = [];
        assert!(allowed(
            "http://127.0.0.1:8787",
            Some("127.0.0.1:8787"),
            &none
        ));
        assert!(allowed(
            "http://localhost:8787",
            Some("127.0.0.1:8787"),
            &none
        ));
        assert!(allowed("http://[::1]:8787", Some("localhost:8787"), &none));
        assert!(allowed(
            "https://chat.example.com",
            Some("chat.example.com"),
            &none
        ));
        assert!(!allowed(
            "http://localhost:5173",
            Some("127.0.0.1:8787"),
            &none
        ));
        assert!(!allowed(
            "https://evil.example",
            Some("127.0.0.1:8787"),
            &none
        ));
        assert!(!allowed(
            "http://localhost:8787",
            Some("chat.example.com:8787"),
            &none
        ));
        assert!(!allowed("null", Some("127.0.0.1:8787"), &none));
        assert!(!allowed("http://127.0.0.1:8787", None, &none));
    }

    #[test]
    fn listed_origins_are_allowed_exactly() {
        let listed = vec!["http://localhost:5173".to_string()];
        assert!(allowed(
            "http://localhost:5173",
            Some("127.0.0.1:8787"),
            &listed
        ));
        assert!(allowed("http://LOCALHOST:5173/", None, &listed));
        assert!(!allowed(
            "http://localhost:5174",
            Some("127.0.0.1:8787"),
            &listed
        ));
        assert!(!allowed(
            "https://localhost:5173",
            Some("127.0.0.1:8787"),
            &listed
        ));
    }
}
