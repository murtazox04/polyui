use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_ROWS: usize = 1000;
pub const MAX_SERIES: usize = 8;
pub const MAX_CATEGORIES: usize = 200;

const MIN_WIDTH: u32 = 160;
const MIN_HEIGHT: u32 = 120;
const MAX_SIDE: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    Bar,
    Line,
    Area,
    Pie,
    Scatter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(from = "String")]
pub enum Locale {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "uz-Latn")]
    UzLatn,
    #[serde(rename = "uz-Cyrl")]
    UzCyrl,
    #[serde(rename = "ru")]
    Ru,
}

impl Locale {
    pub fn parse(tag: &str) -> Locale {
        let lowered = tag.trim().to_ascii_lowercase().replace('_', "-");
        let mut parts = lowered.split('-');
        let language = parts.next().unwrap_or_default();
        let rest: Vec<&str> = parts.collect();
        match language {
            "uz" if rest.contains(&"cyrl") => Locale::UzCyrl,
            "uz" => Locale::UzLatn,
            "ru" => Locale::Ru,
            _ => Locale::En,
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::UzLatn => "uz-Latn",
            Locale::UzCyrl => "uz-Cyrl",
            Locale::Ru => "ru",
        }
    }
}

impl From<String> for Locale {
    fn from(tag: String) -> Self {
        Locale::parse(&tag)
    }
}

impl From<&str> for Locale {
    fn from(tag: &str) -> Self {
        Locale::parse(tag)
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.tag())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSpec {
    pub title: String,
    pub kind: ChartKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default)]
    pub rows: Vec<Value>,
}

impl ChartSpec {
    pub fn new(title: impl Into<String>, kind: ChartKind, rows: Vec<Value>) -> Self {
        ChartSpec {
            title: title.into(),
            kind,
            x_label: None,
            y_label: None,
            unit: None,
            rows,
        }
    }

    pub fn unit_text(&self) -> Option<&str> {
        self.unit
            .as_deref()
            .map(str::trim)
            .filter(|unit| !unit.is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChartOptions {
    pub width: u32,
    pub height: u32,
    pub locale: Locale,
}

impl Default for ChartOptions {
    fn default() -> Self {
        ChartOptions {
            width: 640,
            height: 360,
            locale: Locale::En,
        }
    }
}

impl ChartOptions {
    pub fn with_locale(locale: Locale) -> Self {
        ChartOptions {
            locale,
            ..ChartOptions::default()
        }
    }

    pub fn clamped_width(&self) -> f64 {
        f64::from(self.width.clamp(MIN_WIDTH, MAX_SIDE))
    }

    pub fn clamped_height(&self) -> f64 {
        f64::from(self.height.clamp(MIN_HEIGHT, MAX_SIDE))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChartError {
    TooManyRows { count: usize, max: usize },
    TooManySeries { count: usize, max: usize },
    TooManyCategories { count: usize, max: usize },
}

impl fmt::Display for ChartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChartError::TooManyRows { count, max } => {
                write!(
                    formatter,
                    "chart has {count} rows; at most {max} are allowed"
                )
            }
            ChartError::TooManySeries { count, max } => {
                write!(
                    formatter,
                    "chart has {count} series; at most {max} are allowed"
                )
            }
            ChartError::TooManyCategories { count, max } => {
                write!(
                    formatter,
                    "chart has {count} categories; at most {max} are allowed"
                )
            }
        }
    }
}

impl std::error::Error for ChartError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_parses_common_tags() {
        assert_eq!(Locale::parse("en-US"), Locale::En);
        assert_eq!(Locale::parse("uz"), Locale::UzLatn);
        assert_eq!(Locale::parse("uz-Latn-UZ"), Locale::UzLatn);
        assert_eq!(Locale::parse("uz_Cyrl"), Locale::UzCyrl);
        assert_eq!(Locale::parse("ru-RU"), Locale::Ru);
        assert_eq!(Locale::parse("klingon"), Locale::En);
        assert_eq!(Locale::parse(""), Locale::En);
    }

    #[test]
    fn locale_round_trips_through_json() {
        let locale: Locale = serde_json::from_str("\"uz-Cyrl\"").unwrap();
        assert_eq!(locale, Locale::UzCyrl);
        assert_eq!(serde_json::to_string(&locale).unwrap(), "\"uz-Cyrl\"");
    }

    #[test]
    fn spec_deserializes_from_resolved_props() {
        let spec: ChartSpec = serde_json::from_str(
            r#"{"title":"Gas","kind":"bar","x_label":"Month","unit":"m3",
                "on_select":{"action_id":"a1"},
                "rows":[{"x":"Jan","y":3}]}"#,
        )
        .unwrap();
        assert_eq!(spec.kind, ChartKind::Bar);
        assert_eq!(spec.x_label.as_deref(), Some("Month"));
        assert_eq!(spec.rows.len(), 1);
    }

    #[test]
    fn options_fill_defaults_and_clamp() {
        let options: ChartOptions = serde_json::from_str(r#"{"locale":"ru"}"#).unwrap();
        assert_eq!(options.width, 640);
        assert_eq!(options.locale, Locale::Ru);
        let tiny = ChartOptions {
            width: 0,
            height: 1_000_000,
            locale: Locale::En,
        };
        assert_eq!(tiny.clamped_width(), 160.0);
        assert_eq!(tiny.clamped_height(), 4096.0);
    }
}
