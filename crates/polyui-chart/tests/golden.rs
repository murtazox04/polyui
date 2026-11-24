use std::fs;
use std::path::PathBuf;

use polyui_chart::{ChartKind, ChartOptions, ChartSpec, Locale, Palette, compile, to_svg};
use serde_json::{Value, json};

const UPDATE_VARIABLE: &str = "POLYUI_UPDATE_GOLDEN";

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name)
}

fn check(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os(UPDATE_VARIABLE).is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden file {}; run with {UPDATE_VARIABLE}=1 to create it",
            path.display()
        )
    });
    assert!(
        expected == actual,
        "{name} differs from the golden file; run with {UPDATE_VARIABLE}=1 to accept the change"
    );
}

fn spec(title: &str, kind: ChartKind, rows: Value) -> ChartSpec {
    ChartSpec::new(title, kind, rows.as_array().cloned().unwrap_or_default())
}

fn render(name: &str, spec: &ChartSpec, locale: Locale, palette: &Palette) {
    let scene = compile(spec, &ChartOptions::with_locale(locale)).unwrap();
    check(&format!("{name}.svg"), &to_svg(&scene, palette));
}

fn gas_by_month() -> ChartSpec {
    let mut spec = spec(
        "Gaz uchun to'lovlar",
        ChartKind::Bar,
        json!([
            {"x": "Yan", "y": 182000}, {"x": "Fev", "y": 205500}, {"x": "Mar", "y": 171200},
            {"x": "Apr", "y": 120000}, {"x": "May", "y": 64300}, {"x": "Iyun", "y": 29322.19}
        ]),
    );
    spec.x_label = Some("Oy".to_owned());
    spec.y_label = Some("Summa".to_owned());
    spec.unit = Some("so'm".to_owned());
    spec
}

#[test]
fn golden_bar_uz_latn() {
    let spec = gas_by_month();
    render("bar_uz_latn", &spec, Locale::UzLatn, &Palette::light());
    let scene = compile(&spec, &ChartOptions::with_locale(Locale::UzLatn)).unwrap();
    let pretty = serde_json::to_string_pretty(&scene).unwrap();
    check("bar_uz_latn.json", &format!("{pretty}\n"));
}

#[test]
fn golden_bar_dark() {
    render("bar_dark_en", &gas_by_month(), Locale::En, &Palette::dark());
}

#[test]
fn golden_grouped_bar_with_negatives() {
    let rows = json!([
        {"x": "Q1", "y": 120, "series": "Revenue"}, {"x": "Q1", "y": -40, "series": "Profit"},
        {"x": "Q2", "y": 150, "series": "Revenue"}, {"x": "Q2", "y": 15, "series": "Profit"},
        {"x": "Q3", "y": 90, "series": "Revenue"}, {"x": "Q3", "y": -25, "series": "Profit"},
        {"x": "Q4", "y": 210, "series": "Revenue"}, {"x": "Q4", "y": 60, "series": "Profit"}
    ]);
    let mut spec = spec("Revenue and profit", ChartKind::Bar, rows);
    spec.unit = Some("k USD".to_owned());
    render("grouped_bar_en", &spec, Locale::En, &Palette::light());
}

#[test]
fn golden_line_ru_with_gap() {
    let rows = json!([
        {"x": "Янв", "y": 4.1}, {"x": "Фев", "y": 3.8}, {"x": "Мар", "y": 6.2},
        {"x": "Апр", "y": "нет"}, {"x": "Май", "y": 12.5}, {"x": "Июн", "y": 18.1},
        {"x": "Июл", "y": 21.4}, {"x": "Авг", "y": 20.2}, {"x": "Сен", "y": 15.0},
        {"x": "Окт", "y": 9.6}, {"x": "Ноя", "y": 2.3}, {"x": "Дек", "y": -3.4}
    ]);
    let mut spec = spec("Средняя температура", ChartKind::Line, rows);
    spec.y_label = Some("Температура".to_owned());
    spec.unit = Some("°C".to_owned());
    render("line_ru", &spec, Locale::Ru, &Palette::light());
}

#[test]
fn golden_area_uz_cyrl_series() {
    let mut rows = Vec::new();
    for (index, month) in ["Янв", "Фев", "Мар", "Апр", "Май", "Июн"]
        .iter()
        .enumerate()
    {
        let step = index as f64;
        rows.push(json!({"x": month, "y": 1_200_000.0 + step * 310_000.0, "series": "Газ"}));
        rows.push(json!({"x": month, "y": 800_000.0 + step * 120_000.0, "series": "Электр"}));
    }
    let spec = spec("Коммунал тўловлар", ChartKind::Area, Value::Array(rows));
    render("area_uz_cyrl", &spec, Locale::UzCyrl, &Palette::light());
}

#[test]
fn golden_pie_en() {
    let rows = json!([
        {"x": "Gas", "y": 182000}, {"x": "Power", "y": 96000}, {"x": "Water", "y": 31000},
        {"x": "Internet", "y": 99000}, {"x": "Phone", "y": 25000}, {"x": "Trash", "y": 6000}
    ]);
    let mut spec = spec("Bills this month", ChartKind::Pie, rows);
    spec.unit = Some("so'm".to_owned());
    render("pie_en", &spec, Locale::En, &Palette::light());
}

#[test]
fn golden_scatter_en() {
    let rows: Vec<Value> = (0..40)
        .map(|index| {
            let x = index as f64 * 0.75 - 5.0;
            let y = x * x * 3.0 - x * 12.0 + ((index * 37) % 11) as f64 * 4.0;
            let series = if index % 3 == 0 { "Outdoor" } else { "Indoor" };
            json!({"x": x, "y": y, "series": series})
        })
        .collect();
    let mut spec = spec(
        "Heating load vs temperature",
        ChartKind::Scatter,
        Value::Array(rows),
    );
    spec.x_label = Some("Outside temperature, °C".to_owned());
    spec.y_label = Some("Load".to_owned());
    spec.unit = Some("kWh".to_owned());
    render("scatter_en", &spec, Locale::En, &Palette::light());
}

#[test]
fn golden_dense_labels_en() {
    let rows: Vec<Value> = (1..=90)
        .map(|day| json!({"x": format!("Day {day}"), "y": (day * 7919 % 113) as f64 * 10.5}))
        .collect();
    render(
        "dense_labels_en",
        &spec("Daily usage", ChartKind::Bar, Value::Array(rows)),
        Locale::En,
        &Palette::light(),
    );
}

#[test]
fn golden_hostile_text() {
    let rows = json!([
        {"x": "<script>alert(1)</script>", "y": 3, "series": "\"A\" & 'B'"},
        {"x": "</text><a href=\"javascript:alert(1)\">x</a>", "y": 5, "series": "\"A\" & 'B'"},
        {"x": "Tom & Jerry", "y": 2, "series": "<img src=x onerror=alert(1)>"}
    ]);
    let mut spec = spec(
        "<script>alert(\"title\")</script> & 'quotes'",
        ChartKind::Bar,
        rows,
    );
    spec.unit = Some("<b>".to_owned());
    render("hostile_en", &spec, Locale::En, &Palette::light());
}

#[test]
fn golden_empty_ru() {
    render(
        "empty_ru",
        &spec("Нет строк", ChartKind::Line, json!([])),
        Locale::Ru,
        &Palette::light(),
    );
}
