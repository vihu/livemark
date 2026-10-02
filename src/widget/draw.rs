//! Drawing the visible lines: code bands and quote bars behind, the
//! selection, the text, strike and link lines over it, and the caret.
use std::ops::Range;

use iced::advanced::graphics::text::Raw;
use iced::advanced::graphics::text::Renderer as _;
use iced::advanced::renderer::{self, Renderer as _};
use iced::{Color, Point, Rectangle, Size, Theme, Vector};

use super::Editor;
use super::lines::{Shaped, marks_in};
use super::marks;
use super::shape::{Colors, TEXT_SIZE};
use crate::style::{MarkKind, Style};

/// How far a code block's band reaches past the text on either side.
const CODE_INSET: f32 = 8.0;

/// Where a quote's bar sits in the gutter left of the text, and its width.
const QUOTE_BAR: (f32, f32) = (10.0, 3.0);

impl Editor {
    /// Draws the visible lines into `area` with the selection, and the
    /// caret when `caret` is set.
    pub(super) fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        area: Rectangle,
        caret: bool,
    ) {
        let palette = theme.palette();
        let text = palette.background.base.text;
        let colors = Colors {
            text,
            marker: Color { a: 0.4, ..text },
            code: palette.primary.base.color,
            link: palette.primary.base.color,
        };
        let selection_color = palette.primary.weak.color;
        let code_background = palette.background.weak.color;
        let quote_bar = palette.background.strong.color;
        let match_color = Color {
            a: 0.35,
            ..palette.warning.base.color
        };
        let mark_palette = marks::Palette {
            marker: colors.marker,
            line: quote_bar,
            accent: palette.primary.base.color,
            tick: palette.primary.base.text,
        };
        let matches = self.find.as_ref().map_or(&[][..], |f| &f.matches[..]);
        let selection = self.doc.selection().range();
        let caret_rect = caret.then(|| self.caret()).flatten();
        let quad = |renderer: &mut iced::Renderer, bounds: Rectangle, color: Color| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    ..renderer::Quad::default()
                },
                color,
            );
        };
        self.with_lines(|lines, source| {
            lines.colors = colors;
            lines.theme = Some(theme.clone());
            let mut drawn = Vec::new();
            let mut index = lines.anchor;
            let mut top = -lines.offset;
            while top < lines.height && index < source.doc.line_count() {
                let shaped = lines.shaped(source, index);
                let origin = area.position() + Vector::new(0.0, top);
                let at = |r: Rectangle| Rectangle::new(origin + Vector::new(r.x, r.y), r.size());
                let range = source.doc.line_range(index);
                // A code block is a band across the text area, fences
                // included; inline code sits on a box of the same color.
                if source.styled.code_block_at(range.clone()).is_some() {
                    let band = Rectangle::new(
                        origin - Vector::new(CODE_INSET, 0.0),
                        Size::new(area.width + 2.0 * CODE_INSET, shaped.height),
                    );
                    quad(renderer, band, code_background);
                }
                if source.styled.in_quote(range.clone()) {
                    let bar = Rectangle::new(
                        origin - Vector::new(QUOTE_BAR.0, 0.0),
                        Size::new(QUOTE_BAR.1, shaped.height),
                    );
                    quad(renderer, bar, quote_bar);
                }
                let in_grid = marks_in(source.concealed, range.clone())
                    .any(|m| matches!(m.kind, MarkKind::TableRow(..) | MarkKind::TableRule(_)));
                // Inline code boxes and the decorations below belong to the
                // source, which a grid covers.
                let boxes = if in_grid {
                    Vec::new()
                } else {
                    spans(&shaped.line.runs, |s| s.code)
                };
                for code in boxes {
                    for (r, _) in shaped.stretches(code) {
                        let r = Rectangle::new(
                            Point::new(r.x - 2.0, r.y + 2.0),
                            Size::new(r.width + 4.0, r.height - 4.0),
                        );
                        quad(renderer, at(r), code_background);
                    }
                }
                // Find matches on this line, except those in hidden text
                // (REFERENCE-001 section 16).
                let first = matches.partition_point(|m| m.end < range.start);
                for found in matches[first..].iter().take_while(|m| m.start <= range.end) {
                    let hidden = source
                        .hidden
                        .iter()
                        .any(|h| h.start < found.end && found.start < h.end);
                    if hidden || found.end > range.end || in_grid {
                        continue;
                    }
                    let from = shaped.line.to_display(found.start);
                    let to = shaped.line.to_display(found.end);
                    for (r, _) in shaped.stretches(from..to) {
                        quad(renderer, at(r), match_color);
                    }
                }
                if !selection.is_empty()
                    && selection.start <= range.end
                    && range.start <= selection.end
                {
                    let from = shaped.line.to_display(selection.start.max(range.start));
                    let to = shaped.line.to_display(selection.end.min(range.end));
                    for (r, _) in shaped.stretches(from..to) {
                        quad(renderer, at(r), selection_color);
                    }
                    // The line ending is selected too: a sliver past the
                    // end of the last row.
                    if let Some(run) = shaped
                        .buffer
                        .layout_runs()
                        .last()
                        .filter(|_| selection.end > range.end)
                    {
                        let x = run.line_w + shaped.shift(run.line_top);
                        let size = Size::new(TEXT_SIZE * 0.4, run.line_height);
                        let sliver = Rectangle::new(Point::new(x, run.line_top), size);
                        quad(renderer, at(sliver), selection_color);
                    }
                }
                draw_text(renderer, &shaped, origin, area, text);
                // A table outside the selection: its grid over the rows.
                for mark in marks_in(source.concealed, range.clone()) {
                    let (MarkKind::TableRow(table, _) | MarkKind::TableRule(table)) = mark.kind
                    else {
                        continue;
                    };
                    let grid = lines.grid(source, table);
                    // The band's own line is shaped thin: it lines up with
                    // the header row's.
                    let x = match mark.kind {
                        MarkKind::TableRule(_) => {
                            let header = source.styled.tables()[table].rows[0].0.start;
                            let shaped = lines.shaped(source, source.doc.line_at(header));
                            marks::start_x(&shaped, header)
                        }
                        _ => marks::start_x(&shaped, mark.range.start),
                    };
                    let at = origin + Vector::new(x, 0.0);
                    if let MarkKind::TableRow(_, row) = mark.kind {
                        let colors = (text, quote_bar, code_background);
                        grid.draw_row(renderer, row, at, shaped.height, area, colors);
                    } else {
                        quad(
                            renderer,
                            Rectangle::new(at, Size::new(grid.width(), shaped.height)),
                            quote_bar,
                        );
                    }
                }
                let line_text = &source.doc.text()[range.clone()];
                marks::draw(
                    renderer,
                    &shaped,
                    marks_in(source.concealed, range.clone()),
                    (line_text, range.start),
                    origin,
                    area.width,
                    &mark_palette,
                );
                // Strike lines through struck text and done tasks, a line
                // under link text: iced's renderers draw no text
                // decorations.
                let size = shaped.buffer.metrics().font_size;
                let thickness = (size / 16.0).max(1.0);
                let decorations = [
                    (
                        spans(&shaped.line.runs, |s| s.strikethrough && !s.marker),
                        -0.3,
                        text,
                    ),
                    (
                        spans(&shaped.line.runs, |s| s.done && !s.marker),
                        -0.3,
                        colors.marker,
                    ),
                    (
                        spans(&shaped.line.runs, |s| s.link && !s.marker),
                        0.15,
                        colors.link,
                    ),
                ];
                for (stretch, rise, color) in decorations.into_iter().filter(|_| !in_grid) {
                    for range in stretch {
                        for (r, baseline) in shaped.stretches(range) {
                            let line = Rectangle::new(
                                Point::new(r.x, baseline + rise * size),
                                Size::new(r.width, thickness),
                            );
                            quad(renderer, at(line), color);
                        }
                    }
                }
                top += shaped.height;
                index += 1;
                drawn.push(shaped);
            }
            lines.visible = drawn.len();
            lines.trim(&drawn);
        });
        if let Some(rect) = caret_rect {
            let bounds = Rectangle::new(area.position() + Vector::new(rect.x, rect.y), rect.size());
            if let Some(bounds) = bounds.intersection(&area) {
                quad(renderer, bounds, text);
            }
        }
    }
}

