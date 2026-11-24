use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::dataset::{Dataset, Point};
use crate::describe::{describe_dataset, no_data_note};
use crate::format::{TickFormat, format_number, format_percent};
use crate::scene::{
    ChartScene, ColorToken, Hit, HitShape, LegendEntry, Mark, PathCommand, TextAnchor, TextRole,
    ViewBox,
};
use crate::spec::{ChartError, ChartKind, ChartOptions, ChartSpec, Locale};
use crate::text::{estimate_text_width, label_step, truncate_to_width, visible_label_indices};
use crate::ticks::{Ticks, nice_ticks};

const PADDING: f64 = 12.0;
const TITLE_SIZE: f64 = 15.0;
const TICK_SIZE: f64 = 11.0;
const LEGEND_SIZE: f64 = 11.0;
const AXIS_TITLE_SIZE: f64 = 11.0;
const NOTE_SIZE: f64 = 13.0;
const SWATCH: f64 = 10.0;
const LABEL_GAP: f64 = 6.0;
const MAX_X_LABEL_WIDTH: f64 = 120.0;
const MAX_LEGEND_LABEL_WIDTH: f64 = 160.0;
const MAX_LEGEND_ROWS: usize = 3;
const LEGEND_ROW_HEIGHT: f64 = 16.0;
const POINT_HIT: f64 = 32.0;
const SCATTER_HIT: f64 = 24.0;
const AREA_OPACITY: f64 = 0.18;
const PIE_LABEL_ROOM: f64 = 22.0;
const PIE_LABEL_MIN_SWEEP: f64 = 0.25;
const ARC_SEGMENT: f64 = PI / 18.0;

