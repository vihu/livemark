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

/// For a line of only spaces or tabs under a list item's lines (as
/// Shift+Enter leaves one), where the item's text starts, so the caret
/// there stands where typed text will go.
fn blank_in_item(source: &Source, index: usize) -> Option<usize> {
    let text = source.doc.text();
    // Spaces after any quote markers (`>       ` in a callout).
    let blank = |line: &str| line.trim_matches([' ', '\t', '>']).is_empty();
    let spaced = |line: &str| line.ends_with([' ', '\t']);
    let own = &text[source.doc.line_range(index)];
    if !blank(own) || !spaced(own) {
        return None;
    }
    let mut above = index;
    while above > 0 {
        above -= 1;
        let range = source.doc.line_range(above);
        let line = &text[range.clone()];
        if let Some(at) = source.styled.continuation_at(range.clone()) {
            return Some(at);
        }
        if !blank(line) {
            // An item's first line: its text.
            let content = line.trim_start_matches([' ', '\t', '>']);
            let digits = content.bytes().take_while(u8::is_ascii_digit).count();
            let marker = if content.starts_with(['-', '+', '*']) {
                1
            } else if digits > 0 && content[digits..].starts_with(['.', ')']) {
                digits + 1
            } else {
                return None;
            };
            return content[marker..]
                .starts_with([' ', '\t'])
                .then(|| source.styled.hang_at(range))
                .flatten();
        }
        if !spaced(line) {
            return None;
        }
    }
    None
}

/// Where the text of the ATX heading at `line` starts when its `#` run
/// shows (not hidden), for that run to hang left of it.
fn revealed_heading(source: &Source, line: Range<usize>) -> Option<usize> {
    let text = &source.doc.text()[line.clone()];
    let indent = text.bytes().take(4).take_while(|&b| b == b' ').count();
    let hashes = text[indent..].bytes().take_while(|&b| b == b'#').count();
    let rest = &text[indent + hashes..];
    let space = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    if indent > 3
        || !(1..=6).contains(&hashes)
        || space == 0
        || super::shape::heading_level(source.styled, line.clone()) == 0
    {
        return None;
    }
    let open = line.start + indent;
    let i = source.hidden.partition_point(|h| h.end <= open);
    let hidden = source.hidden.get(i).is_some_and(|h| h.start <= open);
    (!hidden).then_some(open + hashes + space)
}

/// A source line shaped for drawing.
pub struct Shaped {
    pub buffer: Arc<cosmic_text::Buffer>,
    /// The projection of this line: its source offsets, so never cached
    /// (equal lines elsewhere share the buffer, not the offsets).
    pub line: Line,
    pub height: f32,
    /// How far rows after the first are drawn right: the width of a list
    /// marker or quote prefix (REFERENCE-001 sections 6, 7), when
    /// `hanging`.
    pub hang: f32,
    /// Whether rows after the first are drawn at `hang` rather than at
    /// `lead`.
    pub hanging: bool,
    /// How far all its rows are drawn right: a lazy line in a quote lines
    /// up with the quoted text before it (section 6).
    pub lead: f32,
    /// Where the first row ends.
    pub first_row: f32,
}

