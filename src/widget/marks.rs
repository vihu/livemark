//! What concealed marks are drawn as (REFERENCE-001 sections 3, 6 to 9,
//! 11): a dot for a bullet, a checkbox for a task box, a bar for a nested
//! quote's `>` (the outermost level has the gutter bar), a line across for
//! a rule, the info string as a label for a fence, nothing for a setext
//! underline. Their source stays laid out, only transparent, so nothing moves
//! when the caret shows it.
use std::ops::Range;

use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _};
use iced::{Border, Color, Padding, Pixels, Point, Rectangle, Size, Vector, border};

use super::lines::Shaped;
use super::shape::cursor;
use crate::style::{Mark, MarkKind};

/// The colors marks are drawn in.
pub(super) struct Palette {
    /// Dots, empty boxes.
    pub marker: Color,
    /// Bars and rules.
    pub line: Color,
    /// A checked box.
    pub accent: Color,
    /// The tick on a checked box.
    pub tick: Color,
}

/// Draws the concealed `marks` of the line `shaped` at `origin`, whose
/// source text is `line` starting at offset `start`; rules span `width`.
pub(super) fn draw<'a>(
    renderer: &mut iced::Renderer,
    shaped: &Shaped,
    marks: impl Iterator<Item = &'a Mark>,
    (line, start): (&str, usize),
    origin: Point,
    width: f32,
    palette: &Palette,
) {
    let size = shaped.buffer.metrics().font_size;
    let quad = |renderer: &mut iced::Renderer, bounds: Rectangle, border: Border, color| {
        renderer.fill_quad(
            renderer::Quad {
                bounds: bounds + Vector::new(origin.x, origin.y),
                border,
                ..renderer::Quad::default()
            },
            color,
        );
    };
    for mark in marks {
        let Some((rect, baseline)) = box_of(shaped, mark.range.clone()) else {
            continue;
        };
        let middle = baseline - size * 0.32;
        match mark.kind {
            MarkKind::Bullet => {
                let d = (size * 0.3).round();
                let dot = centered(rect.center_x(), middle, d, d);
                quad(renderer, dot, border::rounded(d / 2.0), palette.marker);
            }
            MarkKind::Task(checked) => {
                let bounds = task_box(rect, baseline, size);
                let side = bounds.width;
                if checked {
                    quad(renderer, bounds, border::rounded(3), palette.accent);
                    let text = small("✓", side * 0.9, text::Alignment::Center, bounds.size());
                    renderer.fill_text(
                        text,
                        Point::new(origin.x + bounds.center_x(), origin.y + bounds.center_y()),
                        palette.tick,
                        Rectangle::new(origin + Vector::new(bounds.x, bounds.y), bounds.size()),
                    );
                } else {
                    let edge = Border {
                        color: palette.marker,
                        width: (size / 14.0).max(1.0),
                        radius: 3.0.into(),
                    };
                    quad(renderer, bounds, edge, Color::TRANSPARENT);
                }
            }
            MarkKind::Quote => {
                // The outermost `>` has the gutter bar.
                if line[..mark.range.start - start].contains('>') {
                    let bar = Rectangle::new(
                        Point::new(rect.center_x() - 1.5, 0.0),
                        Size::new(3.0, shaped.height),
                    );
                    quad(renderer, bar, Border::default(), palette.line);
                }
            }
            MarkKind::Fence => {
                let fence = &line[mark.range.start - start..mark.range.end - start];
                let info = fence.trim_start_matches(['`', '~']).trim();
                if !info.is_empty() {
                    let label = Point::new(width, rect.center_y());
                    label_text(
                        renderer,
                        info,
                        size * 0.75,
                        origin + Vector::new(label.x, label.y),
                        palette.marker,
                    );
                }
            }
            MarkKind::Underline => {}
            MarkKind::TableRow(..) | MarkKind::TableRule(_) => {}
            MarkKind::Rule => {
                let thickness = (size / 12.0).max(1.0).round();
                let rule = Rectangle::new(
                    Point::new(0.0, (rect.center_y() - thickness / 2.0).round()),
                    Size::new(width, thickness),
                );
                quad(renderer, rule, Border::default(), palette.line);
            }
        }
    }
}

