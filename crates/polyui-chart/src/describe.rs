use crate::dataset::{Dataset, Point};
use crate::format::{format_percent, format_spoken};
use crate::spec::{ChartKind, ChartSpec, Locale};

struct Extreme {
    name: String,
    value: String,
}

fn title_sentence(title: &str) -> Option<String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.ends_with(['.', '!', '?']) {
        Some(trimmed.to_owned())
    } else {
        Some(format!("{trimmed}."))
    }
}

fn with_unit(number: String, unit: Option<&str>) -> String {
    match unit {
        Some(unit) => format!("{number} {unit}"),
        None => number,
    }
}

fn point_name(data: &Dataset, point: &Point) -> String {
    let category = data.category_name(point.category);
    match data.series_label(point.series) {
        Some(series) if data.series.len() > 1 => format!("{category} ({series})"),
        _ => category.to_owned(),
    }
}

fn highest_and_lowest(points: &[Point]) -> Option<(&Point, &Point)> {
    let first = points.first()?;
    let (high, low) = points.iter().fold((first, first), |(high, low), point| {
        (
            if point.y > high.y { point } else { high },
            if point.y < low.y { point } else { low },
        )
    });
    Some((high, low))
}

fn no_data(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "No data.",
        Locale::UzLatn => "Ma'lumot yo'q.",
        Locale::UzCyrl => "Маълумот йўқ.",
        Locale::Ru => "Нет данных.",
    }
}

pub fn too_much_data(locale: Locale) -> &'static str {
    match locale {
        Locale::En => "Too much data to show.",
        Locale::UzLatn => "Ma'lumot juda ko'p.",
        Locale::UzCyrl => "Маълумот жуда кўп.",
        Locale::Ru => "Слишком много данных.",
    }
}

pub fn no_data_note(locale: Locale) -> &'static str {
    no_data(locale).trim_end_matches('.')
}

fn extremes_sentence(high: &Extreme, low: &Extreme, locale: Locale) -> String {
    match locale {
        Locale::En => format!(
            "Highest is {} at {}, lowest is {} at {}.",
            high.name, high.value, low.name, low.value
        ),
        Locale::UzLatn => format!(
            "Eng yuqori qiymat: {}, {}. Eng past qiymat: {}, {}.",
            high.name, high.value, low.name, low.value
        ),
        Locale::UzCyrl => format!(
            "Энг юқори қиймат: {}, {}. Энг паст қиймат: {}, {}.",
            high.name, high.value, low.name, low.value
        ),
        Locale::Ru => format!(
            "Наибольшее значение: {}, {}. Наименьшее значение: {}, {}.",
            high.name, high.value, low.name, low.value
        ),
    }
}

fn shares_sentence(high: &Extreme, low: &Extreme, locale: Locale) -> String {
    match locale {
        Locale::En => format!(
            "Largest share is {} at {}, smallest is {} at {}.",
            high.name, high.value, low.name, low.value
        ),
        Locale::UzLatn => format!(
            "Eng katta ulush: {}, {}. Eng kichik ulush: {}, {}.",
            high.name, high.value, low.name, low.value
        ),
        Locale::UzCyrl => format!(
            "Энг катта улуш: {}, {}. Энг кичик улуш: {}, {}.",
            high.name, high.value, low.name, low.value
        ),
        Locale::Ru => format!(
            "Наибольшая доля: {}, {}. Наименьшая доля: {}, {}.",
            high.name, high.value, low.name, low.value
        ),
    }
}

fn single_sentence(only: &Extreme, locale: Locale) -> String {
    match locale {
        Locale::En => format!("{} is {}.", only.name, only.value),
        Locale::UzLatn | Locale::UzCyrl | Locale::Ru => format!("{}: {}.", only.name, only.value),
    }
}

fn range_sentence(low: &str, high: &str, locale: Locale) -> String {
    match locale {
        Locale::En => format!("Values range from {low} to {high}."),
        Locale::UzLatn => format!("Eng kichik qiymat: {low}. Eng katta qiymat: {high}."),
        Locale::UzCyrl => format!("Энг кичик қиймат: {low}. Энг катта қиймат: {high}."),
        Locale::Ru => format!("Значения от {low} до {high}."),
    }
}

