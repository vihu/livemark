//! Drawing the visible lines, the selection and the caret.
use iced::advanced::graphics::text::Raw;
use iced::advanced::graphics::text::Renderer as _;
use iced::advanced::renderer::{self, Renderer as _};
use iced::{Color, Rectangle, Size, Theme, Vector};

use std::ops::Range;

use super::Editor;
use super::lines::{self, Colors, TEXT_SIZE};
use crate::style::Style;

/// How far a code block's band reaches past the text on either side.
const CODE_INSET: f32 = 8.0;

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
        };
        let selection_color = palette.primary.weak.color;
        let code_background = palette.background.weak.color;
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
                let range = source.doc.line_range(index);
                // A code block is a band across the text area, fences
                // included; inline code sits on a box of the same color.
                if source.styled.code_block_at(range.clone()).is_some() {
                    let at = origin - Vector::new(CODE_INSET, 0.0);
                    let size = Size::new(area.width + 2.0 * CODE_INSET, shaped.height);
                    quad(renderer, Rectangle::new(at, size), code_background);
                }
                for code in code_spans(&shaped.line.runs) {
                    let (start, end) = (lines::cursor(code.start), lines::cursor(code.end));
                    for run in shaped.buffer.layout_runs() {
                        for (x, width) in run.highlight(start, end) {
                            let at = origin + Vector::new(x - 2.0, run.line_top + 2.0);
                            let size = Size::new(width + 4.0, run.line_height - 4.0);
                            quad(renderer, Rectangle::new(at, size), code_background);
                        }
                    }
                }
                if !selection.is_empty()
                    && selection.start <= range.end
                    && range.start <= selection.end
                {
                    let from = shaped.line.to_display(selection.start.max(range.start));
                    let to = shaped.line.to_display(selection.end.min(range.end));
                    let (start, end) = (lines::cursor(from), lines::cursor(to));
                    let runs: Vec<_> = shaped.buffer.layout_runs().collect();
                    for run in &runs {
                        for (x, width) in run.highlight(start, end) {
                            let at = origin + Vector::new(x, run.line_top);
                            quad(
                                renderer,
                                Rectangle::new(at, Size::new(width, run.line_height)),
                                selection_color,
                            );
                        }
                    }
                    // The line ending is selected too: a sliver past the
                    // end of the last row.
                    if let Some(run) = runs.last().filter(|_| selection.end > range.end) {
                        let at = origin + Vector::new(run.line_w, run.line_top);
                        let size = Size::new(TEXT_SIZE * 0.4, run.line_height);
                        quad(renderer, Rectangle::new(at, size), selection_color);
                    }
                }
                renderer.fill_raw(Raw {
                    buffer: std::sync::Arc::downgrade(&shaped.buffer),
                    position: origin,
                    color: text,
                    clip_bounds: area,
                });
                top += shaped.height;
                index += 1;
                drawn.push(shaped);
            }
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

/// The stretches of inline code, backticks included, in display offsets.
fn code_spans(runs: &[(Range<usize>, Style)]) -> Vec<Range<usize>> {
    let mut spans: Vec<Range<usize>> = Vec::new();
    for (range, _) in runs.iter().filter(|(_, style)| style.code) {
        match spans.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => spans.push(range.clone()),
        }
    }
    spans
}
