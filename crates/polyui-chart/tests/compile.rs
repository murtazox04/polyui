use polyui_chart::{
    ChartError, ChartKind, ChartOptions, ChartScene, ChartSpec, ColorToken, HitShape, Locale, Mark,
    Palette, TextAnchor, TextRole, compile, describe, estimate_text_width, to_svg,
};
use serde_json::{Value, json};

fn rows(value: Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}

fn months() -> Vec<Value> {
    rows(json!([
        {"x": "Jan", "y": 900}, {"x": "Feb", "y": 1000}, {"x": "Mar", "y": 1200},
        {"x": "Apr", "y": 700}, {"x": "May", "y": 450}, {"x": "Jun", "y": 300}
    ]))
}

fn texts(scene: &ChartScene, role: TextRole) -> Vec<String> {
    scene
        .marks
        .iter()
        .filter_map(|mark| match mark {
            Mark::Text {
                text, role: found, ..
            } if *found == role => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn rects(scene: &ChartScene) -> Vec<(f64, f64, f64, f64, ColorToken)> {
    scene
        .marks
        .iter()
        .filter_map(|mark| match mark {
            Mark::Rect {
                x,
                y,
                width,
                height,
                fill,
            } => Some((*x, *y, *width, *height, *fill)),
            _ => None,
        })
        .collect()
}

fn all_numbers_finite(scene: &ChartScene) -> bool {
    let json = serde_json::to_value(scene).unwrap();
    fn walk(value: &Value) -> bool {
        match value {
            Value::Number(number) => number.as_f64().is_some_and(f64::is_finite),
            Value::Array(items) => items.iter().all(walk),
            Value::Object(fields) => fields.values().all(walk),
            Value::Null => false,
            Value::Bool(_) | Value::String(_) => true,
        }
    }
    walk(&json)
}

#[test]
fn bar_chart_has_one_bar_and_hit_per_row() {
    let mut spec = ChartSpec::new("Gas", ChartKind::Bar, months());
    spec.unit = Some("so'm".to_owned());
    let scene = compile(&spec, &ChartOptions::with_locale(Locale::UzLatn)).unwrap();
    let bars: Vec<_> = rects(&scene)
        .into_iter()
        .filter(|rect| rect.4 == ColorToken::Series1)
        .collect();
    assert_eq!(bars.len(), 6);
    assert_eq!(scene.hits.len(), 6);
    assert_eq!(scene.hits[2].row, 2);
    assert_eq!(scene.hits[2].label, "Mar: 1\u{a0}200 so'm");
    let tallest = bars.iter().map(|bar| bar.3).fold(0.0, f64::max);
    assert_eq!(bars[2].3, tallest);
    assert!(scene.legend.is_empty());
    assert_eq!(texts(&scene, TextRole::Title), vec!["Gas"]);
    assert!(all_numbers_finite(&scene));
}

#[test]
fn grouped_bars_split_each_band() {
    let data = rows(json!([
        {"x": "Jan", "y": 5, "series": "Gas"}, {"x": "Jan", "y": 7, "series": "Power"},
        {"x": "Feb", "y": 6, "series": "Gas"}, {"x": "Feb", "y": 3, "series": "Power"}
    ]));
    let scene = compile(
        &ChartSpec::new("Bills", ChartKind::Bar, data),
        &ChartOptions::default(),
    )
    .unwrap();
    assert_eq!(scene.legend.len(), 2);
    assert_eq!(scene.legend[1].label, "Power");
    assert_eq!(scene.legend[1].color, ColorToken::Series2);
    let gas: Vec<_> = rects(&scene)
        .into_iter()
        .filter(|rect| rect.4 == ColorToken::Series1)
        .collect();
    let power: Vec<_> = rects(&scene)
        .into_iter()
        .filter(|rect| rect.4 == ColorToken::Series2)
        .collect();
    assert_eq!(gas.len(), 3, "two bars plus one legend swatch");
    assert_eq!(power.len(), 3);
    assert!(gas[1].0 < power[1].0);
    assert!(gas[1].0 + gas[1].2 <= power[1].0);
    assert_eq!(scene.hits[1].series.as_deref(), Some("Power"));
    assert_eq!(scene.hits[1].label, "Jan, Power: 7");
}

#[test]
fn negative_bars_hang_below_the_zero_line() {
    let data = rows(json!([{"x": "a", "y": 10}, {"x": "b", "y": -5}]));
    let scene = compile(
        &ChartSpec::new("Profit", ChartKind::Bar, data),
        &ChartOptions::default(),
    )
    .unwrap();
    let bars: Vec<_> = rects(&scene)
        .into_iter()
        .filter(|rect| rect.4 == ColorToken::Series1)
        .collect();
    let zero = bars[0].1 + bars[0].3;
    assert!(
        (bars[1].1 - zero).abs() < 0.02,
        "negative bar starts at the zero line"
    );
    assert!(bars[1].3 > 0.0);
    assert!(
        texts(&scene, TextRole::Tick)
            .iter()
            .any(|label| label.starts_with('-'))
    );
}

#[test]
fn line_and_area_draw_paths() {
    for kind in [ChartKind::Line, ChartKind::Area] {
        let scene = compile(
            &ChartSpec::new("Trend", kind, months()),
            &ChartOptions::default(),
        )
        .unwrap();
        let paths = scene
            .marks
            .iter()
            .filter(|mark| matches!(mark, Mark::Path { .. }))
            .count();
        assert_eq!(paths, if kind == ChartKind::Area { 2 } else { 1 });
        assert_eq!(scene.hits.len(), 6);
        assert!(all_numbers_finite(&scene));
    }
}

#[test]
fn line_breaks_at_missing_values() {
    let data = rows(json!([
        {"x": "a", "y": 1}, {"x": "b", "y": 2}, {"x": "c", "y": "n/a"},
        {"x": "d", "y": 3}, {"x": "e", "y": 4}
    ]));
    let scene = compile(
        &ChartSpec::new("Gaps", ChartKind::Line, data),
        &ChartOptions::default(),
    )
    .unwrap();
    let paths = scene
        .marks
        .iter()
        .filter(|mark| matches!(mark, Mark::Path { .. }))
        .count();
    assert_eq!(paths, 2);
    assert_eq!(scene.skipped, 1);
}

#[test]
fn pie_slices_carry_percentages() {
    let data = rows(json!([
        {"x": "Gas", "y": 60}, {"x": "Power", "y": 30}, {"x": "Water", "y": 10}
    ]));
    let mut spec = ChartSpec::new("Bills", ChartKind::Pie, data);
    spec.unit = Some("so'm".to_owned());
    let scene = compile(&spec, &ChartOptions::default()).unwrap();
    let values = texts(&scene, TextRole::Value);
    assert_eq!(values, vec!["60%", "30%", "10%"]);
    assert_eq!(scene.legend.len(), 3);
    assert_eq!(scene.hits[0].label, "Gas: 60 so'm, 60%");
    assert!(matches!(scene.hits[0].shape, HitShape::Polygon { .. }));
    let legend = texts(&scene, TextRole::Legend);
    assert_eq!(legend[0], "Gas 60%");
}

#[test]
fn pie_with_one_slice_is_a_circle() {
    let data = rows(json!([{"x": "All", "y": 5}]));
    let scene = compile(
        &ChartSpec::new("One", ChartKind::Pie, data),
        &ChartOptions::default(),
    )
    .unwrap();
    assert!(
        scene
            .marks
            .iter()
            .any(|mark| matches!(mark, Mark::Circle { .. }))
    );
    match &scene.hits[0].shape {
        HitShape::Polygon { points } => assert!(points.len() >= 30),
        HitShape::Rect { .. } => panic!("pie hit must be a polygon"),
    }
}

#[test]
fn scatter_uses_numeric_x() {
    let data = rows(json!([
        {"x": 1.5, "y": 10}, {"x": 3, "y": 20}, {"x": 10, "y": 5}, {"x": "bad", "y": 1}
    ]));
    let mut spec = ChartSpec::new("Spread", ChartKind::Scatter, data);
    spec.x_label = Some("Temperature".to_owned());
    let scene = compile(&spec, &ChartOptions::default()).unwrap();
    let dots = scene
        .marks
        .iter()
        .filter(|mark| matches!(mark, Mark::Circle { .. }))
        .count();
    assert_eq!(dots, 3);
    assert_eq!(scene.skipped, 1);
    assert_eq!(scene.hits[0].label, "1.5: 10");
    assert_eq!(texts(&scene, TextRole::AxisTitle), vec!["Temperature"]);
}

#[test]
fn empty_rows_show_a_note() {
    let scene = compile(
        &ChartSpec::new("Nothing", ChartKind::Bar, Vec::new()),
        &ChartOptions::with_locale(Locale::UzCyrl),
    )
    .unwrap();
    assert_eq!(texts(&scene, TextRole::Note), vec!["Маълумот йўқ"]);
    assert!(scene.hits.is_empty());
    assert_eq!(scene.description, "Nothing. Маълумот йўқ.");
}

#[test]
fn single_row_works_for_every_kind() {
    for kind in [
        ChartKind::Bar,
        ChartKind::Line,
        ChartKind::Area,
        ChartKind::Pie,
        ChartKind::Scatter,
    ] {
        let data = rows(json!([{"x": 4, "y": 42}]));
        let scene = compile(&ChartSpec::new("One", kind, data), &ChartOptions::default()).unwrap();
        assert_eq!(scene.hits.len(), 1, "{kind:?}");
        assert!(all_numbers_finite(&scene), "{kind:?}");
    }
}

#[test]
fn all_equal_values_still_get_an_axis() {
    for kind in [
        ChartKind::Bar,
        ChartKind::Line,
        ChartKind::Area,
        ChartKind::Scatter,
    ] {
        let data = rows(json!([{"x": 1, "y": 7}, {"x": 2, "y": 7}, {"x": 3, "y": 7}]));
        let scene = compile(
            &ChartSpec::new("Flat", kind, data),
            &ChartOptions::default(),
        )
        .unwrap();
        assert!(texts(&scene, TextRole::Tick).len() >= 2, "{kind:?}");
        assert!(all_numbers_finite(&scene), "{kind:?}");
    }
    let zeros = rows(json!([{"x": "a", "y": 0}, {"x": "b", "y": 0}]));
    let scene = compile(
        &ChartSpec::new("Zero", ChartKind::Bar, zeros),
        &ChartOptions::default(),
    )
    .unwrap();
    assert_eq!(scene.hits.len(), 2);
}

#[test]
fn huge_and_tiny_values_stay_finite() {
    let data = rows(json!([
        {"x": "a", "y": 1.7e308}, {"x": "b", "y": -1.7e308}, {"x": "c", "y": 5e-324},
        {"x": "d", "y": 1e15}
    ]));
    for kind in [
        ChartKind::Bar,
        ChartKind::Line,
        ChartKind::Area,
        ChartKind::Pie,
        ChartKind::Scatter,
    ] {
        let scene = compile(
            &ChartSpec::new("Huge", kind, data.clone()),
            &ChartOptions::default(),
        )
        .unwrap();
        assert!(all_numbers_finite(&scene), "{kind:?}");
        let svg = to_svg(&scene, &Palette::light());
        assert!(!svg.contains("NaN") && !svg.contains("inf"), "{kind:?}");
    }
    let pie = rows(json!([{"x": "a", "y": 1.7e308}, {"x": "b", "y": 1.7e308}]));
    let scene = compile(
        &ChartSpec::new("Pie", ChartKind::Pie, pie),
        &ChartOptions::default(),
    )
    .unwrap();
    assert_eq!(texts(&scene, TextRole::Value), vec!["50%", "50%"]);
}

#[test]
fn nan_like_strings_are_skipped() {
    let data = rows(json!([
        {"x": "a", "y": "NaN"}, {"x": "b", "y": "Infinity"}, {"x": "c", "y": "-inf"},
        {"x": "d", "y": "1e999"}, {"x": "e", "y": 3}
    ]));
    let scene = compile(
        &ChartSpec::new("Odd", ChartKind::Line, data),
        &ChartOptions::default(),
    )
    .unwrap();
    assert_eq!(scene.skipped, 4);
    assert_eq!(scene.hits.len(), 1);
    assert!(scene.description.ends_with("4 values skipped."));
}

#[test]
fn x_labels_are_thinned_so_they_never_overlap() {
    let data: Vec<Value> = (0..120)
        .map(|index| json!({"x": format!("Category {index}"), "y": index}))
        .collect();
    let scene = compile(
        &ChartSpec::new("Dense", ChartKind::Bar, data),
        &ChartOptions::default(),
    )
    .unwrap();
    let mut labels: Vec<(f64, f64)> = scene
        .marks
        .iter()
        .filter_map(|mark| match mark {
            Mark::Text {
                x,
                text,
                role: TextRole::Tick,
                anchor: TextAnchor::Middle,
                size,
                ..
            } => Some((*x, estimate_text_width(text, *size))),
            _ => None,
        })
        .collect();
    labels.sort_by(|left, right| left.0.total_cmp(&right.0));
    assert!(labels.len() > 1 && labels.len() < 120);
    for pair in labels.windows(2) {
        let right_edge = pair[0].0 + pair[0].1 / 2.0;
        let left_edge = pair[1].0 - pair[1].1 / 2.0;
        assert!(right_edge < left_edge, "labels overlap: {pair:?}");
    }
    assert_eq!(scene.hits.len(), 120);
}

#[test]
fn limits_return_errors() {
    let data: Vec<Value> = (0..1001).map(|index| json!({"x": index, "y": 1})).collect();
    let error = compile(
        &ChartSpec::new("Big", ChartKind::Scatter, data),
        &ChartOptions::default(),
    );
    assert!(matches!(
        error,
        Err(ChartError::TooManyRows {
            count: 1001,
            max: 1000
        })
    ));
    let message = error.unwrap_err().to_string();
    assert_eq!(message, "chart has 1001 rows; at most 1000 are allowed");
}

#[test]
fn hostile_text_is_escaped_in_svg() {
    let data = rows(json!([
        {"x": "<script>alert(1)</script>", "y": 1, "series": "\"quoted\" & 'single'"},
        {"x": "a&b", "y": 2, "series": "<b>bold</b>"}
    ]));
    let mut spec = ChartSpec::new("<script>alert(\"t\")</script> & co", ChartKind::Bar, data);
    spec.unit = Some("<u>".to_owned());
    spec.y_label = Some("\"y\" & 'z'".to_owned());
    let scene = compile(&spec, &ChartOptions::default()).unwrap();
    let svg = to_svg(&scene, &Palette::light());
    assert!(!svg.contains("<script"));
    assert!(!svg.contains("<b>"));
    assert!(!svg.contains("<u>"));
    assert!(
        svg.contains("<title>&lt;script&gt;alert(&quot;t&quot;)&lt;/script&gt; &amp; co</title>")
    );
    assert!(svg.contains("&quot;quoted&quot; &amp; &#39;single&#39;"));
    assert!(svg.contains("a&amp;b"));
    assert!(!svg.contains("foreignObject"));
    assert!(!svg.contains("href"));
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("role=\"img\""));
    assert!(svg.contains("<desc>"));
    let openings = svg.matches('<').count();
    let elements = svg.lines().count();
    assert!(openings <= elements * 2);
}

#[test]
fn hostile_palette_cannot_inject() {
    let mut palette = Palette::dark();
    palette.series[0] = "url(https://evil.example/a.svg#x)".to_owned();
    palette.text = "red\"/><script>alert(1)</script>".to_owned();
    palette.background = Some("url(#x)".to_owned());
    let scene = compile(
        &ChartSpec::new("T", ChartKind::Bar, months()),
        &ChartOptions::default(),
    )
    .unwrap();
    let svg = to_svg(&scene, &palette);
    assert!(!svg.contains("url("));
    assert!(!svg.contains("<script"));
    assert!(svg.contains("#4e79a7"));
}

#[test]
fn describe_matches_scene_description() {
    let spec = ChartSpec::new("Gas", ChartKind::Bar, months());
    for locale in [Locale::En, Locale::UzLatn, Locale::UzCyrl, Locale::Ru] {
        let scene = compile(&spec, &ChartOptions::with_locale(locale)).unwrap();
        assert_eq!(scene.description, describe(&spec, locale));
    }
}

#[test]
fn scene_round_trips_through_json() {
    let spec: ChartSpec = serde_json::from_value(json!({
        "title": "Gas", "kind": "area", "y_label": "Usage", "unit": "m³",
        "rows": [{"x": "Jan", "y": 3}, {"x": "Feb", "y": 4}]
    }))
    .unwrap();
    let scene = compile(&spec, &ChartOptions::default()).unwrap();
    let json = scene.to_json();
    let back: ChartScene = serde_json::from_str(&json).unwrap();
    assert_eq!(back, scene);
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["marks"][0]["type"], "text");
    assert_eq!(value["hits"][0]["shape"], "rect");
    assert_eq!(value["locale"], "en");
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn pick<'a>(&mut self, items: &'a [Value]) -> &'a Value {
        &items[(self.next() as usize) % items.len()]
    }
}