fn count_sentence(count: usize, series: usize, slices: bool, locale: Locale) -> String {
    match locale {
        Locale::En => {
            let noun = match (slices, count == 1) {
                (true, true) => "slice",
                (true, false) => "slices",
                (false, true) => "point",
                (false, false) => "points",
            };
            if series > 1 {
                format!("{count} {noun} in {series} series.")
            } else {
                format!("{count} {noun}.")
            }
        }
        Locale::UzLatn => {
            let noun = if slices { "bo'lak" } else { "nuqta" };
            if series > 1 {
                format!("Jami {count} ta {noun}, {series} ta seriya.")
            } else {
                format!("Jami {count} ta {noun}.")
            }
        }
        Locale::UzCyrl => {
            let noun = if slices { "бўлак" } else { "нуқта" };
            if series > 1 {
                format!("Жами {count} та {noun}, {series} та серия.")
            } else {
                format!("Жами {count} та {noun}.")
            }
        }
        Locale::Ru => {
            let noun = if slices {
                "секторов"
            } else {
                "точек"
            };
            if series > 1 {
                format!("Всего {noun}: {count}, серий: {series}.")
            } else {
                format!("Всего {noun}: {count}.")
            }
        }
    }
}

fn skipped_sentence(count: usize, locale: Locale) -> String {
    match locale {
        Locale::En if count == 1 => "1 value skipped.".to_owned(),
        Locale::En => format!("{count} values skipped."),
        Locale::UzLatn => format!("{count} ta qiymat o'tkazib yuborildi."),
        Locale::UzCyrl => format!("{count} та қиймат ўтказиб юборилди."),
        Locale::Ru => format!("Пропущено значений: {count}."),
    }
}

fn body_sentences(spec: &ChartSpec, data: &Dataset, locale: Locale) -> Vec<String> {
    let unit = spec.unit_text();
    let Some((high, low)) = highest_and_lowest(&data.points) else {
        return vec![no_data(locale).to_owned()];
    };
    let count = data.points.len();
    let spoken = |value: f64| with_unit(format_spoken(value, locale), unit);
    let mut sentences = Vec::new();
    match spec.kind {
        ChartKind::Pie => {
            let shares = data.shares();
            let share = |point: &Point| {
                let fraction = data
                    .points
                    .iter()
                    .position(|candidate| candidate.row == point.row)
                    .and_then(|index| shares.get(index))
                    .copied()
                    .unwrap_or(0.0);
                Extreme {
                    name: point_name(data, point),
                    value: format_percent(fraction, locale),
                }
            };
            if count == 1 {
                sentences.push(single_sentence(&share(high), locale));
            } else {
                sentences.push(shares_sentence(&share(high), &share(low), locale));
            }
            sentences.push(count_sentence(count, 1, true, locale));
        }
        ChartKind::Scatter => {
            if count == 1 {
                let only = Extreme {
                    name: format!("x {}", format_spoken(high.x, locale)),
                    value: spoken(high.y),
                };
                sentences.push(single_sentence(&only, locale));
            } else {
                sentences.push(range_sentence(&spoken(low.y), &spoken(high.y), locale));
            }
            sentences.push(count_sentence(count, data.series.len(), false, locale));
        }
        ChartKind::Bar | ChartKind::Line | ChartKind::Area => {
            let extreme = |point: &Point| Extreme {
                name: point_name(data, point),
                value: spoken(point.y),
            };
            if count == 1 {
                sentences.push(single_sentence(&extreme(high), locale));
            } else {
                sentences.push(extremes_sentence(&extreme(high), &extreme(low), locale));
            }
            sentences.push(count_sentence(count, data.series.len(), false, locale));
        }
    }
    sentences
}

pub fn describe_dataset(spec: &ChartSpec, data: &Dataset, locale: Locale) -> String {
    let mut sentences: Vec<String> = title_sentence(&spec.title).into_iter().collect();
    sentences.extend(body_sentences(spec, data, locale));
    let skipped = data.skipped + data.duplicates;
    if skipped > 0 {
        sentences.push(skipped_sentence(skipped, locale));
    }
    sentences.join(" ")
}

