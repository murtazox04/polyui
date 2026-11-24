#![forbid(unsafe_code)]

mod dataset;
mod describe;
mod format;
mod layout;
mod scene;
mod spec;
mod svg;
mod text;
mod ticks;

pub use describe::describe;
pub use format::{TickFormat, format_compact, format_number, format_percent, format_spoken};
pub use layout::compile;
pub use scene::{
    ChartScene, ColorToken, Hit, HitShape, LegendEntry, Mark, PathCommand, TextAnchor, TextRole,
    ViewBox,
};
pub use spec::{
    ChartError, ChartKind, ChartOptions, ChartSpec, Locale, MAX_CATEGORIES, MAX_ROWS, MAX_SERIES,
};
pub use svg::{Palette, escape_xml, is_safe_color, to_svg};
pub use text::{estimate_text_width, label_step, truncate_to_width, visible_label_indices};
pub use ticks::{Ticks, nice_ticks};

pub fn compile_svg(
    spec: &ChartSpec,
    options: &ChartOptions,
    palette: &Palette,
) -> Result<String, ChartError> {
    compile(spec, options).map(|scene| to_svg(&scene, palette))
}