fn round2(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

#[derive(Debug, Clone, Copy)]
struct Plot {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Plot {
    fn width(&self) -> f64 {
        (self.right - self.left).max(1.0)
    }

    fn height(&self) -> f64 {
        (self.bottom - self.top).max(1.0)
    }
}

struct Canvas {
    width: f64,
    height: f64,
    locale: Locale,
    marks: Vec<Mark>,
    hits: Vec<Hit>,
}

impl Canvas {
    fn text(&mut self, position: (f64, f64), text: String, anchor: TextAnchor, role: TextRole) {
        if text.is_empty() {
            return;
        }
        let (color, size) = match role {
            TextRole::Title => (ColorToken::Text, TITLE_SIZE),
            TextRole::Legend => (ColorToken::Text, LEGEND_SIZE),
            TextRole::Value => (ColorToken::Text, TICK_SIZE),
            TextRole::Tick => (ColorToken::Muted, TICK_SIZE),
            TextRole::AxisTitle => (ColorToken::Muted, AXIS_TITLE_SIZE),
            TextRole::Note => (ColorToken::Muted, NOTE_SIZE),
        };
        self.marks.push(Mark::Text {
            x: round2(position.0),
            y: round2(position.1),
            text,
            anchor,
            role,
            color,
            size,
        });
    }

    fn line(&mut self, from: (f64, f64), to: (f64, f64), stroke: ColorToken) {
        self.marks.push(Mark::Line {
            x1: round2(from.0),
            y1: round2(from.1),
            x2: round2(to.0),
            y2: round2(to.1),
            stroke,
            stroke_width: 1.0,
        });
    }

    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, fill: ColorToken) {
        self.marks.push(Mark::Rect {
            x: round2(x),
            y: round2(y),
            width: round2(width.max(0.0)),
            height: round2(height.max(0.0)),
            fill,
        });
    }

    fn circle(&mut self, center: (f64, f64), radius: f64, fill: ColorToken) {
        self.marks.push(Mark::Circle {
            cx: round2(center.0),
            cy: round2(center.1),
            r: round2(radius),
            fill,
        });
    }

    fn hit_rect(&mut self, point: &Point, series: Option<&str>, area: Plot, label: String) {
        self.hits.push(Hit {
            row: point.row,
            series: series.map(str::to_owned),
            shape: HitShape::Rect {
                x: round2(area.left),
                y: round2(area.top),
                width: round2(area.right - area.left),
                height: round2(area.bottom - area.top),
            },
            label,
        });
    }
}

fn move_to(x: f64, y: f64) -> PathCommand {
    PathCommand::MoveTo {
        x: round2(x),
        y: round2(y),
    }
}

fn line_to(x: f64, y: f64) -> PathCommand {
    PathCommand::LineTo {
        x: round2(x),
        y: round2(y),
    }
}

fn value_label(
    data: &Dataset,
    name: &str,
    point: &Point,
    unit: Option<&str>,
    locale: Locale,
) -> String {
    let value = format_number(point.y, locale);
    let value = match unit {
        Some(unit) => format!("{value} {unit}"),
        None => value,
    };
    match data.series_label(point.series) {
        Some(series) => format!("{name}, {series}: {value}"),
        None => format!("{name}: {value}"),
    }
}

fn axis_title(label: Option<&str>, unit: Option<&str>) -> Option<String> {
    let label = label.map(str::trim).filter(|label| !label.is_empty());
    match (label, unit) {
        (Some(label), Some(unit)) => Some(format!("{label}, {unit}")),
        (Some(label), None) => Some(label.to_owned()),
        (None, Some(unit)) => Some(unit.to_owned()),
        (None, None) => None,
    }
}

fn legend_entries(spec: &ChartSpec, data: &Dataset) -> Vec<LegendEntry> {
    if spec.kind == ChartKind::Pie {
        return data
            .categories
            .iter()
            .enumerate()
            .map(|(index, label)| LegendEntry {
                label: label.clone(),
                color: ColorToken::series(index),
            })
            .collect();
    }
    if !data.has_series {
        return Vec::new();
    }
    data.series
        .iter()
        .enumerate()
        .map(|(index, label)| LegendEntry {
            label: label.clone(),
            color: ColorToken::series(index),
        })
        .collect()
}

fn draw_legend(canvas: &mut Canvas, labels: &[(String, ColorToken)], top: f64) -> f64 {
    let left = PADDING;
    let right = canvas.width - PADDING;
    let mut x = left;
    let mut row = 0;
    for (index, (label, color)) in labels.iter().enumerate() {
        let text = truncate_to_width(label, LEGEND_SIZE, MAX_LEGEND_LABEL_WIDTH);
        let item_width = SWATCH + 4.0 + estimate_text_width(&text, LEGEND_SIZE) + 14.0;
        if x > left && x + item_width > right {
            row += 1;
            x = left;
        }
        let baseline = top + row as f64 * LEGEND_ROW_HEIGHT + LEGEND_SIZE;
        if row >= MAX_LEGEND_ROWS {
            let hidden = labels.len() - index;
            let last_row = top + (MAX_LEGEND_ROWS - 1) as f64 * LEGEND_ROW_HEIGHT + LEGEND_SIZE;
            canvas.text(
                (right, last_row),
                format!("+{hidden}"),
                TextAnchor::End,
                TextRole::Legend,
            );
            row = MAX_LEGEND_ROWS - 1;
            break;
        }
        canvas.rect(x, baseline - SWATCH + 1.0, SWATCH, SWATCH, *color);
        canvas.text(
            (x + SWATCH + 4.0, baseline),
            text,
            TextAnchor::Start,
            TextRole::Legend,
        );
        x += item_width;
    }
    top + (row + 1) as f64 * LEGEND_ROW_HEIGHT
}

pub fn compile(spec: &ChartSpec, options: &ChartOptions) -> Result<ChartScene, ChartError> {
    let locale = options.locale;
    let data = Dataset::build(spec, locale)?;
    let mut canvas = Canvas {
        width: options.clamped_width(),
        height: options.clamped_height(),
        locale,
        marks: Vec::new(),
        hits: Vec::new(),
    };

    let mut top = PADDING;
    let title = truncate_to_width(spec.title.trim(), TITLE_SIZE, canvas.width - 2.0 * PADDING);
    if !title.is_empty() {
        canvas.text(
            (PADDING, top + TITLE_SIZE),
            title,
            TextAnchor::Start,
            TextRole::Title,
        );
        top += TITLE_SIZE + 10.0;
    }

    let legend = legend_entries(spec, &data);
    if !legend.is_empty() && !data.is_empty() {
        let drawn: Vec<(String, ColorToken)> = if spec.kind == ChartKind::Pie {
            let shares = data.shares();
            legend
                .iter()
                .zip(shares)
                .map(|(entry, share)| {
                    (
                        format!("{} {}", entry.label, format_percent(share, locale)),
                        entry.color,
                    )
                })
                .collect()
        } else {
            legend
                .iter()
                .map(|entry| (entry.label.clone(), entry.color))
                .collect()
        };
        top = draw_legend(&mut canvas, &drawn, top) + 6.0;
    }

    if data.is_empty() {
        canvas.text(
            (canvas.width / 2.0, (top + canvas.height) / 2.0),
            no_data_note(locale).to_owned(),
            TextAnchor::Middle,
            TextRole::Note,
        );
    } else {
        match spec.kind {
            ChartKind::Pie => layout_pie(&mut canvas, spec, &data, top),
            ChartKind::Scatter => layout_scatter(&mut canvas, spec, &data, top),
            ChartKind::Bar | ChartKind::Line | ChartKind::Area => {
                layout_categories(&mut canvas, spec, &data, top)
            }
        }
    }

    Ok(ChartScene {
        view_box: ViewBox {
            x: 0.0,
            y: 0.0,
            width: canvas.width,
            height: canvas.height,
        },
        kind: spec.kind,
        locale,
        title: spec.title.trim().to_owned(),
        description: describe_dataset(spec, &data, locale),
        marks: canvas.marks,
        legend,
        hits: canvas.hits,
        skipped: data.skipped + data.duplicates,
    })
}

struct Frame {
    plot: Plot,
    ticks: Ticks,
}

impl Frame {
    fn y(&self, value: f64) -> f64 {
        self.plot.bottom - self.ticks.fraction(value) * self.plot.height()
    }

    fn baseline(&self) -> f64 {
        let anchor = if self.ticks.min > 0.0 {
            self.ticks.min
        } else if self.ticks.max < 0.0 {
            self.ticks.max
        } else {
            0.0
        };
        self.y(anchor)
    }
}

fn vertical_frame(
    canvas: &mut Canvas,
    spec: &ChartSpec,
    top: f64,
    low: f64,
    high: f64,
) -> (Frame, f64) {
    let mut top = top;
    if let Some(title) = axis_title(spec.y_label.as_deref(), spec.unit_text()) {
        let title = truncate_to_width(&title, AXIS_TITLE_SIZE, canvas.width - 2.0 * PADDING);
        canvas.text(
            (PADDING, top + AXIS_TITLE_SIZE),
            title,
            TextAnchor::Start,
            TextRole::AxisTitle,
        );
        top += AXIS_TITLE_SIZE + 8.0;
    }
    let x_title = spec
        .x_label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty());
    let mut bottom = canvas.height - PADDING - TICK_SIZE - LABEL_GAP;
    if let Some(label) = x_title {
        let label = truncate_to_width(label, AXIS_TITLE_SIZE, canvas.width - 2.0 * PADDING);
        canvas.text(
            (canvas.width / 2.0, canvas.height - PADDING),
            label,
            TextAnchor::Middle,
            TextRole::AxisTitle,
        );
        bottom -= AXIS_TITLE_SIZE + LABEL_GAP;
    }
    let plot_top = top + TICK_SIZE / 2.0;
    let bottom = bottom.max(plot_top + 20.0);
    let target = ((bottom - plot_top) / 40.0).clamp(2.0, 8.0) as usize;
    let ticks = nice_ticks(low, high, target);
    let format = TickFormat::new(&ticks.values, ticks.step, canvas.locale);
    let labels: Vec<String> = ticks
        .values
        .iter()
        .map(|tick| format.format(*tick))
        .collect();
    let widest = labels
        .iter()
        .map(|label| estimate_text_width(label, TICK_SIZE))
        .fold(0.0, f64::max);
    let left = (PADDING + widest + LABEL_GAP).min(canvas.width / 2.0);
    let frame = Frame {
        plot: Plot {
            left,
            top: plot_top,
            right: (canvas.width - PADDING).max(left + 20.0),
            bottom,
        },
        ticks,
    };
    for (tick, label) in frame.ticks.values.iter().zip(labels) {
        let y = frame.y(*tick);
        canvas.line(
            (frame.plot.left, y),
            (frame.plot.right, y),
            ColorToken::Grid,
        );
        canvas.text(
            (frame.plot.left - LABEL_GAP, y + TICK_SIZE * 0.35),
            label,
            TextAnchor::End,
            TextRole::Tick,
        );
    }
    let baseline = frame.baseline();
    (frame, baseline)
}

