use crate::{Map, Value};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Locale {
    #[default]
    En,
    UzLatn,
    UzCyrl,
    Ru,
}

const EN_ORDINALS: [&str; 12] = [
    "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth",
    "eleventh", "twelfth",
];

const UZ_LATN_ORDINALS: [&str; 12] = [
    "birinchi",
    "ikkinchi",
    "uchinchi",
    "to\u{2bb}rtinchi",
    "beshinchi",
    "oltinchi",
    "yettinchi",
    "sakkizinchi",
    "to\u{2bb}qqizinchi",
    "o\u{2bb}ninchi",
    "o\u{2bb}n birinchi",
    "o\u{2bb}n ikkinchi",
];

const UZ_CYRL_ORDINALS: [&str; 12] = [
    "биринчи",
    "иккинчи",
    "учинчи",
    "тўртинчи",
    "бешинчи",
    "олтинчи",
    "еттинчи",
    "саккизинчи",
    "тўққизинчи",
    "ўнинчи",
    "ўн биринчи",
    "ўн иккинчи",
];

const RU_ORDINALS: [&str; 12] = [
    "первый",
    "второй",
    "третий",
    "четвёртый",
    "пятый",
    "шестой",
    "седьмой",
    "восьмой",
    "девятый",
    "десятый",
    "одиннадцатый",
    "двенадцатый",
];

impl Locale {
    pub const ALL: [Locale; 4] = [Self::En, Self::UzLatn, Self::UzCyrl, Self::Ru];

    pub fn from_tag(tag: &str) -> Self {
        let lower = tag.trim().to_ascii_lowercase().replace('_', "-");
        let mut parts = lower.split('-');
        match parts.next() {
            Some("uz") if parts.any(|part| part == "cyrl") => Self::UzCyrl,
            Some("uz") => Self::UzLatn,
            Some("ru") => Self::Ru,
            _ => Self::En,
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::UzLatn => "uz-Latn",
            Self::UzCyrl => "uz-Cyrl",
            Self::Ru => "ru",
        }
    }

    pub fn ordinals(self) -> &'static [&'static str; 12] {
        match self {
            Self::En => &EN_ORDINALS,
            Self::UzLatn => &UZ_LATN_ORDINALS,
            Self::UzCyrl => &UZ_CYRL_ORDINALS,
            Self::Ru => &RU_ORDINALS,
        }
    }

    pub fn ordinal(self, position: usize) -> String {
        position
            .checked_sub(1)
            .and_then(|index| self.ordinals().get(index))
            .map_or_else(|| position.to_string(), |word| (*word).to_string())
    }

    pub fn and_word(self) -> &'static str {
        match self {
            Self::En => "and",
            Self::UzLatn => "va",
            Self::UzCyrl => "ва",
            Self::Ru => "и",
        }
    }

    pub fn yes_word(self) -> &'static str {
        match self {
            Self::En => "yes",
            Self::UzLatn => "ha",
            Self::UzCyrl => "ҳа",
            Self::Ru => "да",
        }
    }

    pub fn no_word(self) -> &'static str {
        match self {
            Self::En => "no",
            Self::UzLatn => "yo\u{2bb}q",
            Self::UzCyrl => "йўқ",
            Self::Ru => "нет",
        }
    }
}

pub fn join_list(items: &[String], locale: Locale) -> String {
    let items: Vec<&str> = items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect();
    match items.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} {} {last}", rest.join(", "), locale.and_word()),
    }
}

pub fn join_ordinals(items: &[String], locale: Locale) -> String {
    items
        .iter()
        .map(|item| item.trim())
        .enumerate()
        .filter(|(_, item)| !item.is_empty())
        .map(|(index, item)| format!("{}, {item}", locale.ordinal(index + 1)))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn join_sentences(items: &[String]) -> String {
    let mut out = String::new();
    for item in items {
        let trimmed = item.trim();
        let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed).trim_end();
        if trimmed.is_empty() {
            continue;
        }
        if !out.is_empty() {
            if out.ends_with(['?', '!', ':', ';']) {
                out.push(' ');
            } else {
                out.push_str(". ");
            }
        }
        out.push_str(trimmed);
    }
    out
}