impl Shaped {
    /// How far the row starting `line_top` down is drawn to the right.
    pub fn shift(&self, line_top: f32) -> f32 {
        if line_top > 0.0 && self.hanging {
            self.hang
        } else {
            self.lead
        }
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
    /// Whether a layout has given the real size (until then 600 by 400).
    pub sized: bool,
    /// A caret to bring into view once sized.
    pub pending_reveal: Option<(usize, Affinity)>,
    /// Source mode: one size, the code font throughout (REFERENCE-001
    /// section 17).
    pub source: bool,
    /// Every text size times this (`Editor::set_zoom`).
    pub zoom: f32,
    /// How far left of the text a revealed heading's `#` run may hang:
    /// the gutter and the padding.
    pub room: f32,
    highlights: Highlights,
    cache: HashMap<u64, Cached>,
    /// Tables as grids, by table text and styling, width and colors, and
    /// the keys used since the last frame ended (what `trim` keeps).
    grids: HashMap<u64, Arc<Grid>>,
    grids_used: Vec<u64>,
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
            sized: false,
            pending_reveal: None,
            source: false,
            zoom: 1.0,
            room: 0.0,
            highlights: Highlights::default(),
            grids: HashMap::new(),
            grids_used: Vec::new(),
            cache: HashMap::new(),
        }
    }

    /// Line `index` shaped, from the cache when its text, styling, width
    /// and colors are unchanged.
    pub fn shaped(&mut self, source: &Source, index: usize) -> Shaped {
        let range = source.doc.line_range(index);
        // A lazy line in a quote lines up with the quoted line's text, and
        // a later line of a list item with the item's text (not in source
        // mode, which shows the markdown as written), its own indentation
        // taken off.
        let lazy = source
            .styled
            .lazy_at(range.clone())
            .map(|at| (at, false))
            .or_else(|| {
                let styled = source.styled;
                let item = styled.continuation_at(range.clone());
                item.or_else(|| blank_in_item(source, index))
                    .map(|at| (at, true))
            })
            .filter(|_| !self.source);
        // A heading whose `#` run shows: the run hangs left of the text,
        // into the gutter, so the heading text stays where it is with the
        // run hidden (REFERENCE-001 section 3), as far as the room allows.
        if let Some(text_at) = revealed_heading(source, range.clone()).filter(|_| !self.source) {
            let plain = self.shape_line(source, index, (0.0, None));
            let run = super::marks::start_x(&plain, text_at);
            let hang = run.min(self.room);
            return self.shape_line(source, index, (-hang, Some(run - hang)));
        }
        let Some((anchor, item)) = lazy else {
            return self.shape_line(source, index, (0.0, None));
        };
        let quoted = self.shaped(source, source.doc.line_at(anchor));
        let target = super::marks::start_x(&quoted, anchor);
        let text = &source.doc.text()[range.clone()];
        // A list item's line in a quote: past the quote's markers too.
        let prefix: &[char] = if item {
            &[' ', '\t', '>']
        } else {
            &[' ', '\t']
        };
        let indent = text.len() - text.trim_start_matches(prefix).len();
        let own = if indent == 0 {
            0.0
        } else {
            let plain = self.shape_line(source, index, (0.0, None));
            super::marks::start_x(&plain, range.start + indent)
        };
        // Its first row past its own indentation, the rest at the quoted
        // text or the item's. An item's line indented further (written for
        // `- [ ] ` as source, wider than the drawn checkbox) shifts left as
        // far as the gutter's room, where its start and a caret there still
        // show; a lazy quote line keeps its place.
        let lead = if item {
            (target - own).max(-self.room)
        } else {
            (target - own).max(0.0)
        };
        self.shape_line(source, index, (lead, Some(target.max(own + lead))))
    }

    /// Line `index` shaped with its first row `lead` to the right and,
    /// when `rest` is not 0, its other rows `rest` to the right.
    fn shape_line(
        &mut self,
        source: &Source,
        index: usize,
        (lead, rest): (f32, Option<f32>),
    ) -> Shaped {
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
            self.zoom.to_bits(),
            lead.to_bits(),
            rest.map(f32::to_bits),
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
                let looks = (level, mono, self.colors, compact, self.zoom);
                let narrower = lead.max(rest.unwrap_or(0.0));
                let cached = shape(&line, looks, hang, self.width - narrower, &tokens);
                self.cache.insert(key, cached.clone());
                cached
            }
        };
        Shaped {
            buffer: cached.buffer,
            line,
            height: cached.height,
            hang: rest.unwrap_or(cached.hang),
            hanging: rest.is_some() || cached.hang > 0.0,
            lead,
            first_row: cached.first_row,
        }
    }

    /// Drops shaped lines not drawn lately and grids not used this frame,
    /// once their caches are large.
    pub fn trim(&mut self, keep: &[Shaped]) {
        if self.cache.len() > 4 * keep.len() + 256 {
            self.cache
                .retain(|_, cached| keep.iter().any(|k| Arc::ptr_eq(&k.buffer, &cached.buffer)));
        }
        // Grids go only between frames: a frame's text holds its buffers
        // weakly until it is rendered.
        if self.grids.len() > 32 {
            let used = std::mem::take(&mut self.grids_used);
            self.grids.retain(|key, _| used.contains(key));
        }
        self.grids_used.clear();
    }

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
        // Its styling too, relative to its start: a link reference
        // definition elsewhere changes how its cells look.
        let start = table.range.start;
        let runs = source.styled.runs();
        let first = runs.partition_point(|(r, _)| r.end <= start);
        for (r, style) in runs[first..]
            .iter()
            .take_while(|(r, _)| r.start < table.range.end)
        {
            (r.start.saturating_sub(start), r.end - start, style).hash(&mut hasher);
        }
        for m in &table.markers {
            (m.start - start, m.end - start).hash(&mut hasher);
        }
        // The room right of where it starts (a list item's or quote's
        // indent), not the whole width.
        let width = (self.width - self.grid_x(source, index)).max(0.0);
        width.to_bits().hash(&mut hasher);
        for color in [
            self.colors.text,
            self.colors.marker,
            self.colors.code,
            self.colors.link,
        ] {
            color.into_rgba8().hash(&mut hasher);
        }
        self.zoom.to_bits().hash(&mut hasher);
        let key = hasher.finish();
        self.grids_used.push(key);
        if let Some(grid) = self.grids.get(&key) {
            return grid.clone();
        }
        let grid = Arc::new(Grid::new(
            source.doc.text(),
            source.styled,
            table,
            (self.colors, self.zoom),
            width,
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
        let (table, row) = match mark.kind {
            MarkKind::TableRow(table, row) => (table, row),
            MarkKind::TableRule(table) => (table, 0),
            _ => return None,
        };
        let start = self.grid_x(source, table);
        let grid = self.grid(source, table);
        Some(grid.hit(&source.styled.tables()[table], row, x - start))
    }

    /// Where table `table`'s grid starts, from the text area's left edge:
    /// at its header row's first pipe, for every row (rows after the first
    /// can have other prefixes, and the delimiter row's line is thin).
    pub fn grid_x(&mut self, source: &Source, table: usize) -> f32 {
        let header = source.styled.tables()[table].rows[0].0.start;
        let shaped = self.shaped(source, source.doc.line_at(header));
        super::marks::start_x(&shaped, header)
    }

    /// Whether `x`, `y` in the text area is on where source `range` is
    /// drawn (in a table drawn as a grid: on the text of the cell there).
    pub fn covers(&mut self, source: &Source, range: Range<usize>, x: f32, y: f32) -> bool {
        let (index, top) = self.line_at_y(source, y);
        let line = source.doc.line_range(index);
        let row = marks_in(source.concealed, line.clone()).find_map(|m| match m.kind {
            MarkKind::TableRow(table, row) => Some((table, row)),
            _ => None,
        });
        if let Some((table, row)) = row {
            let start = self.grid_x(source, table);
            return self.grid(source, table).covers(row, x - start);
        }
        let (start, end) = (range.start.max(line.start), range.end.min(line.end));
        if start > end {
            return false;
        }
        let shaped = self.shaped(source, index);
        let display = shaped.line.to_display(start)..shaped.line.to_display(end);
        let point = iced::Point::new(x, y - top);
        shaped
            .stretches(display)
            .iter()
            .any(|(rect, _)| rect.contains(point))
    }

    /// Where the text of the item or quote starts when `x`, `y` in the text
    /// area is on its concealed bullet or quote marker: after the marker,
    /// any task box and nested `>` (where its wrapped rows hang).
    pub fn text_after_mark(&mut self, source: &Source, x: f32, y: f32) -> Option<usize> {
        let (index, top) = self.line_at_y(source, y);
        let range = source.doc.line_range(index);
        let shaped = self.shaped(source, index);
        let point = iced::Point::new(x, y - top);
        let on = super::marks::on_bullet_or_quote(
            &shaped,
            marks_in(source.concealed, range.clone()),
            point,
        );
        on.then(|| source.styled.hang_at(range)).flatten()
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