/// Draws a line's text at `origin`; a hanging line twice, its first row
/// in place and the rest shifted right under its text, each clipped to its
/// rows.
fn draw_text(
    renderer: &mut iced::Renderer,
    shaped: &Shaped,
    origin: Point,
    area: Rectangle,
    color: Color,
) {
    let buffer = std::sync::Arc::downgrade(&shaped.buffer);
    if shaped.hang == 0.0 {
        renderer.fill_raw(Raw {
            buffer,
            position: origin,
            color,
            clip_bounds: area,
        });
        return;
    }
    let split = origin.y + shaped.first_row;
    let first = Rectangle::new(
        Point::new(area.x, area.y.max(origin.y)),
        Size::new(area.width, (split - area.y.max(origin.y)).max(0.0)),
    );
    let rest = Rectangle::new(
        Point::new(area.x, split.max(area.y)),
        Size::new(
            area.width,
            (area.y + area.height - split.max(area.y)).max(0.0),
        ),
    );
    for (position, clip) in [
        (origin, first),
        (origin + Vector::new(shaped.hang, 0.0), rest),
    ] {
        renderer.fill_raw(Raw {
            buffer: buffer.clone(),
            position,
            color,
            clip_bounds: clip,
        });
    }
}

/// The stretches of runs with `pick` set, neighbours joined, in display
/// offsets.
fn spans(runs: &[(Range<usize>, Style)], pick: fn(&Style) -> bool) -> Vec<Range<usize>> {
    let mut spans: Vec<Range<usize>> = Vec::new();
    for (range, _) in runs.iter().filter(|(_, style)| pick(style)) {
        match spans.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => spans.push(range.clone()),
        }
    }
    spans
}