pub fn value_text(value: &Value, locale: Locale) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(flag) => {
            if *flag {
                locale.yes_word().to_string()
            } else {
                locale.no_word().to_string()
            }
        }
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        Value::Array(items) => join_list(&item_texts(items, locale), locale),
        Value::Object(map) => map
            .get("label")
            .map_or_else(String::new, |label| match label {
                Value::Object(_) | Value::Array(_) => String::new(),
                other => value_text(other, locale),
            }),
    }
}

fn item_texts(items: &[Value], locale: Locale) -> Vec<String> {
    items.iter().map(|item| value_text(item, locale)).collect()
}

#[derive(Debug)]
enum Piece {
    Content { text: String, capitalize: bool },
    Separator(String),
}

fn is_separator_char(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | ',' | ':' | ';' | '?' | '!')
}

fn push_literal(pieces: &mut Vec<Piece>, literal: &str) {
    let mut current = String::new();
    let mut current_is_separator = false;
    for c in literal.chars() {
        let separator = is_separator_char(c);
        if !current.is_empty() && separator != current_is_separator {
            pieces.push(make_piece(
                std::mem::take(&mut current),
                current_is_separator,
            ));
        }
        current_is_separator = separator;
        current.push(c);
    }
    if !current.is_empty() {
        pieces.push(make_piece(current, current_is_separator));
    }
}

fn make_piece(text: String, separator: bool) -> Piece {
    if separator {
        Piece::Separator(text)
    } else {
        Piece::Content {
            text,
            capitalize: false,
        }
    }
}

fn is_placeholder_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn placeholder_piece(
    inner: &str,
    values: &Map<String, Value>,
    children: &[String],
    locale: Locale,
) -> Option<Piece> {
    let (name, filter) = inner.split_once('|').unwrap_or((inner, ""));
    if !is_placeholder_name(name) {
        return None;
    }
    if name == "children" && !values.contains_key("children") {
        return Some(Piece::Content {
            text: join_sentences(children),
            capitalize: false,
        });
    }
    let value = values.get(name).unwrap_or(&Value::Null);
    let (text, capitalize) = match (filter, value) {
        ("list", Value::Array(items)) => (join_list(&item_texts(items, locale), locale), false),
        ("ordinals", Value::Array(items)) => {
            (join_ordinals(&item_texts(items, locale), locale), true)
        }
        _ => (value_text(value, locale), false),
    };
    Some(Piece::Content { text, capitalize })
}

fn parse_template(
    template: &str,
    values: &Map<String, Value>,
    children: &[String],
    locale: Locale,
) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let (before, from_brace) = rest.split_at(open);
        push_literal(&mut pieces, before);
        let after_brace = &from_brace[1..];
        let placeholder = after_brace.find('}').and_then(|close| {
            placeholder_piece(&after_brace[..close], values, children, locale)
                .map(|piece| (piece, close))
        });
        match placeholder {
            Some((piece, close)) => {
                pieces.push(piece);
                rest = &after_brace[close + 1..];
            }
            None => {
                push_literal(&mut pieces, "{");
                rest = after_brace;
            }
        }
    }
    push_literal(&mut pieces, rest);
    pieces
}

fn strength(separator: &str) -> u8 {
    if separator.contains(['.', '?', '!']) {
        4
    } else if separator.contains(':') {
        3
    } else if separator.contains(';') {
        2
    } else if separator.contains(',') {
        1
    } else {
        0
    }
}

fn ends_with_punctuation(text: &str) -> bool {
    text.trim_end().ends_with(['.', ',', ':', ';', '?', '!'])
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end().ends_with(['.', '?', '!'])
}

fn uppercase(c: char) -> char {
    let lowered = match c {
        'a'..='z' | 'à'..='ö' | 'ø'..='þ' | 'а'..='я' => c as u32 - 0x20,
        'ѐ'..='џ' => c as u32 - 0x50,
        _ => c as u32,
    };
    char::from_u32(lowered).unwrap_or(c)
}

fn capitalize_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => std::iter::once(uppercase(first)).chain(chars).collect(),
        None => String::new(),
    }
}