pub fn describe(spec: &ChartSpec, locale: Locale) -> String {
    match Dataset::build(spec, locale) {
        Ok(data) => describe_dataset(spec, &data, locale),
        Err(_) => title_sentence(&spec.title)
            .into_iter()
            .chain([too_much_data(locale).to_owned()])
            .collect::<Vec<_>>()
            .join(" "),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn gas() -> ChartSpec {
        let rows = json!([
            {"x": "January", "y": 900}, {"x": "February", "y": 1000},
            {"x": "March", "y": 1200}, {"x": "April", "y": 700},
            {"x": "May", "y": 450}, {"x": "July", "y": 300}
        ]);
        let mut spec = ChartSpec::new(
            "Gas usage by month",
            ChartKind::Bar,
            rows.as_array().cloned().unwrap_or_default(),
        );
        spec.unit = Some("cubic meters".to_owned());
        spec
    }

    #[test]
    fn english_summary() {
        assert_eq!(
            describe(&gas(), Locale::En),
            "Gas usage by month. Highest is March at 1.2 thousand cubic meters, \
             lowest is July at 300 cubic meters. 6 points."
        );
    }

    #[test]
    fn uzbek_and_russian_summaries() {
        let mut spec = gas();
        spec.unit = Some("m³".to_owned());
        assert_eq!(
            describe(&spec, Locale::UzLatn),
            "Gas usage by month. Eng yuqori qiymat: March, 1,2 ming m³. \
             Eng past qiymat: July, 300 m³. Jami 6 ta nuqta."
        );
        assert_eq!(
            describe(&spec, Locale::UzCyrl),
            "Gas usage by month. Энг юқори қиймат: March, 1,2 минг m³. \
             Энг паст қиймат: July, 300 m³. Жами 6 та нуқта."
        );
        assert_eq!(
            describe(&spec, Locale::Ru),
            "Gas usage by month. Наибольшее значение: March, 1,2 тысячи m³. \
             Наименьшее значение: July, 300 m³. Всего точек: 6."
        );
    }

    #[test]
    fn empty_and_skipped() {
        let empty = ChartSpec::new("Nothing", ChartKind::Line, Vec::new());
        assert_eq!(describe(&empty, Locale::En), "Nothing. No data.");
        assert_eq!(describe(&empty, Locale::Ru), "Nothing. Нет данных.");
        let rows = vec![json!({"x": "a", "y": 2}), json!({"x": "b", "y": "NaN"})];
        let spec = ChartSpec::new("Mixed", ChartKind::Bar, rows);
        assert_eq!(
            describe(&spec, Locale::En),
            "Mixed. a is 2. 1 point. 1 value skipped."
        );
        assert_eq!(
            describe(&spec, Locale::UzLatn),
            "Mixed. a: 2. Jami 1 ta nuqta. 1 ta qiymat o'tkazib yuborildi."
        );
    }

    #[test]
    fn pie_summary_uses_shares() {
        let rows = vec![
            json!({"x": "Gas", "y": 60}),
            json!({"x": "Power", "y": 30}),
            json!({"x": "Water", "y": 10}),
        ];
        let spec = ChartSpec::new("Bills", ChartKind::Pie, rows);
        assert_eq!(
            describe(&spec, Locale::En),
            "Bills. Largest share is Gas at 60%, smallest is Water at 10%. 3 slices."
        );
        assert_eq!(
            describe(&spec, Locale::Ru),
            "Bills. Наибольшая доля: Gas, 60\u{a0}%. Наименьшая доля: Water, 10\u{a0}%. Всего секторов: 3."
        );
    }

    #[test]
    fn series_and_scatter_summaries() {
        let rows = vec![
            json!({"x": "Jan", "y": 5, "series": "Gas"}),
            json!({"x": "Jan", "y": 9, "series": "Power"}),
        ];
        let spec = ChartSpec::new("Usage", ChartKind::Line, rows);
        assert_eq!(
            describe(&spec, Locale::En),
            "Usage. Highest is Jan (Power) at 9, lowest is Jan (Gas) at 5. 2 points in 2 series."
        );
        let rows = vec![json!({"x": 1, "y": 5}), json!({"x": 2, "y": 2500})];
        let spec = ChartSpec::new("Spread?", ChartKind::Scatter, rows);
        assert_eq!(
            describe(&spec, Locale::En),
            "Spread? Values range from 5 to 2.5 thousand. 2 points."
        );
    }

    #[test]
    fn limit_errors_still_describe() {
        let rows: Vec<Value> = (0..1001).map(|index| json!({"x": index, "y": 1})).collect();
        let spec = ChartSpec::new("Big", ChartKind::Bar, rows);
        assert_eq!(describe(&spec, Locale::UzCyrl), "Big. Маълумот жуда кўп.");
        let untitled = ChartSpec::new("  ", ChartKind::Bar, Vec::new());
        assert_eq!(describe(&untitled, Locale::En), "No data.");
    }
}
