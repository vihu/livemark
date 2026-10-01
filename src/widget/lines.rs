//! Lines as drawn: each source line projected (`layout::Line`) and shaped
//! as one cosmic-text buffer through iced's font system, a scroll position
//! kept as an anchor line, and points mapped to source offsets and back.
//! Only lines that are walked over or drawn are shaped.
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use iced::Font;
use iced::advanced::graphics::text::{cosmic_text, font_system, to_attributes, to_color};

use crate::doc::Doc;
use crate::layout::{Affinity, Line};
use crate::style::{Style, Styled};

/// Body text size in pixels.
pub const TEXT_SIZE: f32 = 16.0;

/// Line height as a multiple of the text size.
const LINE_HEIGHT: f32 = 1.5;

/// Heading sizes in em by level (Keeprs' web editor: 1.4, 1.25, 1.1).
const HEADING: [f32; 6] = [1.4, 1.25, 1.1, 1.0, 1.0, 1.0];

/// Colors the lines are shaped with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    pub text: iced::Color,
    pub marker: iced::Color,
    pub code: iced::Color,
}

/// A source line shaped for drawing.
pub struct Shaped {
    pub buffer: Arc<cosmic_text::Buffer>,
    /// The projection of this line: its source offsets, so never cached
    /// (equal lines elsewhere share the buffer, not the offsets).
    pub line: Line,
    pub height: f32,
}

/// A shaped buffer, the same for every line with the same text and style.
#[derive(Clone)]
struct Cached {
    buffer: Arc<cosmic_text::Buffer>,
    height: f32,
}

/// What the lines are drawn from, borrowed for one call.
pub struct Source<'a> {
    pub doc: &'a Doc,
    pub styled: &'a Styled,
    pub hidden: &'a [Range<usize>],
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
    cache: HashMap<u64, Cached>,
}

impl Lines {
    pub fn new(colors: Colors) -> Self {
        Self {
            width: 600.0,
            height: 400.0,
            anchor: 0,
            offset: 0.0,
            colors,
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
        let level = heading_level(source.styled, range);
        let mut hasher = DefaultHasher::new();
        (&line.text, &line.runs, level, self.width.to_bits()).hash(&mut hasher);
        for color in [self.colors.text, self.colors.marker, self.colors.code] {
            color.into_rgba8().hash(&mut hasher);
        }
        let key = hasher.finish();
        let cached = match self.cache.get(&key) {
            Some(cached) => cached.clone(),
            None => {
                let cached = shape(&line, level, self.width, self.colors);
                self.cache.insert(key, cached.clone());
                cached
            }
        };
        Shaped {
            buffer: cached.buffer,
            line,
            height: cached.height,
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
        match shaped.buffer.hit(x, y - top) {
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
                return (x, run.line_top, run.line_height);
            }
        }
        runs.last().map_or((0.0, 0.0, shaped.height), |run| {
            (run.line_w, run.line_top, run.line_height)
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

/// A cosmic-text cursor at display offset `display` of a line's buffer.
pub fn cursor(display: usize) -> cosmic_text::Cursor {
    cosmic_text::Cursor::new(0, display)
}

/// The heading level of the line at `range`, or 0.
fn heading_level(styled: &Styled, range: Range<usize>) -> u8 {
    let runs = styled.runs();
    let first = runs.partition_point(|(r, _)| r.end <= range.start);
    runs[first..]
        .iter()
        .take_while(|(r, _)| r.start < range.end)
        .find(|(_, style)| style.heading > 0)
        .map_or(0, |(_, style)| style.heading)
}

fn shape(line: &Line, level: u8, width: f32, colors: Colors) -> Cached {
    let scale = if level == 0 {
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
    let plain = attrs(Style::default(), colors);
    let mut spans = Vec::new();
    let mut at = 0;
    for (range, style) in &line.runs {
        if at < range.start {
            spans.push((&line.text[at..range.start], plain.clone()));
        }
        spans.push((&line.text[range.clone()], attrs(*style, colors)));
        at = range.end;
    }
    if at < line.text.len() {
        spans.push((&line.text[at..], plain.clone()));
    }
    buffer.set_rich_text(spans, &plain, cosmic_text::Shaping::Advanced, None);
    buffer.shape_until_scroll(raw, false);
    let height = buffer
        .layout_runs()
        .map(|run| run.line_top + run.line_height)
        .fold(metrics.line_height, f32::max);
    Cached {
        buffer: Arc::new(buffer),
        height,
    }
}

fn attrs(style: Style, colors: Colors) -> cosmic_text::Attrs<'static> {
    let font = if style.code {
        Font::MONOSPACE
    } else {
        Font::DEFAULT
    };
    let mut attrs = to_attributes(font);
    if style.strong || style.heading > 0 {
        attrs = attrs.weight(cosmic_text::Weight::BOLD);
    }
    if style.emphasis {
        attrs = attrs.style(cosmic_text::Style::Italic);
    }
    let color = if style.marker {
        colors.marker
    } else if style.code {
        colors.code
    } else {
        colors.text
    };
    attrs.color(to_color(color))
}