pub fn render(
    template: &str,
    values: &Map<String, Value>,
    children: &[String],
    locale: Locale,
) -> String {
    let pieces = parse_template(template, values, children, locale);
    let mut merged: Vec<Piece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match piece {
            Piece::Content { text, capitalize } => {
                if !text.trim().is_empty() {
                    merged.push(Piece::Content { text, capitalize });
                }
            }
            Piece::Separator(separator) => match merged.last_mut() {
                None => {}
                Some(Piece::Separator(existing)) => {
                    if strength(&separator) > strength(existing) {
                        *existing = separator;
                    }
                }
                Some(Piece::Content { .. }) => merged.push(Piece::Separator(separator)),
            },
        }
    }
    let mut out = String::new();
    let mut pending: Option<String> = None;
    for piece in merged {
        match piece {
            Piece::Separator(separator) => pending = Some(separator),
            Piece::Content { text, capitalize } => {
                if let Some(separator) = pending.take() {
                    let closes = text.trim_start().starts_with([']', ')']);
                    let opens = out.trim_end().ends_with(['[', '(']);
                    if !closes && !opens {
                        if ends_with_punctuation(&out) {
                            out.push(' ');
                        } else {
                            out.push_str(&separator);
                        }
                    }
                }
                let starts_sentence = out.trim().is_empty() || ends_sentence(&out);
                if capitalize && starts_sentence {
                    out.push_str(&capitalize_first(text.trim_start()));
                } else {
                    out.push_str(&text);
                }
            }
        }
    }
    if let Some(separator) = pending
        && !ends_with_punctuation(&out)
        && let Some(mark) = separator.chars().find(|c| matches!(c, '.' | '?' | '!'))
    {
        out.push(mark);
    }
    tidy(&out)
}

pub fn tidy(text: &str) -> String {
    let mut collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let before = collapsed.len();
        for (from, to) in [
            ("[]", ""),
            ("()", ""),
            ("[ ]", ""),
            ("( )", ""),
            ("  ", " "),
            (" .", "."),
            (" ,", ","),
        ] {
            collapsed = collapsed.replace(from, to);
        }
        if collapsed.len() == before {
            break;
        }
    }
    let trimmed = collapsed
        .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '.' | ',' | ':' | ';'));
    trimmed.trim_end().to_string()
}

fn strip_enclosing(text: &str, open: char, close: char) -> Option<&str> {
    let inner = text.strip_prefix(open)?.strip_suffix(close)?;
    let mut depth = 0usize;
    for c in inner.chars() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth = depth.checked_sub(1)?;
        }
    }
    (depth == 0).then_some(inner)
}

pub fn label_from_alt(alt: &str) -> String {
    let mut text = alt.trim();
    for (open, close) in [('[', ']'), ('(', ')')] {
        if let Some(inner) = strip_enclosing(text, open, close) {
            text = inner.trim();
        }
    }
    text.trim_end_matches(|c: char| matches!(c, '_' | ':' | '.') || c.is_whitespace())
        .to_string()
}

pub fn as_sentence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.ends_with(['.', '?', '!', '\u{2026}']) {
        trimmed.to_string()
    } else {
        format!("{trimmed}.")
    }
}

#[cfg(all(test, not(feature = "client-lite")))]
mod tests {
    use super::*;
    use serde_json::json;

    fn values(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn locales_from_tags() {
        assert_eq!(Locale::from_tag("uz-Latn"), Locale::UzLatn);
        assert_eq!(Locale::from_tag("uz"), Locale::UzLatn);
        assert_eq!(Locale::from_tag("uz-Cyrl-UZ"), Locale::UzCyrl);
        assert_eq!(Locale::from_tag("ru_RU"), Locale::Ru);
        assert_eq!(Locale::from_tag("en-US"), Locale::En);
        assert_eq!(Locale::from_tag("fr"), Locale::En);
        assert_eq!(Locale::Ru.ordinal(12), "двенадцатый");
        assert_eq!(Locale::En.ordinal(13), "13");
        assert_eq!(Locale::En.ordinal(0), "0");
        for locale in Locale::ALL {
            for word in locale.ordinals() {
                let capital = capitalize_first(word);
                assert_ne!(capital.chars().next(), word.chars().next(), "{word}");
            }
        }
    }

    #[test]
    fn lists_in_every_locale() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(join_list(&items, Locale::En), "a, b and c");
        assert_eq!(join_list(&items, Locale::UzLatn), "a, b va c");
        assert_eq!(join_list(&items, Locale::UzCyrl), "a, b ва c");
        assert_eq!(join_list(&items, Locale::Ru), "a, b и c");
        assert_eq!(join_list(&items[..1], Locale::En), "a");
        assert_eq!(join_list(&[], Locale::En), "");
    }