fn draw_x_labels(canvas: &mut Canvas, plot: Plot, labels: &[(f64, String)], slot: f64) {
    let max_width = MAX_X_LABEL_WIDTH.min(plot.width());
    let fitted: Vec<(f64, String)> = labels
        .iter()
        .map(|(x, label)| (*x, truncate_to_width(label, TICK_SIZE, max_width)))
        .collect();
    let widest = fitted
        .iter()
        .map(|(_, label)| estimate_text_width(label, TICK_SIZE))
        .fold(0.0, f64::max);
    let step = label_step(fitted.len(), slot, widest, LABEL_GAP);
    let baseline = plot.bottom + LABEL_GAP + TICK_SIZE;
    for index in visible_label_indices(fitted.len(), step) {
        let Some((x, label)) = fitted.get(index) else {
            continue;
        };
        let room = 2.0 * x.min(canvas.width - x).max(0.0);
        let label = truncate_to_width(label, TICK_SIZE, room);
        canvas.text((*x, baseline), label, TextAnchor::Middle, TextRole::Tick);
    }
}

fn layout_categories(canvas: &mut Canvas, spec: &ChartSpec, data: &Dataset, top: f64) {
    let Some((low, high)) = data.y_extent() else {
        return;
    };
    let (low, high) = match spec.kind {
        ChartKind::Line => (low, high),
        _ => (low.min(0.0), high.max(0.0)),
    };
    let (frame, baseline) = vertical_frame(canvas, spec, top, low, high);
    let plot = frame.plot;
    let count = data.categories.len().max(1);
    let band = plot.width() / count as f64;
    let center = |category: usize| plot.left + band * (category as f64 + 0.5);

    let labels: Vec<(f64, String)> = data
        .categories
        .iter()
        .enumerate()
        .map(|(index, label)| (center(index), label.clone()))
        .collect();
    draw_x_labels(canvas, plot, &labels, band);

    let unit = spec.unit_text();
    if spec.kind == ChartKind::Bar {
        draw_bars(canvas, data, &frame, baseline, band, unit);
    } else {
        draw_lines(canvas, spec.kind, data, &frame, baseline, band, unit);
    }
    canvas.line(
        (plot.left, baseline),
        (plot.right, baseline),
        ColorToken::Axis,
    );
}

