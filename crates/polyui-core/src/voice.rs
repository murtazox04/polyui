use crate::client::RenderBatch;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Utterance {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VoiceMatch {
    pub component: String,
    pub option: Option<String>,
    pub batch: RenderBatch,
    pub action: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordinal {
    Nth(usize),
    Last,
}

impl Ordinal {
    pub fn position(self, count: usize) -> Option<usize> {
        match self {
            Self::Nth(n) if n >= 1 && n <= count => Some(n - 1),
            Self::Last if count > 0 => Some(count - 1),
            _ => None,
        }
    }
}

fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'ç' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ı' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
        'ş' | 'š' => 's',
        'ğ' => 'g',
        'ý' | 'ÿ' => 'y',
        'ž' => 'z',
        'ё' => 'е',
        'й' => 'и',
        'ў' => 'у',
        'қ' => 'к',
        'ғ' => 'г',
        'ҳ' => 'х',
        other => other,
    }
}

fn is_apostrophe(c: char) -> bool {
    matches!(
        c,
        '\'' | '`' | '\u{2bb}' | '\u{2bc}' | '\u{2018}' | '\u{2019}' | '\u{b4}' | '\u{2b9}'
    )
}

fn shifted(c: char, offset: u32) -> char {
    char::from_u32(c as u32 + offset).unwrap_or(c)
}

pub fn lowercase(c: char) -> char {
    let code = c as u32;
    match c {
        'A'..='Z' | 'À'..='Ö' | 'Ø'..='Þ' | 'А'..='Я' => shifted(c, 0x20),
        'Ѐ'..='Џ' => shifted(c, 0x50),
        '\u{100}'..='\u{137}'
        | '\u{14a}'..='\u{177}'
        | '\u{460}'..='\u{4bf}'
        | '\u{4d0}'..='\u{4ff}'
            if code % 2 == 0 =>
        {
            shifted(c, 1)
        }
        '\u{139}'..='\u{148}' | '\u{179}'..='\u{17e}' | '\u{4c1}'..='\u{4ce}' if code % 2 == 1 => {
            shifted(c, 1)
        }
        'İ' => 'i',
        other => other,
    }
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || (c > '\u{bf}'
            && c != '×'
            && c != '÷'
            && !c.is_whitespace()
            && !matches!(c, '\u{2000}'..='\u{206f}' | '\u{3000}'..='\u{303f}' | '\u{fe30}'..='\u{fe4f}' | '\u{ff00}'..='\u{ff0f}'))
}

pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().map(lowercase) {
        if is_apostrophe(c) {
            continue;
        }
        let folded = fold(c);
        if is_word_char(folded) {
            out.push(folded);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn tokens(normalized: &str) -> Vec<&str> {
    normalized
        .split(' ')
        .filter(|token| !token.is_empty())
        .collect()
}

const EN_ORDINALS: [&str; 12] = [
    "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth",
    "eleventh", "twelfth",
];
const EN_CARDINALS: [&str; 12] = [
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve",
];
const UZ_LATN_STEMS: [&str; 10] = [
    "birinchi",
    "ikkinchi",
    "uchinchi",
    "tortinchi",
    "beshinchi",
    "oltinchi",
    "yettinchi",
    "sakkizinchi",
    "toqqizinchi",
    "oninchi",
];
const UZ_CYRL_STEMS: [&str; 10] = [
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
];
const RU_STEMS: [&str; 12] = [
    "перв",
    "втор",
    "трет",
    "четвёрт",
    "пят",
    "шест",
    "седьм",
    "восьм",
    "девят",
    "десят",
    "одиннадцат",
    "двенадцат",
];
const RU_ENDINGS: [&str; 17] = [
    "ый", "ой", "ий", "ая", "ое", "ую", "ье", "ья", "ью", "ого", "его", "ом", "ем", "ые", "ых",
    "ым", "ей",
];
const RU_CARDINALS: [&str; 12] = [
    "один",
    "два",
    "три",
    "четыре",
    "пять",
    "шесть",
    "семь",
    "восемь",
    "девять",
    "десять",
    "одиннадцать",
    "двенадцать",
];
const NUMBER_WORDS: [&str; 5] = ["number", "option", "номер", "вариант", "raqam"];
const LAST_WORDS: [&str; 4] = ["last", "oxirgi", "охирги", "последн"];
const TEN_WORDS: [&str; 2] = ["on", "ўн"];

const YES_WORDS: [&str; 22] = [
    "yes",
    "yeah",
    "yep",
    "sure",
    "ok",
    "okay",
    "confirm",
    "ha",
    "xa",
    "albatta",
    "mayli",
    "xop",
    "tasdiqlayman",
    "ҳа",
    "албатта",
    "майли",
    "хоп",
    "да",
    "ага",
    "конечно",
    "подтверждаю",
    "хорошо",
];
const NO_WORDS: [&str; 9] = [
    "no",
    "nope",
    "cancel",
    "yo'q",
    "bekor",
    "йўқ",
    "бекор",
    "нет",
    "отмена",
];

fn matches_word(token: &str, word: &str) -> bool {
    token == normalize(word)
}

fn position_in(list: &[&str], token: &str) -> Option<usize> {
    list.iter().position(|word| matches_word(token, word))
}

fn stem_position(stems: &[&str], token: &str) -> Option<usize> {
    stems
        .iter()
        .position(|stem| token.starts_with(normalize(stem).as_str()))
}

fn russian_ordinal(token: &str) -> Option<usize> {
    RU_STEMS.iter().position(|stem| {
        let stem = normalize(stem);
        token.strip_prefix(stem.as_str()).is_some_and(|ending| {
            RU_ENDINGS
                .iter()
                .any(|candidate| normalize(candidate) == ending)
        })
    })
}

fn english_suffixed(token: &str) -> Option<usize> {
    let digits = ["st", "nd", "rd", "th"]
        .iter()
        .find_map(|suffix| token.strip_suffix(suffix))?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn number(token: &str) -> Option<usize> {
    if token.is_empty() || token.len() > 4 || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    token.parse().ok().filter(|n| *n >= 1)
}

pub fn ordinal(tokens: &[&str]) -> Option<Ordinal> {
    for (index, token) in tokens.iter().enumerate() {
        let next = tokens.get(index + 1).copied().unwrap_or("");
        if TEN_WORDS.iter().any(|ten| matches_word(token, ten)) {
            let latin = stem_position(&UZ_LATN_STEMS[..2], next);
            let cyrillic = stem_position(&UZ_CYRL_STEMS[..2], next);
            if let Some(unit) = latin.or(cyrillic) {
                return Some(Ordinal::Nth(11 + unit));
            }
        }
        if let Some(found) = position_in(&EN_ORDINALS, token)
            .or_else(|| stem_position(&UZ_LATN_STEMS, token))
            .or_else(|| stem_position(&UZ_CYRL_STEMS, token))
            .or_else(|| russian_ordinal(token))
        {
            return Some(Ordinal::Nth(found + 1));
        }
        if let Some(n) = english_suffixed(token).or_else(|| number(token)) {
            return Some(Ordinal::Nth(n));
        }
        if NUMBER_WORDS.iter().any(|word| matches_word(token, word))
            && let Some(found) =
                position_in(&EN_CARDINALS, next).or_else(|| position_in(&RU_CARDINALS, next))
        {
            return Some(Ordinal::Nth(found + 1));
        }
        if LAST_WORDS
            .iter()
            .any(|word| token.starts_with(normalize(word).as_str()))
        {
            return Some(Ordinal::Last);
        }
    }
    None
}

pub fn yes_no(tokens: &[&str]) -> Option<bool> {
    let yes = tokens
        .iter()
        .any(|token| YES_WORDS.iter().any(|word| matches_word(token, word)));
    let no = tokens
        .iter()
        .any(|token| NO_WORDS.iter().any(|word| matches_word(token, word)));
    match (yes, no) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

pub const FULL_MATCH: usize = 100;
pub const ORDINAL_MATCH: usize = 50;
pub const PARTIAL_MATCH: usize = 10;

pub fn phrase_score(text: &[&str], phrase: &str) -> usize {
    let normalized = normalize(phrase);
    let words = tokens(&normalized);
    if words.is_empty() {
        return 0;
    }
    if text
        .windows(words.len())
        .any(|window| window == words.as_slice())
    {
        return FULL_MATCH + words.len();
    }
    let partial = words
        .iter()
        .filter(|word| word.chars().count() >= 4 && !word.chars().all(|c| c.is_ascii_digit()))
        .filter(|word| text.contains(word))
        .count();
    if partial > 0 {
        PARTIAL_MATCH + partial
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Option<Ordinal> {
        let normalized = normalize(text);
        ordinal(&tokens(&normalized))
    }

    fn answer(text: &str) -> Option<bool> {
        let normalized = normalize(text);
        yes_no(&tokens(&normalized))
    }

    #[test]
    fn normalizes_apostrophes_and_diacritics() {
        assert_eq!(normalize("Yo'q"), "yoq");
        assert_eq!(normalize("yo`q"), "yoq");
        assert_eq!(normalize("yo\u{2bb}q"), "yoq");
        assert_eq!(normalize("Ёлка, Й!"), "елка и");
        assert_eq!(normalize("Café  Crème"), "cafe creme");
        assert_eq!(normalize("ТЎРТИНЧИ"), "туртинчи");
        assert_eq!(normalize("ҚЎҒҲ Ёё"), "кугх ее");
        assert_eq!(normalize("ÀÉÎÕÜ ŞĞČŽ"), "aeiou sgcz");
        assert_eq!(normalize("a\u{2014}b\u{3001}c"), "a b c");
    }

    #[test]
    fn uzbek_latin_ordinals() {
        assert_eq!(parse("birinchi"), Some(Ordinal::Nth(1)));
        assert_eq!(parse("ikkinchi"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("ikkinchisi"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("ikkinchisini tanla"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("to'rtinchisi"), Some(Ordinal::Nth(4)));
        assert_eq!(parse("toʻqqizinchi"), Some(Ordinal::Nth(9)));
        assert_eq!(parse("o'n ikkinchi"), Some(Ordinal::Nth(12)));
        assert_eq!(parse("o`ninchi"), Some(Ordinal::Nth(10)));
        assert_eq!(parse("oxirgisi"), Some(Ordinal::Last));
        assert_eq!(parse("3-chi"), Some(Ordinal::Nth(3)));
    }

    #[test]
    fn uzbek_cyrillic_ordinals() {
        assert_eq!(parse("иккинчисини"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("тўртинчи"), Some(Ordinal::Nth(4)));
        assert_eq!(parse("ўн биринчи"), Some(Ordinal::Nth(11)));
    }

    #[test]
    fn russian_ordinals() {
        assert_eq!(parse("первый"), Some(Ordinal::Nth(1)));
        assert_eq!(parse("второй"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("второе"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("выбери вторую"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("третий"), Some(Ordinal::Nth(3)));
        assert_eq!(parse("четвёртый"), Some(Ordinal::Nth(4)));
        assert_eq!(parse("четвертую"), Some(Ordinal::Nth(4)));
        assert_eq!(parse("двенадцатый"), Some(Ordinal::Nth(12)));
        assert_eq!(parse("номер два"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("последний"), Some(Ordinal::Last));
        assert_eq!(parse("вторник"), None);
        assert_eq!(parse("девятнадцатый"), None);
    }

    #[test]
    fn english_ordinals() {
        assert_eq!(parse("the second one"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("Second!"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("number two"), Some(Ordinal::Nth(2)));
        assert_eq!(parse("the 3rd"), Some(Ordinal::Nth(3)));
        assert_eq!(parse("the last one"), Some(Ordinal::Last));
        assert_eq!(parse("pay it"), None);
        assert_eq!(Ordinal::Nth(2).position(3), Some(1));
        assert_eq!(Ordinal::Nth(4).position(3), None);
        assert_eq!(Ordinal::Last.position(3), Some(2));
    }

    #[test]
    fn yes_and_no() {
        assert_eq!(answer("ha"), Some(true));
        assert_eq!(answer("xa, to'la"), Some(true));
        assert_eq!(answer("yo'q"), Some(false));
        assert_eq!(answer("yoʻq"), Some(false));
        assert_eq!(answer("Да"), Some(true));
        assert_eq!(answer("нет"), Some(false));
        assert_eq!(answer("ҳа"), Some(true));
        assert_eq!(answer("йўқ"), Some(false));
        assert_eq!(answer("Yes please"), Some(true));
        assert_eq!(answer("no"), Some(false));
        assert_eq!(answer("yes no"), None);
        assert_eq!(answer("maybe"), None);
    }

    #[test]
    fn phrases() {
        let normalized = normalize("pay the gas bill");
        let text = tokens(&normalized);
        assert_eq!(phrase_score(&text, "Gas bill"), FULL_MATCH + 2);
        assert_eq!(phrase_score(&text, "Gas"), FULL_MATCH + 1);
        assert_eq!(phrase_score(&text, "Water bill now"), PARTIAL_MATCH + 1);
        assert_eq!(phrase_score(&text, "Power"), 0);
        assert_eq!(phrase_score(&text, ""), 0);
    }
}
