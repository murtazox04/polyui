use serde::{Deserialize, Serialize};

use crate::spec::{ChartKind, Locale};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorToken {
    #[serde(rename = "series-1")]
    Series1,
    #[serde(rename = "series-2")]
    Series2,
    #[serde(rename = "series-3")]
    Series3,
    #[serde(rename = "series-4")]
    Series4,
    #[serde(rename = "series-5")]
    Series5,
    #[serde(rename = "series-6")]
    Series6,
    #[serde(rename = "series-7")]
    Series7,
    #[serde(rename = "series-8")]
    Series8,
    #[serde(rename = "axis")]
    Axis,
    #[serde(rename = "grid")]
    Grid,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "muted")]
    Muted,
}

impl ColorToken {
    pub const SERIES: [ColorToken; 8] = [
        ColorToken::Series1,
        ColorToken::Series2,
        ColorToken::Series3,
        ColorToken::Series4,
        ColorToken::Series5,
        ColorToken::Series6,
        ColorToken::Series7,
        ColorToken::Series8,
    ];

    pub fn series(index: usize) -> ColorToken {
        ColorToken::SERIES
            .get(index % ColorToken::SERIES.len())
            .copied()
            .unwrap_or(ColorToken::Series1)
    }

    pub fn name(self) -> &'static str {
        match self {
            ColorToken::Series1 => "series-1",
            ColorToken::Series2 => "series-2",
            ColorToken::Series3 => "series-3",
            ColorToken::Series4 => "series-4",
            ColorToken::Series5 => "series-5",
            ColorToken::Series6 => "series-6",
            ColorToken::Series7 => "series-7",
            ColorToken::Series8 => "series-8",
            ColorToken::Axis => "axis",
            ColorToken::Grid => "grid",
            ColorToken::Text => "text",
            ColorToken::Muted => "muted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextRole {
    Title,
    AxisTitle,
    Tick,
    Legend,
    Value,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum PathCommand {
    #[serde(rename = "M")]
    MoveTo { x: f64, y: f64 },
    #[serde(rename = "L")]
    LineTo { x: f64, y: f64 },
    #[serde(rename = "A")]
    Arc {
        radius: f64,
        large_arc: bool,
        clockwise: bool,
        x: f64,
        y: f64,
    },
    #[serde(rename = "Z")]
    Close,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mark {
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fill: ColorToken,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: ColorToken,
        stroke_width: f64,
    },
    Path {
        commands: Vec<PathCommand>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<ColorToken>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill_opacity: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<ColorToken>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke_width: Option<f64>,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: ColorToken,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        anchor: TextAnchor,
        role: TextRole,
        color: ColorToken,
        size: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case")]
pub enum HitShape {
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Polygon {
        points: Vec<[f64; 2]>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub row: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(flatten)]
    pub shape: HitShape,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegendEntry {
    pub label: String,
    pub color: ColorToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartScene {
    pub view_box: ViewBox,
    pub kind: ChartKind,
    pub locale: Locale,
    pub title: String,
    pub description: String,
    pub marks: Vec<Mark>,
    pub legend: Vec<LegendEntry>,
    pub hits: Vec<Hit>,
    pub skipped: usize,
}

impl ChartScene {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_tokens_cycle() {
        assert_eq!(ColorToken::series(0), ColorToken::Series1);
        assert_eq!(ColorToken::series(7), ColorToken::Series8);
        assert_eq!(ColorToken::series(8), ColorToken::Series1);
    }

    #[test]
    fn tokens_serialize_as_theme_names() {
        for token in ColorToken::SERIES.into_iter().chain([
            ColorToken::Axis,
            ColorToken::Grid,
            ColorToken::Text,
            ColorToken::Muted,
        ]) {
            let json = serde_json::to_string(&token).unwrap();
            assert_eq!(json, format!("\"{}\"", token.name()));
        }
    }

    #[test]
    fn hit_shape_is_flattened() {
        let hit = Hit {
            row: 2,
            series: None,
            shape: HitShape::Rect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
            },
            label: "Mar: 3".to_owned(),
        };
        let json = serde_json::to_value(&hit).unwrap();
        assert_eq!(json["shape"], "rect");
        assert_eq!(json["width"], 3.0);
        let back: Hit = serde_json::from_value(json).unwrap();
        assert_eq!(back, hit);
    }
}
