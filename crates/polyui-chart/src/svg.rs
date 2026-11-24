use std::fmt::Write;

use serde::{Deserialize, Serialize};

use crate::scene::{ChartScene, ColorToken, Mark, PathCommand, TextAnchor, TextRole};

const FONT_FAMILY: &str = "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    pub series: [String; 8],
    pub axis: String,
    pub grid: String,
    pub text: String,
    pub muted: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
}

impl Palette {
    pub fn light() -> Palette {
        Palette {
            series: [
                "#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f", "#edc948", "#b07aa1",
                "#ff9da7",
            ]
            .map(str::to_owned),
            axis: "#6b7280".to_owned(),
            grid: "#e5e7eb".to_owned(),
            text: "#111827".to_owned(),
            muted: "#6b7280".to_owned(),
            background: Some("#ffffff".to_owned()),
        }
    }

    pub fn dark() -> Palette {
        Palette {
            series: [
                "#7aa6d6", "#ffa95c", "#ff8a8c", "#94d6d1", "#7fcc73", "#ffdf6b", "#cf9ec2",
                "#ffbcc4",
            ]
            .map(str::to_owned),
            axis: "#9ca3af".to_owned(),
            grid: "#374151".to_owned(),
            text: "#f9fafb".to_owned(),
            muted: "#9ca3af".to_owned(),
            background: Some("#111827".to_owned()),
        }
    }

    fn raw(&self, token: ColorToken) -> &str {
        match token {
            ColorToken::Series1 => &self.series[0],
            ColorToken::Series2 => &self.series[1],
            ColorToken::Series3 => &self.series[2],
            ColorToken::Series4 => &self.series[3],
            ColorToken::Series5 => &self.series[4],
            ColorToken::Series6 => &self.series[5],
            ColorToken::Series7 => &self.series[6],
            ColorToken::Series8 => &self.series[7],
            ColorToken::Axis => &self.axis,
            ColorToken::Grid => &self.grid,
            ColorToken::Text => &self.text,
            ColorToken::Muted => &self.muted,
        }
    }

    pub fn color(&self, token: ColorToken) -> String {
        let candidate = self.raw(token).trim();
        if is_safe_color(candidate) {
            candidate.to_owned()
        } else {
            Palette::light().raw(token).to_owned()
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Palette::light()
    }
}

fn is_hex_color(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('#') else {
        return false;
    };
    matches!(digits.len(), 3 | 4 | 6 | 8) && digits.chars().all(|digit| digit.is_ascii_hexdigit())
}

fn is_functional_color(value: &str) -> bool {
    let Some((name, rest)) = value.split_once('(') else {
        return false;
    };
    let Some(arguments) = rest.strip_suffix(')') else {
        return false;
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "rgb" | "rgba" | "hsl" | "hsla" | "oklch" | "oklab" | "lab" | "lch"
    ) && arguments.chars().all(|character| {
        character.is_ascii_digit()
            || matches!(character, ' ' | ',' | '.' | '%' | '/' | '-')
            || matches!(character, 'd' | 'e' | 'g' | 'r' | 'a' | 'n' | 't' | 'u')
    })
}

fn is_named_color(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .chars()
            .all(|character| character.is_ascii_alphabetic())
}

pub fn is_safe_color(value: &str) -> bool {
    is_hex_color(value) || is_functional_color(value) || is_named_color(value)
}

pub fn escape_xml(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            '\t' | '\n' | '\r' => escaped.push(' '),
            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{fffe}' | '\u{ffff}' => {}
            other => escaped.push(other),
        }
    }
    escaped
}

fn number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == 0.0 {
        "0".to_owned()
    } else {
        rounded.to_string()
    }
}

fn path_data(commands: &[PathCommand]) -> String {
    let mut data = String::new();
    for command in commands {
        if !data.is_empty() {
            data.push(' ');
        }
        match command {
            PathCommand::MoveTo { x, y } => {
                let _ = write!(data, "M{} {}", number(*x), number(*y));
            }
            PathCommand::LineTo { x, y } => {
                let _ = write!(data, "L{} {}", number(*x), number(*y));
            }
            PathCommand::Arc {
                radius,
                large_arc,
                clockwise,
                x,
                y,
            } => {
                let radius = number(radius.abs());
                let _ = write!(
                    data,
                    "A{radius} {radius} 0 {} {} {} {}",
                    u8::from(*large_arc),
                    u8::from(*clockwise),
                    number(*x),
                    number(*y)
                );
            }
            PathCommand::Close => data.push('Z'),
        }
    }
    data
}

fn anchor_name(anchor: TextAnchor) -> &'static str {
    match anchor {
        TextAnchor::Start => "start",
        TextAnchor::Middle => "middle",
        TextAnchor::End => "end",
    }
}