fn draw_bars(
    canvas: &mut Canvas,
    data: &Dataset,
    frame: &Frame,
    baseline: f64,
    band: f64,
    unit: Option<&str>,
) {
    let plot = frame.plot;
    let series_count = data.series.len().max(1) as f64;
    let group = band * 0.72;
    let slot = group / series_count;
    let inset = if series_count > 1.0 { slot * 0.08 } else { 0.0 };
    for point in &data.points {
        let band_left = plot.left + band * point.category as f64;
        let slot_left = band_left + (band - group) / 2.0 + slot * point.series as f64;
        let value_y = frame.y(point.y);
        canvas.rect(
            slot_left + inset,
            value_y.min(baseline),
            slot - 2.0 * inset,
            (value_y - baseline).abs(),
            ColorToken::series(point.series),
        );
        let hit_width = band / series_count;
        let hit_left = band_left + hit_width * point.series as f64;
        let name = data.category_name(point.category);
        let label = value_label(data, name, point, unit, canvas.locale);
        canvas.hit_rect(
            point,
            data.series_label(point.series),
            Plot {
                left: hit_left,
                top: plot.top,
                right: hit_left + hit_width,
                bottom: plot.bottom,
            },
            label,
        );
    }
}

fn draw_lines(
    canvas: &mut Canvas,
    kind: ChartKind,
    data: &Dataset,
    frame: &Frame,
    baseline: f64,
    band: f64,
    unit: Option<&str>,
) {
    let plot = frame.plot;
    let by_slot: HashMap<(usize, usize), &Point> = data
        .points
        .iter()
        .map(|point| ((point.category, point.series), point))
        .collect();
    let x_of = |category: usize| plot.left + band * (category as f64 + 0.5);
    let show_dots = data.categories.len() <= 60;

    for series in 0..data.series.len() {
        let color = ColorToken::series(series);
        let mut runs: Vec<Vec<(f64, f64)>> = Vec::new();
        let mut current: Vec<(f64, f64)> = Vec::new();
        for category in 0..data.categories.len() {
            match by_slot.get(&(category, series)) {
                Some(point) => current.push((x_of(category), frame.y(point.y))),
                None if !current.is_empty() => runs.push(std::mem::take(&mut current)),
                None => {}
            }
        }
        if !current.is_empty() {
            runs.push(current);
        }
        for run in &runs {
            let (Some(first), Some(last)) = (run.first(), run.last()) else {
                continue;
            };
            if kind == ChartKind::Area && run.len() > 1 {
                let mut commands = vec![move_to(first.0, baseline)];
                commands.extend(run.iter().map(|(x, y)| line_to(*x, *y)));
                commands.push(line_to(last.0, baseline));
                commands.push(PathCommand::Close);
                canvas.marks.push(Mark::Path {
                    commands,
                    fill: Some(color),
                    fill_opacity: Some(AREA_OPACITY),
                    stroke: None,
                    stroke_width: None,
                });
            }
            if run.len() > 1 {
                let mut commands = vec![move_to(first.0, first.1)];
                commands.extend(run.iter().skip(1).map(|(x, y)| line_to(*x, *y)));
                canvas.marks.push(Mark::Path {
                    commands,
                    fill: None,
                    fill_opacity: None,
                    stroke: Some(color),
                    stroke_width: Some(2.0),
                });
            }
            if show_dots || run.len() == 1 {
                for (x, y) in run {
                    canvas.circle((*x, *y), 3.0, color);
                }
            }
        }
    }

    let hit_width = band.min(POINT_HIT);
    for point in &data.points {
        let x = x_of(point.category);
        let y = frame.y(point.y);
        let name = data.category_name(point.category);
        let label = value_label(data, name, point, unit, canvas.locale);
        canvas.hit_rect(
            point,
            data.series_label(point.series),
            Plot {
                left: x - hit_width / 2.0,
                top: y - POINT_HIT / 2.0,
                right: x + hit_width / 2.0,
                bottom: y + POINT_HIT / 2.0,
            },
            label,
        );
    }
}