    #[test]
    fn card_templates_have_no_empty_fragments() {
        let template = "{title}. {children}";
        let children = vec!["Total: 29 322 so'm".to_string(), "[Pay]".to_string()];
        assert_eq!(
            render(template, &values(json!({})), &children, Locale::En),
            "Total: 29 322 so'm. [Pay]"
        );
        assert_eq!(
            render(
                template,
                &values(json!({ "title": "Your bills" })),
                &[],
                Locale::En
            ),
            "Your bills."
        );
        assert_eq!(render(template, &values(json!({})), &[], Locale::En), "");
        assert_eq!(
            render(
                template,
                &values(json!({ "title": "Done." })),
                &children,
                Locale::En
            ),
            "Done. Total: 29 322 so'm. [Pay]"
        );
    }

    #[test]
    fn ordinals_are_capitalized_at_sentence_start() {
        let options = json!({ "label": "Pick one", "options": [ { "id": "gas", "label": "Gas" }, { "id": "power", "label": "Power" } ] });
        assert_eq!(
            render(
                "{label}. {options|ordinals}",
                &values(options.clone()),
                &[],
                Locale::En
            ),
            "Pick one. First, Gas; second, Power"
        );
        assert_eq!(
            render(
                "{label}: {options|list}",
                &values(options.clone()),
                &[],
                Locale::En
            ),
            "Pick one: Gas and Power"
        );
        assert_eq!(
            render(
                "{label}. {options|ordinals}",
                &values(options),
                &[],
                Locale::UzLatn
            ),
            "Pick one. Birinchi, Gas; ikkinchi, Power"
        );
    }

    #[test]
    fn missing_values_and_brackets() {
        assert_eq!(
            render(
                "[{label}]",
                &values(json!({ "label": "Pay" })),
                &[],
                Locale::En
            ),
            "[Pay]"
        );
        assert_eq!(render("[{label}]", &values(json!({})), &[], Locale::En), "");
        assert_eq!(
            render("[image: {alt}]", &values(json!({})), &[], Locale::En),
            "[image]"
        );
        assert_eq!(
            render(
                "{label}: ____",
                &values(json!({ "label": "Phone" })),
                &[],
                Locale::En
            ),
            "Phone: ____"
        );
        assert_eq!(
            render(
                "{label}?",
                &values(json!({ "label": "Phone" })),
                &[],
                Locale::En
            ),
            "Phone?"
        );
        assert_eq!(
            render(
                "{label}: {value} {unit}",
                &values(json!({ "label": "Total", "value": "29 322" })),
                &[],
                Locale::En
            ),
            "Total: 29 322"
        );
        assert_eq!(
            render(
                "{title} {body} [{confirm_label}] [{cancel_label}]",
                &values(json!({ "title": "Sure?", "confirm_label": "Yes", "cancel_label": "No" })),
                &[],
                Locale::En
            ),
            "Sure? [Yes] [No]"
        );
        assert_eq!(
            render(
                "{a}  {b}",
                &values(json!({ "a": "x\n\ny", "b": 2 })),
                &[],
                Locale::En
            ),
            "x y 2"
        );
        assert_eq!(
            render("{unclosed", &values(json!({})), &[], Locale::En),
            "{unclosed"
        );
        assert_eq!(
            render("{Bad}", &values(json!({})), &[], Locale::En),
            "{Bad}"
        );
    }

    #[test]
    fn labels_and_sentences() {
        assert_eq!(label_from_alt("[Pay]"), "Pay");
        assert_eq!(label_from_alt("(paid)"), "paid");
        assert_eq!(label_from_alt("Phone: ____"), "Phone");
        assert_eq!(label_from_alt("[a] [b]"), "[a] [b]");
        assert_eq!(label_from_alt("[[ref]]"), "[ref]");
        assert_eq!(label_from_alt("Your bills."), "Your bills");
        assert_eq!(as_sentence("Your bills"), "Your bills.");
        assert_eq!(as_sentence("Ready?"), "Ready?");
        assert_eq!(
            join_sentences(&["Phone?".into(), "Next.".into(), "".into()]),
            "Phone? Next"
        );
    }
}