#[test]
fn arbitrary_rows_never_panic() {
    let scalars = [
        json!(null),
        json!(true),
        json!(0),
        json!(-1),
        json!(1e308),
        json!(-1e308),
        json!(5e-324),
        json!(0.1),
        json!(42),
        json!("NaN"),
        json!("x"),
        json!(""),
        json!("<&>\"'"),
        json!([1]),
        json!({"a": 1}),
        json!(i64::MIN),
        json!(u64::MAX),
        json!("\u{0}\u{1b}\u{fffe}"),
        json!("日本語のラベル"),
        json!("Ўзбекистон"),
    ];
    let kinds = [
        ChartKind::Bar,
        ChartKind::Line,
        ChartKind::Area,
        ChartKind::Pie,
        ChartKind::Scatter,
    ];
    let locales = [Locale::En, Locale::UzLatn, Locale::UzCyrl, Locale::Ru];
    let mut random = Lcg(7);
    for round in 0..300 {
        let count = (random.next() % 60) as usize;
        let data: Vec<Value> = (0..count)
            .map(|_| match random.next() % 6 {
                0 => random.pick(&scalars).clone(),
                1 => json!({"x": random.pick(&scalars), "y": random.pick(&scalars)}),
                _ => json!({
                    "x": random.pick(&scalars),
                    "y": random.pick(&scalars),
                    "series": if random.next().is_multiple_of(3) { random.pick(&scalars).clone() } else { json!("s") }
                }),
            })
            .collect();
        let kind = kinds[round % kinds.len()];
        let options = ChartOptions {
            width: (random.next() % 5000) as u32,
            height: (random.next() % 5000) as u32,
            locale: locales[round % locales.len()],
        };
        let spec = ChartSpec::new(format!("Round {round} <&>"), kind, data);
        match compile(&spec, &options) {
            Ok(scene) => {
                assert!(all_numbers_finite(&scene), "round {round}");
                let svg = to_svg(&scene, &Palette::light());
                assert!(!svg.contains('\u{0}'));
                assert!(!svg.contains("=\"NaN\"") && !svg.contains("inf\""));
            }
            Err(error) => assert!(!error.to_string().is_empty()),
        }
        assert!(!describe(&spec, options.locale).is_empty());
    }
}