fn layout_scatter(canvas: &mut Canvas, spec: &ChartSpec, data: &Dataset, top: f64) {
    let (Some((low_y, high_y)), Some((low_x, high_x))) = (data.y_extent(), data.x_extent()) else {
        return;
    };
    let (frame, _) = vertical_frame(canvas, spec, top, low_y, high_y);
    let plot = frame.plot;
    let target = (plot.width() / 80.0).clamp(2.0, 10.0) as usize;
    let x_ticks = nice_ticks(low_x, high_x, target);
    let x_of = |value: f64| plot.left + x_ticks.fraction(value) * plot.width();
    let format = TickFormat::new(&x_ticks.values, x_ticks.step, canvas.locale);
    let labels: Vec<(f64, String)> = x_ticks
        .values
        .iter()
        .map(|tick| (x_of(*tick), format.format(*tick)))
        .collect();
    for (x, _) in &labels {
        canvas.line((*x, plot.top), (*x, plot.bottom), ColorToken::Grid);
    }
    let slot = plot.width() / (labels.len().max(2) - 1) as f64;
    draw_x_labels(canvas, plot, &labels, slot);
    canvas.line(
        (plot.left, plot.bottom),
        (plot.right, plot.bottom),
        ColorToken::Axis,
    );

    let unit = spec.unit_text();
    for point in &data.points {
        let center = (x_of(point.x), frame.y(point.y));
        canvas.circle(center, 4.0, ColorToken::series(point.series));
        let name = format_number(point.x, canvas.locale);
        let label = value_label(data, &name, point, unit, canvas.locale);
        canvas.hit_rect(
            point,
            data.series_label(point.series),
            Plot {
                left: center.0 - SCATTER_HIT / 2.0,
                top: center.1 - SCATTER_HIT / 2.0,
                right: center.0 + SCATTER_HIT / 2.0,
                bottom: center.1 + SCATTER_HIT / 2.0,
            },
            label,
        );
    }
}

