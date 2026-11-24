use std::fmt;

pub const BLOCKED_SCHEMES: [&str; 6] = ["javascript", "data", "file", "vbscript", "blob", "about"];

pub const MAX_URL_CHARS: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlProblem {
    Empty,
    TooLong,
    UnsafeCharacter,
    NotAbsolute,
    BlockedScheme(String),
    SchemeNotAllowed {
        scheme: String,
        allowed: Vec<String>,
    },
    MissingHost,
    BadHost,
    Credentials,
}

impl fmt::Display for UrlProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "URL is empty"),
            Self::TooLong => write!(f, "URL is longer than {MAX_URL_CHARS} characters"),
            Self::UnsafeCharacter => {
                write!(
                    f,
                    "URL contains whitespace, a backslash or a control character"
                )
            }
            Self::NotAbsolute => write!(f, "URL must be absolute, like https://example.com/a"),
            Self::BlockedScheme(scheme) => write!(f, "URL scheme \"{scheme}\" is never allowed"),
            Self::SchemeNotAllowed { scheme, allowed } => write!(
                f,
                "URL scheme \"{scheme}\" is not allowed, allowed: {}",
                allowed.join(", ")
            ),
            Self::MissingHost => write!(f, "URL has no host"),
            Self::BadHost => write!(
                f,
                "URL host must be plain ASCII letters, digits, dots and dashes"
            ),
            Self::Credentials => write!(f, "URL must not contain a user name or password"),
        }
    }
}

impl std::error::Error for UrlProblem {}

fn is_unsafe_char(c: char) -> bool {
    c.is_whitespace()
        || c.is_control()
        || c == '\\'
        || matches!(
            c,
            '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
        )
}

pub fn check_url<S: AsRef<str>>(url: &str, allowed: &[S]) -> Result<(), UrlProblem> {
    if url.is_empty() {
        return Err(UrlProblem::Empty);
    }
    if url.chars().count() > MAX_URL_CHARS {
        return Err(UrlProblem::TooLong);
    }
    if url.chars().any(is_unsafe_char) {
        return Err(UrlProblem::UnsafeCharacter);
    }
    let Some((scheme, rest)) = url.split_once(':') else {
        return Err(UrlProblem::NotAbsolute);
    };
    let mut scheme_chars = scheme.chars();
    let scheme_ok = scheme_chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme_chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !scheme_ok {
        return Err(UrlProblem::NotAbsolute);
    }
    let scheme = scheme.to_ascii_lowercase();
    if BLOCKED_SCHEMES.contains(&scheme.as_str()) {
        return Err(UrlProblem::BlockedScheme(scheme));
    }
    if !allowed
        .iter()
        .any(|candidate| candidate.as_ref().eq_ignore_ascii_case(&scheme))
    {
        return Err(UrlProblem::SchemeNotAllowed {
            scheme,
            allowed: allowed.iter().map(|s| s.as_ref().to_string()).collect(),
        });
    }
    if matches!(scheme.as_str(), "http" | "https" | "ws" | "wss" | "ftp") {
        check_authority(rest)?;
    } else if scheme == "ui" {
        let name = rest.strip_prefix("//").ok_or(UrlProblem::NotAbsolute)?;
        if name
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .is_empty()
        {
            return Err(UrlProblem::MissingHost);
        }
    } else if rest.is_empty() {
        return Err(UrlProblem::NotAbsolute);
    }
    Ok(())
}

fn check_authority(rest: &str) -> Result<(), UrlProblem> {
    let Some(after) = rest.strip_prefix("//") else {
        return Err(UrlProblem::NotAbsolute);
    };
    let authority = after.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err(UrlProblem::Credentials);
    }
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        let Some((inside, port)) = bracketed.split_once(']') else {
            return Err(UrlProblem::BadHost);
        };
        if inside.is_empty()
            || !inside
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
        {
            return Err(UrlProblem::BadHost);
        }
        check_port(port.strip_prefix(':'), port.is_empty())?;
        return Ok(());
    } else {
        match authority.split_once(':') {
            Some((host, port)) => {
                check_port(Some(port), false)?;
                host
            }
            None => authority,
        }
    };
    if host.is_empty() {
        return Err(UrlProblem::MissingHost);
    }
    let host_ok = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !host.starts_with('.')
        && !host.starts_with('-');
    if host_ok {
        Ok(())
    } else {
        Err(UrlProblem::BadHost)
    }
}

fn check_port(port: Option<&str>, absent: bool) -> Result<(), UrlProblem> {
    if absent {
        return Ok(());
    }
    match port {
        Some(digits)
            if !digits.is_empty()
                && digits.len() <= 5
                && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            Ok(())
        }
        _ => Err(UrlProblem::BadHost),
    }
}

pub fn is_safe_url<S: AsRef<str>>(url: &str, allowed: &[S]) -> bool {
    check_url(url, allowed).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTTPS: [&str; 1] = ["https"];

    #[test]
    fn accepts_plain_https() {
        for url in [
            "https://example.com",
            "https://example.com/a/b?c=d#e",
            "HTTPS://Example.com:8443/x",
            "https://[::1]:8080/",
            "https://xn--e1afmkfd.xn--p1ai",
            "https://example.com/путь",
        ] {
            assert!(is_safe_url(url, &HTTPS), "{url}");
        }
    }

    #[test]
    fn rejects_tricks() {
        for url in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            " javascript:alert(1)",
            "\tjavascript:alert(1)",
            "java\nscript:alert(1)",
            "java%73cript:alert(1)",
            "&#106;avascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "DATA:text/html;base64,PHNjcmlwdD4=",
            "file:///etc/passwd",
            "vbscript:msgbox(1)",
            "http://example.com",
            "//example.com/a",
            "/relative",
            "https:/example.com",
            "https://",
            "https://user:pass@example.com",
            "https://exa mple.com",
            "https://example.com\u{202E}moc.live",
            "https:\\\\example.com",
            "https://пример.рф",
            "https://example.com:99999999",
            "",
            "https://example.com ",
        ] {
            assert!(!is_safe_url(url, &HTTPS), "{url:?}");
        }
    }

    #[test]
    fn http_only_when_allowed() {
        assert!(is_safe_url("http://example.com", &["https", "http"]));
        assert_eq!(
            check_url("javascript:x", &["javascript"]),
            Err(UrlProblem::BlockedScheme("javascript".into()))
        );
        assert!(is_safe_url("mailto:a@b.c", &["mailto"]));
    }

    #[test]
    fn ui_resources_need_the_double_slash_and_a_name() {
        for url in [
            "ui://weather",
            "ui://weather-server/dashboard-template",
            "UI://Weather/x?y=1",
        ] {
            assert!(is_safe_url(url, &["ui"]), "{url}");
        }
        for url in [
            "ui:weather",
            "ui:/weather",
            "ui://",
            "ui:///x",
            "ui://a b",
            "https://example.com",
        ] {
            assert!(!is_safe_url(url, &["ui"]), "{url:?}");
        }
    }
}
