//! Scrolling and hit testing over the shaped lines: the furthest scroll,
//! the line at a height, the source offset under a point, the caret's
//! rectangle and row, and bringing an offset into view.
use std::ops::Range;

use iced::advanced::graphics::text::cosmic_text;

use super::lines::{Lines, Source};
use crate::layout::Affinity;

impl Lines {
    /// The furthest scroll: the anchor line and offset with the last
    /// line's bottom at the view's bottom, or the top when the document
    /// is shorter than the view.
    pub fn end(&mut self, source: &Source) -> (usize, f32) {
        let mut index = source.doc.line_count() - 1;
        let mut room = self.height;
        loop {
            let height = self.shaped(source, index).height;
            if height >= room {
                return (index, height - room);
            }
            if index == 0 {
                return (0, 0.0);
            }
            room -= height;
            index -= 1;
        }
    }

    /// The scroll position in lines: the anchor plus the share of it
    /// scrolled past.
    pub fn position(&mut self, source: &Source, (anchor, offset): (usize, f32)) -> f32 {
        let height = self.shaped(source, anchor).height.max(1.0);
        anchor as f32 + (offset / height).clamp(0.0, 1.0)
    }

    /// Scrolls by `dy` pixels (positive: further down the document),
    /// stopping with the first line at the top or the document's end at
    /// the bottom.
    pub fn scroll_by(&mut self, source: &Source, dy: f32) {
        self.offset += dy;
        while self.offset < 0.0 {
            if self.anchor == 0 {
                self.offset = 0.0;
                return;
            }
            self.anchor -= 1;
            self.offset += self.shaped(source, self.anchor).height;
        }
        let last = source.doc.line_count() - 1;
        loop {
            let height = self.shaped(source, self.anchor).height;
            if self.offset < height || self.anchor >= last {
                break;
            }
            self.offset -= height;
            self.anchor += 1;
        }
        self.stop_at_end(source);
    }

    /// Keeps the view from scrolling past the document's end. Only near
    /// the end: finding it shapes the last lines (and highlights code down
    /// to them), so far from it nothing is measured (no row is under 8
    /// pixels).
    pub fn stop_at_end(&mut self, source: &Source) {
        let last = source.doc.line_count() - 1;
        if last - self.anchor.min(last) > (self.height / 8.0) as usize + 1 {
            return;
        }
        let (end, end_offset) = self.end(source);
        if self.anchor > end || (self.anchor == end && self.offset > end_offset) {
            self.anchor = end;
            self.offset = end_offset;
        }
    }

    /// The line at height `y` of the visible area and the top of that
    /// line, walking up or down from the anchor line.
    pub fn line_at_y(&mut self, source: &Source, y: f32) -> (usize, f32) {
        let mut index = self.anchor.min(source.doc.line_count() - 1);
        let mut top = -self.offset;
        while y < top && index > 0 {
            index -= 1;
            top -= self.shaped(source, index).height;
        }
        loop {
            let height = self.shaped(source, index).height;
            if y < top + height || index + 1 >= source.doc.line_count() {
                return (index, top);
            }
            top += height;
            index += 1;
        }
    }

    /// The source offset at a point of the visible area, and which side
    /// of it was clicked: where hidden text sits between two glyphs, the
    /// offset attaches to the glyph clicked (REFERENCE-001 section 14), and
    /// at a soft wrap the side tells which row the caret is drawn on.
    pub fn hit(&mut self, source: &Source, x: f32, y: f32) -> (usize, Affinity) {
        let (index, top) = self.line_at_y(source, y);
        let shaped = self.shaped(source, index);
        let y = y - top;
        let row_top = if y >= shaped.first_row {
            shaped.first_row
        } else {
            0.0
        };
        let x = x - shaped.shift(row_top);
        match shaped.buffer.hit(x, y) {
            Some(cursor) => {
                let affinity = match cursor.affinity {
                    cosmic_text::Affinity::Before => Affinity::Before,
                    cosmic_text::Affinity::After => Affinity::After,
                };
                // At the line's end, after its hidden markers, as End goes
                // (REFERENCE-001 section 14).
                let side = if cursor.index >= shaped.line.text.len() {
                    Affinity::After
                } else {
                    affinity
                };
                (shaped.line.to_source(cursor.index, side), affinity)
            }
            None => (source.doc.line_range(index).end, Affinity::Before),
        }
    }