fn polar(center: (f64, f64), radius: f64, angle: f64) -> (f64, f64) {
    (
        center.0 + radius * angle.cos(),
        center.1 + radius * angle.sin(),
    )
}

fn sector_polygon(center: (f64, f64), radius: f64, start: f64, sweep: f64) -> Vec<[f64; 2]> {
    let segments = (sweep / ARC_SEGMENT).ceil().clamp(1.0, 36.0) as usize;
    let full = sweep >= TAU - 1e-9;
    let mut points = Vec::with_capacity(segments + 2);
    if !full {
        points.push([round2(center.0), round2(center.1)]);
    }
    for step in 0..=segments {
        if full && step == segments {
            break;
        }
        let angle = start + sweep * step as f64 / segments as f64;
        let (x, y) = polar(center, radius, angle);
        points.push([round2(x), round2(y)]);
    }
    points
}

fn layout_pie(canvas: &mut Canvas, spec: &ChartSpec, data: &Dataset, top: f64) {
    let area = Plot {
        left: PADDING,
        top,
        right: canvas.width - PADDING,
        bottom: canvas.height - PADDING,
    };
    let center = (
        (area.left + area.right) / 2.0,
        (area.top + area.bottom) / 2.0,
    );
    let radius = (area.width().min(area.height()) / 2.0 - PIE_LABEL_ROOM).max(10.0);
    let shares = data.shares();
    let unit = spec.unit_text();
    let mut start = -FRAC_PI_2;
    for (point, share) in data.points.iter().zip(shares) {
        let sweep = (share * TAU).clamp(0.0, TAU);
        let end = start + sweep;
        let color = ColorToken::series(point.category);
        if sweep >= TAU - 1e-9 {
            canvas.circle(center, radius, color);
        } else if sweep > 0.0 {
            let (start_x, start_y) = polar(center, radius, start);
            let (end_x, end_y) = polar(center, radius, end);
            canvas.marks.push(Mark::Path {
                commands: vec![
                    move_to(center.0, center.1),
                    line_to(start_x, start_y),
                    PathCommand::Arc {
                        radius: round2(radius),
                        large_arc: sweep > PI,
                        clockwise: true,
                        x: round2(end_x),
                        y: round2(end_y),
                    },
                    PathCommand::Close,
                ],
                fill: Some(color),
                fill_opacity: None,
                stroke: None,
                stroke_width: None,
            });
        }
        let percent = format_percent(share, canvas.locale);
        if sweep >= PIE_LABEL_MIN_SWEEP {
            let middle = start + sweep / 2.0;
            let (x, y) = polar(center, radius + 10.0, middle);
            let anchor = if middle.cos() > 0.3 {
                TextAnchor::Start
            } else if middle.cos() < -0.3 {
                TextAnchor::End
            } else {
                TextAnchor::Middle
            };
            canvas.text(
                (x, y + TICK_SIZE * 0.35),
                percent.clone(),
                anchor,
                TextRole::Value,
            );
        }
        let name = data.category_name(point.category);
        let value = format_number(point.y, canvas.locale);
        let value = match unit {
            Some(unit) => format!("{value} {unit}"),
            None => value,
        };
        canvas.hits.push(Hit {
            row: point.row,
            series: None,
            shape: HitShape::Polygon {
                points: sector_polygon(center, radius, start, sweep),
            },
            label: format!("{name}: {value}, {percent}"),
        });
        start = end;
    }
}
