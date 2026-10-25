//! Lines as drawn: each source line projected (`layout::Line`) and shaped
//! as one cosmic-text buffer through iced's font system, and a scroll
//! position kept as an anchor line. Only lines that are walked over or
//! drawn are shaped. Scrolling and points mapped to source offsets and back
//! are in `scroll.rs`; grids in `table.rs`; concealed marks in `marks.rs`.
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use iced::advanced::graphics::text::cosmic_text;
use iced::widget::image::Handle;
use iced::{Rectangle, Theme};

use super::highlight::{Highlights, Token};
use super::marks::marks_in;
use super::picture::{self, Picture};
use super::properties::{Block, folded};
use super::shape::{Cached, Colors, cursor, heading_level, shape};
use super::table::{Grid, table_row};
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
    /// Pictures of the line's images, under its text (REFERENCE-001
    /// section 5), in the line's coordinates.
    pub pictures: Vec<(Handle, Rectangle)>,
}

impl Shaped {
    /// Where the line's text rows end, its pictures below.
    pub fn text_bottom(&self) -> f32 {
        self.buffer
            .layout_runs()
            .last()
            .map_or(self.height, |run| run.line_top + run.line_height)
    }

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
    /// Pictures for images, by destination (`Editor::set_image`).
    pub pictures: &'a HashMap<String, Picture>,
    /// Links the host lists under the last line (`Editor::set_footer`).
    pub footer: Option<&'a super::footer::Footer>,
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
    /// The text at most this many code columns wide, centred
    /// (`Editor::set_line_width`).
    pub line_width: Option<u16>,
    /// How far left of the text a revealed heading's `#` run may hang:
    /// the gutter and the padding.
    pub room: f32,
    /// The whole widget's size, the text's width and room following from
    /// it and the zoom (`fit`).
    pub outer: iced::Size,
    highlights: Highlights,
    cache: HashMap<u64, Cached>,
    /// Tables as grids, by table text and styling, width and colors, and
    /// the keys used since the last frame ended (what `trim` keeps).
    pub(super) grids: HashMap<u64, Arc<Grid>>,
    pub(super) grids_used: Vec<u64>,
    /// The front matter as properties, by its text, the inline tags,
    /// width, colors and zoom (`properties.rs`).
    pub(super) block: Option<(u64, Arc<Block>)>,
    /// The host's footer laid out, by its links, width, colors and zoom
    /// (`footer.rs`).
    pub(super) laid_footer: Option<(u64, Arc<super::footer::Laid>)>,
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
            line_width: None,
            room: 0.0,
            outer: iced::Size::ZERO,
            highlights: Highlights::default(),
            grids: HashMap::new(),
            grids_used: Vec::new(),
            block: None,
            laid_footer: None,
            cache: HashMap::new(),
        }
    }

    /// Line `index` shaped, from the cache when its text, styling, width
    /// and colors are unchanged.
    pub fn shaped(&mut self, source: &Source, index: usize) -> Shaped {
        let mut shaped = self.shaped_text(source, index);
        if !self.source {
            // A table's row drawn as a grid row: as tall as its tallest
            // cell, not its source.
            if let Some((table, row)) = table_row(source, index) {
                shaped.height = self.grid(source, table).heights[row];
            }
            self.place_pictures(source, index, &mut shaped);
            // Concealed front matter: its first line as the properties, the
            // rest folded away.
            if let Some(front) = folded(source)
                && source.doc.line_range(index).start <= front.end
            {
                shaped.height = if index == 0 {
                    self.block(source).height
                } else {
                    0.0
                };
            }
            // The host's footer under the last line.
            if index + 1 == source.doc.line_count()
                && let Some(footer) = self.footer(source)
            {
                shaped.height += footer.height;
            }
        }
        shaped
    }

    /// The text's width, height and gutter room for the widget's size at
    /// the zoom.
    pub fn fit(&mut self) {
        let bounds = Rectangle::new(iced::Point::ORIGIN, self.outer);
        let area = super::surface::text_area(bounds, self.zoom, self.line_width);
        self.width = area.width;
        self.height = area.height;
        self.room = area.x - super::surface::EDGE;
    }

    /// The pictures of the images on line `index` under its text, the
    /// line made taller by them; a line with no text left but them (its
    /// image markdown hidden) collapses to them.
    fn place_pictures(&mut self, source: &Source, index: usize, shaped: &mut Shaped) {
        let range = source.doc.line_range(index);
        let images = source.styled.images();
        let first = images.partition_point(|(r, _)| r.start < range.start);
        // Shown while the markdown shows too: touching the image only
        // brings its source back above it.
        let shown: Vec<_> = images[first..]
            .iter()
            .take_while(|(r, _)| r.start < range.end)
            .filter_map(|(_, url)| source.pictures.get(url))
            .collect();
        if shown.is_empty() {
            return;
        }
        let top = if shaped.line.text.trim().is_empty() {
            -picture::GAP
        } else {
            shaped.height
        };
        // Under the line's text: after a list marker or quote prefix.
        let left = source
            .styled
            .hang_at(range.clone())
            .map(|at| super::marks::start_x(shaped, at))
            .unwrap_or(shaped.lead)
            .clamp(0.0, self.width / 2.0);
        let (rects, bottom) = picture::stack(
            &shown,
            iced::Point::new(left, top),
            self.width - left,
            picture::MAX_HEIGHT * self.zoom,
        );
        shaped.height = bottom;
        shaped.pictures = shown
            .iter()
            .zip(rects)
            .map(|(picture, rect)| (picture.handle.clone(), rect))
            .collect();
    }

    pub fn shaped_text(&mut self, source: &Source, index: usize) -> Shaped {
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
            // From the run's own glyphs: when the first word does not fit
            // after it, the text starts on the next row.
            let end = plain.line.to_display(text_at);
            let run = plain.buffer.layout_runs().next().map_or(0.0, |row| {
                let glyphs = row.glyphs.iter().filter(|g| g.start < end);
                glyphs.map(|g| g.x + g.w).fold(0.0, f32::max)
            });
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
            pictures: Vec::new(),
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
}
