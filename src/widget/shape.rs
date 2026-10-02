//! Shaping one projected line into a cosmic-text buffer through iced's font
//! system: its heading size, and per run the font, weight, slant and color,
//! with syntax colors for code; wrapped list and quote rows hanging after
//! their prefix.
use std::ops::Range;
use std::sync::Arc;

use iced::Font;
use iced::advanced::graphics::core::color::Oklch;
use iced::advanced::graphics::text::{cosmic_text, font_system, to_attributes, to_color};

use super::highlight::Token;
use crate::fonts;
use crate::layout::Line;
use crate::style::{Style, Styled};

/// Body text size in pixels.
pub const TEXT_SIZE: f32 = 16.0;

/// Line height as a multiple of the text size.
const LINE_HEIGHT: f32 = 1.5;

/// Heading sizes in em by level (Keeprs' web editor: 1.4, 1.25, 1.1).
const HEADING: [f32; 6] = [1.4, 1.25, 1.1, 1.0, 1.0, 1.0];

/// `color` made lighter or darker, its hue kept, until it reads on `on`
/// (4.5:1, WCAG AA), toward the text color `text`: iced's palettes give
/// accents that can be too dark on a dark band.
pub fn readable(color: iced::Color, on: iced::Color, text: iced::Color) -> iced::Color {
    let step = if text.relative_luminance() > on.relative_luminance() {
        0.04
    } else {
        -0.04
    };
    let Oklch { mut l, c, h, a } = color.into_oklch();
    for _ in 0..25 {
        let shifted = iced::Color::from_oklch(Oklch { l, c, h, a });
        if shifted.relative_contrast(on) >= 4.5 {
            return shifted;
        }
        l = (l + step).clamp(0.0, 1.0);
    }
    text
}

/// Colors the lines are shaped with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    pub text: iced::Color,
    pub marker: iced::Color,
    pub code: iced::Color,
    pub link: iced::Color,
}

/// A shaped buffer, the same for every line with the same text and style.
#[derive(Clone)]
pub struct Cached {
    pub buffer: Arc<cosmic_text::Buffer>,
    pub height: f32,
    pub hang: f32,
    pub first_row: f32,
}

/// Loads the bundled fonts into iced's font system, once.
pub fn load_fonts() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let mut system = font_system().write().expect("font system lock");
        for font in fonts::JETBRAINS_MONO
            .into_iter()
            .chain(fonts::ATKINSON_HYPERLEGIBLE_NEXT)
        {
            system.load_font(std::borrow::Cow::Borrowed(font));
        }
    });
}

/// A cosmic-text cursor at display offset `display` of a line's buffer.
pub fn cursor(display: usize) -> cosmic_text::Cursor {
    cosmic_text::Cursor::new(0, display)
}

/// The heading level of the line at `range`, or 0.
pub fn heading_level(styled: &Styled, range: Range<usize>) -> u8 {
    let runs = styled.runs();
    let first = runs.partition_point(|(r, _)| r.end <= range.start);
    runs[first..]
        .iter()
        .take_while(|(r, _)| r.start < range.end)
        .find(|(_, style)| style.heading > 0)
        .map_or(0, |(_, style)| style.heading)
}

/// Shapes `line` with `looks`: its heading level (size), whether all of it
/// is in the code font, and the colors.
pub fn shape(
    line: &Line,
    (level, mono, colors, compact): (u8, bool, Colors, bool),
    hang: Option<usize>,
    width: f32,
    tokens: &[Token],
) -> Cached {
    // A compact line (a table's delimiter row under its grid) is a thin
    // band.
    let scale = if compact {
        0.25
    } else if level == 0 {
        1.0
    } else {
        HEADING[usize::from(level) - 1]
    };
    let size = TEXT_SIZE * scale;
    let metrics = cosmic_text::Metrics::new(size, (size * LINE_HEIGHT).round());
    let mut system = font_system().write().expect("font system lock");
    let raw = system.raw();
    let mut buffer = cosmic_text::Buffer::new(raw, metrics);
    buffer.set_size(Some(width.max(1.0)), None);
    buffer.set_wrap(cosmic_text::Wrap::WordOrGlyph);
    let plain = attrs(Style::default(), colors, mono);
    // One span per stretch where neither the style nor the token changes.
    let mut edges = vec![0, line.text.len()];
    for (range, _) in &line.runs {
        edges.extend([range.start, range.end]);
    }
    for token in tokens {
        edges.extend([token.range.start, token.range.end]);
    }
    edges.sort_unstable();
    edges.dedup();
    let mut spans = Vec::new();
    for pair in edges.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let style = line
            .runs
            .iter()
            .find(|(r, _)| r.start <= from && from < r.end)
            .map_or(Style::default(), |(_, style)| *style);
        let mut attrs = attrs(style, colors, mono);
        let token = tokens
            .iter()
            .find(|t| t.range.start <= from && from < t.range.end);
        // Syntax colors leave markers dimmed; a transparent token (a
        // concealed mark) hides one.
        if let Some(token) = token.filter(|t| !style.marker || t.color.a == 0.0) {
            attrs = attrs.color(to_color(token.color));
            if token.italic {
                attrs = attrs.style(cosmic_text::Style::Italic);
            }
        }
        spans.push((&line.text[from..to], attrs));
    }
    buffer.set_rich_text(spans, &plain, cosmic_text::Shaping::Advanced, None);
    buffer.shape_until_scroll(raw, false);
    // A wrapped list item or quoted line: shaped again narrower by its
    // prefix, so the rows after the first fit when drawn under its text.
    let mut indent = 0.0;
    if let Some(at) = hang
        && buffer.layout_runs().nth(1).is_some()
    {
        let x = buffer
            .layout_runs()
            .find_map(|run| run.cursor_position(&cursor(at)))
            .unwrap_or(0.0);
        if x > 0.0 && x < width / 2.0 {
            buffer.set_size(Some(width - x), None);
            buffer.shape_until_scroll(raw, false);
            indent = x;
        }
    }
    let height = buffer
        .layout_runs()
        .map(|run| run.line_top + run.line_height)
        .fold(metrics.line_height, f32::max);
    let first_row = buffer
        .layout_runs()
        .next()
        .map_or(height, |run| run.line_top + run.line_height);
    Cached {
        buffer: Arc::new(buffer),
        height,
        hang: indent,
        first_row,
    }
}

fn attrs(style: Style, colors: Colors, mono: bool) -> cosmic_text::Attrs<'static> {
    let font = if mono || style.code || style.code_block || style.table || style.mono {
        Font::new(fonts::MONO)
    } else {
        Font::new(fonts::PROSE)
    };
    let mut attrs = to_attributes(font);
    if style.strong || style.heading > 0 {
        attrs = attrs.weight(cosmic_text::Weight::BOLD);
    }
    if style.emphasis {
        attrs = attrs.style(cosmic_text::Style::Italic);
    }
    let color = if style.marker || style.done {
        colors.marker
    } else if style.code {
        colors.code
    } else if style.link {
        colors.link
    } else {
        colors.text
    };
    attrs.color(to_color(color))
}
