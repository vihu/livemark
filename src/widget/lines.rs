//! Lines as drawn: each source line projected (`layout::Line`) and shaped
//! as one cosmic-text buffer through iced's font system, a scroll position
//! kept as an anchor line, and points mapped to source offsets and back.
//! Only lines that are walked over or drawn are shaped.
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use iced::Theme;
use iced::advanced::graphics::text::cosmic_text;

use super::highlight::{Highlights, Token};
use super::shape::{Cached, Colors, cursor, heading_level, shape};
use super::table::Grid;
use crate::doc::Doc;
use crate::layout::{Affinity, Line};
use crate::style::{Mark, MarkKind, Styled};

/// A source line shaped for drawing.
pub struct Shaped {
    pub buffer: Arc<cosmic_text::Buffer>,
    /// The projection of this line: its source offsets, so never cached
    /// (equal lines elsewhere share the buffer, not the offsets).
    pub line: Line,
    pub height: f32,
    /// How far rows after the first are drawn right of it: the width of a
    /// list marker or quote prefix (REFERENCE-001 sections 6, 7).
    pub hang: f32,
    /// Where the first row ends.
    pub first_row: f32,
}

impl Shaped {
    /// How far the row starting `line_top` down is drawn to the right.
    pub fn shift(&self, line_top: f32) -> f32 {
        if line_top > 0.0 { self.hang } else { 0.0 }
    }

    /// The rectangles display `range` covers, row by row, in the line's
    /// own coordinates, each with its row's baseline.
    pub fn stretches(&self, range: Range<usize>) -> Vec<(iced::Rectangle, f32)> {
        let (start, end) = (cursor(range.start), cursor(range.end));
        let mut stretches = Vec::new();
        for run in self.buffer.layout_runs() {
            for (x, width) in run.highlight(start, end) {
                let at = iced::Point::new(x + self.shift(run.line_top), run.line_top);
                let size = iced::Size::new(width, run.line_height);
                stretches.push((iced::Rectangle::new(at, size), run.line_y));
            }
        }
        stretches
    }
}

/// What the lines are drawn from, borrowed for one call.
pub struct Source<'a> {
    pub doc: &'a Doc,
    pub styled: &'a Styled,
    pub hidden: &'a [Range<usize>],
    /// Marks drawn as something else (`draw.rs`), sorted.
    pub concealed: &'a [Mark],
}

/// Shaped lines and the scroll position.
pub struct Lines {
    /// Width text wraps at.
    pub width: f32,
    /// Height of the visible area.
    pub height: f32,
    /// The first line drawn.
    pub anchor: usize,
    /// How far the anchor line starts above the top, in pixels.
    pub offset: f32,
    pub colors: Colors,
    /// The theme code is highlighted with; none before the first draw.
    pub theme: Option<Theme>,
    /// How many lines the last frame drew, for the scroll bar.
    pub visible: usize,
    /// Source mode: one size, the code font throughout (REFERENCE-001
    /// section 17).
    pub source: bool,
    highlights: Highlights,
    cache: HashMap<u64, Cached>,
    /// Tables as grids, by table text, width and colors.
    grids: HashMap<u64, Arc<Grid>>,
}

impl Lines {
    pub fn new(colors: Colors) -> Self {
        Self {
            width: 600.0,
            height: 400.0,
            anchor: 0,
            offset: 0.0,
            colors,
            theme: None,
            visible: 0,
            source: false,
            highlights: Highlights::default(),
            grids: HashMap::new(),
            cache: HashMap::new(),
        }
    }

    /// Line `index` shaped, from the cache when its text, styling, width
    /// and colors are unchanged.
    pub fn shaped(&mut self, source: &Source, index: usize) -> Shaped {
        let range = source.doc.line_range(index);
        let line = Line::new(
            source.doc.text(),
            range.clone(),
            source.hidden,
            source.styled.runs(),
        );
        let level = if self.source {
            0
        } else {
            heading_level(source.styled, range.clone())
        };
        let hang = source
            .styled
            .hang_at(range.clone())
            .map(|at| line.to_display(at));
        let mut tokens = match &self.theme {
            Some(theme) => self.highlights.tokens(
                source.doc.text(),
                source.styled,
                range.clone(),
                &line,
                theme,
            ),
            None => Vec::new(),
        };
        // Concealed marks keep their place but are not drawn.
        tokens.extend(marks_in(source.concealed, range.clone()).map(|mark| Token {
            range: line.to_display(mark.range.start)..line.to_display(mark.range.end),
            color: iced::Color::TRANSPARENT,
            italic: false,
        }));
        let mut hasher = DefaultHasher::new();
        let mono = self.source;
        let compact = marks_in(source.concealed, range.clone())
            .any(|m| matches!(m.kind, MarkKind::TableRule(_)));
        compact.hash(&mut hasher);
        (
            &line.text,
            &line.runs,
            level,
            hang,
            mono,
            self.width.to_bits(),
        )
            .hash(&mut hasher);
        for color in [
            self.colors.text,
            self.colors.marker,
            self.colors.code,
            self.colors.link,
        ] {
            color.into_rgba8().hash(&mut hasher);
        }
        for token in &tokens {
            (&token.range, token.color.into_rgba8(), token.italic).hash(&mut hasher);
        }
        let key = hasher.finish();
        let cached = match self.cache.get(&key) {
            Some(cached) => cached.clone(),
            None => {
                let looks = (level, mono, self.colors, compact);
                let cached = shape(&line, looks, hang, self.width, &tokens);
                self.cache.insert(key, cached.clone());
                cached
            }
        };
        Shaped {
            buffer: cached.buffer,
            line,
            height: cached.height,
            hang: cached.hang,
            first_row: cached.first_row,
        }
    }

