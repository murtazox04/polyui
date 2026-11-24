use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::format::format_number;
use crate::spec::{ChartError, ChartKind, ChartSpec, Locale, MAX_CATEGORIES, MAX_ROWS, MAX_SERIES};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub row: usize,
    pub category: usize,
    pub series: usize,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dataset {
    pub categories: Vec<String>,
    pub series: Vec<String>,
    pub has_series: bool,
    pub points: Vec<Point>,
    pub skipped: usize,
    pub duplicates: usize,
}

struct ParsedRow {
    row: usize,
    label: String,
    x: f64,
    y: Option<f64>,
    series: Option<String>,
}

fn finite_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
}

fn category_label(value: Option<&Value>, locale: Locale) -> Option<String> {
    match value? {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        Value::Number(number) => number.as_f64().map(|float| format_number(float, locale)),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn series_name(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

pub fn other_series_name(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Other",
        Locale::UzLatn => "Boshqa",
        Locale::UzCyrl => "Бошқа",
        Locale::Ru => "Другое",
    }
}

fn parse_row(index: usize, row: &Value, kind: ChartKind, locale: Locale) -> Option<ParsedRow> {
    let fields = row.as_object()?;
    let y = finite_number(fields.get("y"));
    let raw_x = fields.get("x");
    let (label, x) = match kind {
        ChartKind::Scatter => {
            let x = finite_number(raw_x)?;
            (format_number(x, locale), x)
        }
        ChartKind::Pie => {
            y.filter(|value| *value > 0.0)?;
            (category_label(raw_x, locale)?, 0.0)
        }
        ChartKind::Bar | ChartKind::Line | ChartKind::Area => (category_label(raw_x, locale)?, 0.0),
    };
    let series = if kind == ChartKind::Pie {
        None
    } else {
        series_name(fields.get("series"))
    };
    Some(ParsedRow {
        row: index,
        label,
        x,
        y,
        series,
    })
}

fn intern(name: &str, names: &mut Vec<String>, lookup: &mut HashMap<String, usize>) -> usize {
    if let Some(index) = lookup.get(name) {
        return *index;
    }
    let index = names.len();
    names.push(name.to_owned());
    lookup.insert(name.to_owned(), index);
    index
}

impl Dataset {
    pub fn build(spec: &ChartSpec, locale: Locale) -> Result<Dataset, ChartError> {
        if spec.rows.len() > MAX_ROWS {
            return Err(ChartError::TooManyRows {
                count: spec.rows.len(),
                max: MAX_ROWS,
            });
        }
        let parsed: Vec<ParsedRow> = spec
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| parse_row(index, row, spec.kind, locale))
            .collect();
        let skipped = spec.rows.len() - parsed.iter().filter(|row| row.y.is_some()).count();
        let has_series = parsed
            .iter()
            .any(|row| row.y.is_some() && row.series.is_some());
        let fallback_series = if has_series {
            other_series_name(locale).to_owned()
        } else {
            spec.y_label
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .to_owned()
        };

        let mut dataset = Dataset {
            has_series,
            skipped,
            ..Dataset::default()
        };
        let mut category_lookup = HashMap::new();
        let mut series_lookup = HashMap::new();
        let mut seen = HashSet::new();
        for row in parsed {
            let category = if spec.kind == ChartKind::Scatter {
                0
            } else {
                intern(&row.label, &mut dataset.categories, &mut category_lookup)
            };
            let Some(y) = row.y else {
                continue;
            };
            let series_label = row.series.as_deref().unwrap_or(&fallback_series);
            let series = intern(series_label, &mut dataset.series, &mut series_lookup);
            if spec.kind != ChartKind::Scatter && !seen.insert((category, series)) {
                dataset.duplicates += 1;
                continue;
            }
            dataset.points.push(Point {
                row: row.row,
                category,
                series,
                x: row.x,
                y,
            });
        }

        if dataset.series.len() > MAX_SERIES {
            return Err(ChartError::TooManySeries {
                count: dataset.series.len(),
                max: MAX_SERIES,
            });
        }
        if dataset.categories.len() > MAX_CATEGORIES {
            return Err(ChartError::TooManyCategories {
                count: dataset.categories.len(),
                max: MAX_CATEGORIES,
            });
        }
        Ok(dataset)
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn category_name(&self, index: usize) -> &str {
        self.categories.get(index).map_or("", String::as_str)
    }

    pub fn series_label(&self, index: usize) -> Option<&str> {
        if self.has_series {
            self.series.get(index).map(String::as_str)
        } else {
            None
        }
    }

    pub fn shares(&self) -> Vec<f64> {
        let largest = self.points.iter().map(|point| point.y).fold(0.0, f64::max);
        if largest <= 0.0 || !largest.is_finite() {
            return vec![0.0; self.points.len()];
        }
        let scaled: Vec<f64> = self.points.iter().map(|point| point.y / largest).collect();
        let total: f64 = scaled.iter().sum();
        if total <= 0.0 || !total.is_finite() {
            return vec![0.0; self.points.len()];
        }
        scaled.into_iter().map(|value| value / total).collect()
    }

    pub fn y_extent(&self) -> Option<(f64, f64)> {
        extent(self.points.iter().map(|point| point.y))
    }

    pub fn x_extent(&self) -> Option<(f64, f64)> {
        extent(self.points.iter().map(|point| point.x))
    }
}

fn extent(values: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    values.fold(None, |range, value| match range {
        None => Some((value, value)),
        Some((low, high)) => Some((low.min(value), high.max(value))),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn spec(kind: ChartKind, rows: Value) -> ChartSpec {
        let rows = rows.as_array().cloned().unwrap_or_default();
        ChartSpec::new("Test", kind, rows)
    }

    #[test]
    fn categories_follow_first_appearance() {
        let data = Dataset::build(
            &spec(
                ChartKind::Bar,
                json!([
                    {"x": "Mar", "y": 3}, {"x": "Jan", "y": 1}, {"x": "Feb", "y": 2}
                ]),
            ),
            Locale::En,
        )
        .unwrap();
        assert_eq!(data.categories, vec!["Mar", "Jan", "Feb"]);
        assert!(!data.has_series);
    }

    #[test]
    fn invalid_values_are_skipped_and_counted() {
        let data = Dataset::build(
            &spec(
                ChartKind::Line,
                json!([
                    {"x": "a", "y": 1},
                    {"x": "b", "y": "NaN"},
                    {"x": "c", "y": "Infinity"},
                    {"x": "d", "y": null},
                    {"x": "e"},
                    {"y": 3},
                    "not an object",
                    {"x": [1], "y": 2},
                    {"x": "f", "y": "12"}
                ]),
            ),
            Locale::En,
        )
        .unwrap();
        assert_eq!(data.points.len(), 1);
        assert_eq!(data.skipped, 8);
    }

    #[test]
    fn numeric_categories_are_formatted_per_locale() {
        let data = Dataset::build(
            &spec(
                ChartKind::Bar,
                json!([{"x": 2024, "y": 1}, {"x": 1500.5, "y": 2}]),
            ),
            Locale::Ru,
        )
        .unwrap();
        assert_eq!(data.categories, vec!["2\u{a0}024", "1\u{a0}500,5"]);
    }

    #[test]
    fn series_are_collected_and_rows_without_series_go_to_other() {
        let data = Dataset::build(
            &spec(
                ChartKind::Bar,
                json!([
                    {"x": "Jan", "y": 1, "series": "Gas"},
                    {"x": "Jan", "y": 2, "series": "Power"},
                    {"x": "Feb", "y": 3}
                ]),
            ),
            Locale::UzLatn,
        )
        .unwrap();
        assert_eq!(data.series, vec!["Gas", "Power", "Boshqa"]);
        assert!(data.has_series);
    }

    #[test]
    fn duplicates_keep_the_first_row() {
        let data = Dataset::build(
            &spec(
                ChartKind::Bar,
                json!([{"x": "a", "y": 1}, {"x": "a", "y": 9}]),
            ),
            Locale::En,
        )
        .unwrap();
        assert_eq!(data.points.len(), 1);
        assert_eq!(data.points[0].y, 1.0);
        assert_eq!(data.duplicates, 1);
    }

    #[test]
    fn pie_skips_non_positive_slices() {
        let data = Dataset::build(
            &spec(
                ChartKind::Pie,
                json!([{"x": "a", "y": 1}, {"x": "b", "y": 0}, {"x": "c", "y": -4}]),
            ),
            Locale::En,
        )
        .unwrap();
        assert_eq!(data.points.len(), 1);
        assert_eq!(data.skipped, 2);
    }

    #[test]
    fn scatter_needs_numeric_x() {
        let data = Dataset::build(
            &spec(
                ChartKind::Scatter,
                json!([{"x": 1.5, "y": 1}, {"x": "two", "y": 2}]),
            ),
            Locale::En,
        )
        .unwrap();
        assert_eq!(data.points.len(), 1);
        assert_eq!(data.skipped, 1);
        assert!(data.categories.is_empty());
    }

    #[test]
    fn limits_are_enforced() {
        let many_rows: Vec<Value> = (0..1001).map(|index| json!({"x": index, "y": 1})).collect();
        let error = Dataset::build(&ChartSpec::new("t", ChartKind::Bar, many_rows), Locale::En);
        assert_eq!(
            error,
            Err(ChartError::TooManyRows {
                count: 1001,
                max: 1000
            })
        );

        let many_series: Vec<Value> = (0..9)
            .map(|index| json!({"x": "a", "y": 1, "series": format!("s{index}")}))
            .collect();
        let error = Dataset::build(
            &ChartSpec::new("t", ChartKind::Line, many_series),
            Locale::En,
        );
        assert_eq!(error, Err(ChartError::TooManySeries { count: 9, max: 8 }));

        let many_categories: Vec<Value> =
            (0..201).map(|index| json!({"x": index, "y": 1})).collect();
        let error = Dataset::build(
            &ChartSpec::new("t", ChartKind::Bar, many_categories),
            Locale::En,
        );
        assert_eq!(
            error,
            Err(ChartError::TooManyCategories {
                count: 201,
                max: 200
            })
        );

        let scatter: Vec<Value> = (0..1000).map(|index| json!({"x": index, "y": 1})).collect();
        assert!(
            Dataset::build(
                &ChartSpec::new("t", ChartKind::Scatter, scatter),
                Locale::En
            )
            .is_ok()
        );
    }
}