/// Draws `content` in the code font at `size`, ending at `end` (its right
/// edge, centred vertically there): the code font's advance is 0.6 em, so
/// its width is known without measuring.
fn label_text(renderer: &mut iced::Renderer, content: &str, size: f32, end: Point, color: Color) {
    let width = content.chars().count() as f32 * size * 0.6;
    let bounds = Size::new(width + size, size * 1.5);
    let at = Point::new(end.x - width, end.y);
    let mut label = small(content, size, text::Alignment::Left, bounds);
    label.font = iced::Font::new(crate::fonts::MONO);
    let clip = Rectangle::new(
        Point::new(at.x, at.y - size),
        Size::new(bounds.width, 2.0 * size),
    );
    renderer.fill_text(label, at, color, clip);
}

/// One line of `content` in the prose font at `size`.
fn small(content: &str, size: f32, align_x: text::Alignment, bounds: Size) -> text::Text {
    text::Text {
        content: content.to_owned(),
        bounds,
        size: Pixels(size),
        line_height: text::LineHeight::Relative(1.0),
        font: iced::Font::new(crate::fonts::PROSE),
        align_x,
        align_y: iced::alignment::Vertical::Center,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::None,
        ellipsis: text::Ellipsis::None,
        hint_factor: None,
    }
}

/// A task's checkbox over the box drawn at `rect` with `baseline`, at
/// font `size`.
fn task_box(rect: Rectangle, baseline: f32, size: f32) -> Rectangle {
    let side = (size * 0.8).round();
    centered(rect.center_x(), baseline - size * 0.32, side, side)
}

/// The task box whose checkbox is under `point` (in the line's
/// coordinates) among the concealed `marks` of the line `shaped`.
pub(super) fn task_at<'a>(
    shaped: &Shaped,
    marks: impl Iterator<Item = &'a Mark>,
    point: Point,
) -> Option<Range<usize>> {
    let size = shaped.buffer.metrics().font_size;
    marks
        .filter(|m| matches!(m.kind, MarkKind::Task(_)))
        .find(|m| {
            box_of(shaped, m.range.clone()).is_some_and(|(rect, baseline)| {
                task_box(rect, baseline, size).expand(2.0).contains(point)
            })
        })
        .map(|m| m.range.clone())
}

/// Where the source `range` of `shaped`'s line is drawn (its first row),
/// in the line's coordinates, with that row's baseline.
fn box_of(shaped: &Shaped, range: Range<usize>) -> Option<(Rectangle, f32)> {
    let display = shaped.line.to_display(range.start)..shaped.line.to_display(range.end);
    shaped.stretches(display).into_iter().next()
}

/// A `width` by `height` rectangle centred on `x`, `y`.
fn centered(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
    Rectangle::new(
        Point::new((x - width / 2.0).round(), (y - height / 2.0).round()),
        Size::new(width, height),
    )
}

/// Where source offset `at` of the line `shaped` is drawn, from the line's
/// left edge (on its first row).
pub(super) fn start_x(shaped: &Shaped, at: usize) -> f32 {
    // An empty range has no highlight in cosmic-text: ask for the cursor.
    let at = cursor(shaped.line.to_display(at));
    shaped
        .buffer
        .layout_runs()
        .find_map(|run| run.cursor_position(&at))
        .unwrap_or(0.0)
}

/// Where the bars of the nested quote levels of the quoted line `shaped`
/// stand (its concealed `>` after the first), from the line's left edge: a
/// lazy line after it draws them too (REFERENCE-001 section 6).
pub(super) fn nested_bars<'a>(
    shaped: &Shaped,
    marks: impl Iterator<Item = &'a Mark>,
    (line, start): (&str, usize),
) -> Vec<f32> {
    marks
        .filter(|m| m.kind == MarkKind::Quote && line[..m.range.start - start].contains('>'))
        .filter_map(|m| box_of(shaped, m.range.clone()))
        .map(|(rect, _)| rect.center_x() - 1.5)
        .collect()
}

/// Whether `point` (in the line's coordinates) is on a concealed bullet or
/// quote marker of the line `shaped`, as drawn there.
pub(super) fn on_bullet_or_quote<'a>(
    shaped: &Shaped,
    mut marks: impl Iterator<Item = &'a Mark>,
    point: Point,
) -> bool {
    marks.any(|m| {
        matches!(m.kind, MarkKind::Bullet | MarkKind::Quote)
            && box_of(shaped, m.range.clone())
                .is_some_and(|(rect, _)| rect.expand(Padding::from([0.0, 2.0])).contains(point))
    })
}