    /// The top of line `index` in the visible area, walking from the
    /// anchor; `None` when it is more than a screen away.
    pub fn top_of(&mut self, source: &Source, index: usize) -> Option<f32> {
        let mut top = -self.offset;
        let mut at = self.anchor;
        while at > index {
            at -= 1;
            top -= self.shaped(source, at).height;
            if top < -2.0 * self.height {
                return None;
            }
        }
        while at < index {
            top += self.shaped(source, at).height;
            at += 1;
            if top > 2.0 * self.height {
                return None;
            }
        }
        Some(top)
    }

    /// The caret at `offset` as (x, top, height) within its line, on the
    /// row before a soft wrap with [`Affinity::Before`], else after it.
    pub fn caret_in_line(
        &mut self,
        source: &Source,
        offset: usize,
        side: Affinity,
    ) -> (f32, f32, f32) {
        let shaped = self.shaped(source, source.doc.line_at(offset));
        let cursor = side_cursor(shaped.line.to_display(offset), side);
        let runs: Vec<_> = shaped.buffer.layout_runs().collect();
        if let Some((run, x)) = row_of(&runs, &cursor, side) {
            return (
                x + shaped.shift(run.line_top),
                run.line_top,
                run.line_height,
            );
        }
        runs.last().map_or((0.0, 0.0, shaped.height), |run| {
            let x = run.line_w + shaped.shift(run.line_top);
            (x, run.line_top, run.line_height)
        })
    }

    /// The source offsets at the start and end of the visual row the caret
    /// at `offset` is on: before hidden text at the start, after it at the
    /// end, so typing there lands outside the hidden markers.
    pub fn row_bounds(&mut self, source: &Source, offset: usize, side: Affinity) -> Range<usize> {
        let shaped = self.shaped(source, source.doc.line_at(offset));
        let cursor = side_cursor(shaped.line.to_display(offset), side);
        let runs: Vec<_> = shaped.buffer.layout_runs().collect();
        let row = row_of(&runs, &cursor, side)
            .map(|(run, _)| {
                let start = run.glyphs.iter().map(|g| g.start).min().unwrap_or(0);
                let end = run.glyphs.iter().map(|g| g.end).max().unwrap_or(0);
                start..end
            })
            .unwrap_or(0..shaped.line.text.len());
        shaped.line.to_source(row.start, Affinity::Before)
            ..shaped.line.to_source(row.end, Affinity::After)
    }

    /// Scrolls the least that shows the caret at `offset`.
    pub fn reveal(&mut self, source: &Source, offset: usize, side: Affinity) {
        let index = source.doc.line_at(offset);
        let (_, row_top, row_height) = self.caret_in_line(source, offset, side);
        let Some(top) = self.top_of(source, index) else {
            // Far away: its row at the edge it comes in from, as if scrolled
            // there (CodeMirror's "nearest"), so the end of the document
            // ends at the bottom.
            let below = index > self.anchor;
            self.anchor = index;
            self.offset = 0.0;
            let dy = if below {
                row_top + row_height - self.height
            } else {
                row_top
            };
            self.scroll_by(source, dy);
            return;
        };
        let (caret_top, caret_bottom) = (top + row_top, top + row_top + row_height);
        if caret_top < 0.0 {
            self.scroll_by(source, caret_top);
        } else if caret_bottom > self.height {
            self.scroll_by(source, caret_bottom - self.height);
        }
    }
}

/// The row a cursor is drawn on, and its x there. Where a row breaks with
/// no space (after a hyphen, in a URL, in CJK text) the offset ends one row
/// and starts the next: `After` takes the later.
fn row_of<'a, 'b>(
    runs: &'a [cosmic_text::LayoutRun<'b>],
    cursor: &cosmic_text::Cursor,
    side: Affinity,
) -> Option<(&'a cosmic_text::LayoutRun<'b>, f32)> {
    let mut rows = runs
        .iter()
        .filter_map(|run| run.cursor_position(cursor).map(|x| (run, x)));
    let found = match side {
        Affinity::Before => rows.next(),
        Affinity::After => rows.next_back(),
    };
    // A wrap inside a run of spaces leaves the offsets between them on no
    // row: the end of the row they trail.
    found.or_else(|| {
        runs.iter()
            .rev()
            .find(|run| run.glyphs.first().is_some_and(|g| g.start <= cursor.index))
            .map(|run| (run, run.line_w))
    })
}

/// A cosmic-text cursor at display offset `display`, on the given side of a
/// soft wrap.
fn side_cursor(display: usize, side: Affinity) -> cosmic_text::Cursor {
    let affinity = match side {
        Affinity::Before => cosmic_text::Affinity::Before,
        Affinity::After => cosmic_text::Affinity::After,
    };
    cosmic_text::Cursor::new_with_affinity(0, display, affinity)
}