    /// Drops shaped lines not drawn lately, once the cache is large.
    pub fn trim(&mut self, keep: &[Shaped]) {
        if self.cache.len() > 4 * keep.len() + 256 {
            self.cache
                .retain(|_, cached| keep.iter().any(|k| Arc::ptr_eq(&k.buffer, &cached.buffer)));
        }
    }

    /// Scrolls by `dy` pixels (positive: further down the document),
    /// stopping with the first line at the top or the last line at the top.
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
        if self.anchor >= last {
            self.anchor = last;
            self.offset = 0.0;
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
        let x = if y >= shaped.first_row {
            x - shaped.hang
        } else {
            x
        };
        match shaped.buffer.hit(x, y) {
            Some(cursor) => {
                let affinity = match cursor.affinity {
                    cosmic_text::Affinity::Before => Affinity::Before,
                    cosmic_text::Affinity::After => Affinity::After,
                };
                (shaped.line.to_source(cursor.index, affinity), affinity)
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
        for run in &runs {
            if let Some(x) = run.cursor_position(&cursor) {
                return (
                    x + shaped.shift(run.line_top),
                    run.line_top,
                    run.line_height,
                );
            }
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
        let row = shaped
            .buffer
            .layout_runs()
            .find(|run| run.cursor_position(&cursor).is_some())
            .map(|run| {
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
            // Far away: put its line at the top, then centre-ish below.
            self.anchor = index;
            self.offset = 0.0;
            self.scroll_by(source, row_top + row_height - self.height / 2.0);
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

/// A cosmic-text cursor at display offset `display`, on the given side of a
/// soft wrap.
fn side_cursor(display: usize, side: Affinity) -> cosmic_text::Cursor {
    let affinity = match side {
        Affinity::Before => cosmic_text::Affinity::Before,
        Affinity::After => cosmic_text::Affinity::After,
    };
    cosmic_text::Cursor::new_with_affinity(0, display, affinity)
}

/// The marks of the sorted `marks` on the line at `range`.
pub fn marks_in(marks: &[Mark], range: Range<usize>) -> impl Iterator<Item = &Mark> {
    let first = marks.partition_point(|m| m.range.end < range.start);
    marks[first..]
        .iter()
        .take_while(move |m| m.range.start <= range.end)
        .filter(move |m| range.start <= m.range.start && m.range.end <= range.end)
}

impl Lines {
    /// Table `index` of `source` as a grid.
    pub fn grid(&mut self, source: &Source, index: usize) -> Arc<Grid> {
        let table = &source.styled.tables()[index];
        let mut hasher = DefaultHasher::new();
        source.doc.text()[table.range.clone()].hash(&mut hasher);
        self.width.to_bits().hash(&mut hasher);
        for color in [
            self.colors.text,
            self.colors.marker,
            self.colors.code,
            self.colors.link,
        ] {
            color.into_rgba8().hash(&mut hasher);
        }
        let key = hasher.finish();
        if let Some(grid) = self.grids.get(&key) {
            return grid.clone();
        }
        if self.grids.len() >= 32 {
            self.grids.clear();
        }
        let grid = Arc::new(Grid::new(
            source.doc.text(),
            source.styled,
            table,
            self.colors,
            self.width,
        ));
        self.grids.insert(key, grid.clone());
        grid
    }

    /// The source offset under `x`, `y` in the text area when that is a
    /// table drawn as a grid: in the cell there (a click on the rule
    /// under the header goes to the header's first cell).
    pub fn table_hit(&mut self, source: &Source, x: f32, y: f32) -> Option<usize> {
        let (index, _) = self.line_at_y(source, y);
        let range = source.doc.line_range(index);
        let mark = marks_in(source.concealed, range)
            .find(|m| matches!(m.kind, MarkKind::TableRow(..) | MarkKind::TableRule(_)))?
            .clone();
        let shaped = self.shaped(source, index);
        let start = super::marks::start_x(&shaped, mark.range.start);
        let (table, row) = match mark.kind {
            MarkKind::TableRow(table, row) => (table, row),
            MarkKind::TableRule(table) => (table, 0),
            _ => return None,
        };
        Some(self.grid(source, table).hit(row, x - start))
    }

    /// The concealed task box whose checkbox is at `x`, `y` in the text
    /// area, if any.
    pub fn task_at(&mut self, source: &Source, x: f32, y: f32) -> Option<Range<usize>> {
        let (index, top) = self.line_at_y(source, y);
        let shaped = self.shaped(source, index);
        let range = source.doc.line_range(index);
        let point = iced::Point::new(x, y - top);
        super::marks::task_at(&shaped, marks_in(source.concealed, range), point)
    }
}