fn write_mark(output: &mut String, mark: &Mark, palette: &Palette) {
    let color = |token: ColorToken| escape_xml(&palette.color(token));
    let _ = match mark {
        Mark::Rect {
            x,
            y,
            width,
            height,
            fill,
        } => writeln!(
            output,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
            number(*x),
            number(*y),
            number(width.max(0.0)),
            number(height.max(0.0)),
            color(*fill)
        ),
        Mark::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            stroke_width,
        } => writeln!(
            output,
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}"/>"#,
            number(*x1),
            number(*y1),
            number(*x2),
            number(*y2),
            color(*stroke),
            number(*stroke_width)
        ),
        Mark::Path {
            commands,
            fill,
            fill_opacity,
            stroke,
            stroke_width,
        } => {
            let mut attributes = format!(r#"d="{}""#, path_data(commands));
            match fill {
                Some(fill) => {
                    let _ = write!(attributes, r#" fill="{}""#, color(*fill));
                }
                None => attributes.push_str(r#" fill="none""#),
            }
            if let Some(opacity) = fill_opacity {
                let _ = write!(
                    attributes,
                    r#" fill-opacity="{}""#,
                    number(opacity.clamp(0.0, 1.0))
                );
            }
            if let Some(stroke) = stroke {
                let _ = write!(
                    attributes,
                    r#" stroke="{}" stroke-width="{}" stroke-linejoin="round" stroke-linecap="round""#,
                    color(*stroke),
                    number(stroke_width.unwrap_or(1.0))
                );
            }
            writeln!(output, "<path {attributes}/>")
        }
        Mark::Circle { cx, cy, r, fill } => writeln!(
            output,
            r#"<circle cx="{}" cy="{}" r="{}" fill="{}"/>"#,
            number(*cx),
            number(*cy),
            number(r.abs()),
            color(*fill)
        ),
        Mark::Text {
            x,
            y,
            text,
            anchor,
            role,
            color: token,
            size,
        } => {
            let weight = if *role == TextRole::Title {
                r#" font-weight="600""#
            } else {
                ""
            };
            writeln!(
                output,
                r#"<text x="{}" y="{}" font-size="{}" fill="{}" text-anchor="{}"{weight}>{}</text>"#,
                number(*x),
                number(*y),
                number(*size),
                color(*token),
                anchor_name(*anchor),
                escape_xml(text)
            )
        }
    };
}

pub fn to_svg(scene: &ChartScene, palette: &Palette) -> String {
    let view = scene.view_box;
    let mut output = String::new();
    let _ = writeln!(
        output,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="{} {} {width} {height}" role="img" font-family="{}">"#,
        number(view.x),
        number(view.y),
        escape_xml(FONT_FAMILY),
        width = number(view.width),
        height = number(view.height),
    );
    let _ = writeln!(output, "<title>{}</title>", escape_xml(&scene.title));
    let _ = writeln!(output, "<desc>{}</desc>", escape_xml(&scene.description));
    if let Some(background) = palette
        .background
        .as_deref()
        .map(str::trim)
        .filter(|color| is_safe_color(color))
    {
        let _ = writeln!(
            output,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
            number(view.x),
            number(view.y),
            number(view.width),
            number(view.height),
            escape_xml(background)
        );
    }
    for mark in &scene.marks {
        write_mark(&mut output, mark, palette);
    }
    output.push_str("</svg>\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_characters() {
        assert_eq!(
            escape_xml(r#"<script>alert("x")</script> & 'y'"#),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; &#39;y&#39;"
        );
        assert_eq!(escape_xml("a\u{0}b\u{1b}c\nd"), "abc d");
        assert_eq!(escape_xml("so'm"), "so&#39;m");
    }

    #[test]
    fn rejects_unsafe_colors() {
        assert!(is_safe_color("#fff"));
        assert!(is_safe_color("#4e79a7"));
        assert!(is_safe_color("rgb(10, 20, 30)"));
        assert!(is_safe_color("oklch(70% 0.1 200deg)"));
        assert!(is_safe_color("rebeccapurple"));
        assert!(!is_safe_color("url(https://evil.example/x.svg#a)"));
        assert!(!is_safe_color("red\" onload=\"alert(1)"));
        assert!(!is_safe_color("var(--x)"));
        assert!(!is_safe_color("#12345"));
        assert!(!is_safe_color(""));
        let mut palette = Palette::light();
        palette.axis = "url(#x)".to_owned();
        assert_eq!(palette.color(ColorToken::Axis), "#6b7280");
    }

    #[test]
    fn formats_numbers_compactly() {
        assert_eq!(number(12.0), "12");
        assert_eq!(number(12.345), "12.35");
        assert_eq!(number(-0.001), "0");
        assert_eq!(number(f64::NAN), "0");
    }

    #[test]
    fn builds_path_data() {
        let data = path_data(&[
            PathCommand::MoveTo { x: 1.0, y: 2.0 },
            PathCommand::Arc {
                radius: 5.0,
                large_arc: true,
                clockwise: true,
                x: 3.0,
                y: 4.0,
            },
            PathCommand::Close,
        ]);
        assert_eq!(data, "M1 2 A5 5 0 1 1 3 4 Z");
    }
}
