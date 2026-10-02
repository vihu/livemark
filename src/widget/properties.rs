//! The front matter drawn as properties while the selection is outside it
//! (PLAN-005): the `title` large (unless the note opens with its own `#`
//! heading), then a line of its `created` date and its tags as chips (a tag
//! written in the text outlined, "in text"), each with a cross, and
//! "+ tag". The block takes the place of the front matter's first line,
//! whose other lines fold away. A cross takes its tag out, "+ tag" puts the
//! caret in the list; a click anywhere else puts it in the YAML, which then
//! shows, as every concealed construct does.
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use iced::advanced::graphics::text::Renderer as _;
use iced::advanced::graphics::text::{Raw, cosmic_text, font_system, to_attributes, to_color};
use iced::advanced::renderer::{self, Renderer as _};
use iced::{Border, Color, Font, Point, Rectangle, Size, Vector};

use super::lines::{Lines, Source};
use super::shape::{Colors, TEXT_SIZE, heading_level};
use crate::edit::properties::Chip;
use crate::fonts;
use crate::parse::properties::unquoted;
use crate::style::{MarkKind, Styled};

/// A chip's height, the space between chips, and the room under the block,
/// at 100%.
const CHIP: f32 = 24.0;
const GAP: f32 = 6.0;
const BELOW: f32 = 14.0;

/// What a click on the block does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A chip's cross.
    Remove(Chip),
    /// "+ tag".
    Add,
}

/// Text shaped on its own, placed in the block.
pub(super) struct Label {
    pub(super) buffer: Arc<cosmic_text::Buffer>,
    pub(super) at: Point,
    pub(super) size: Size,
}

/// A tag's chip.
struct ChipBox {
    chip: Chip,
    bounds: Rectangle,
    cross: Rectangle,
    labels: Vec<Label>,
    inline: bool,
}

/// The properties laid out to a width.
pub struct Block {
    labels: Vec<Label>,
    chips: Vec<ChipBox>,
    add: Rectangle,
    /// How tall the block is, the room under it included.
    pub height: f32,
}

/// `text` shaped in the prose font at `size`, wrapped at `width`.
pub(super) fn label(
    text: &str,
    size: f32,
    weight: cosmic_text::Weight,
    color: Color,
    width: f32,
) -> Label {
    let metrics = cosmic_text::Metrics::new(size, (size * 1.35).round());
    let mut system = font_system().write().expect("font system lock");
    let raw = system.raw();
    let mut buffer = cosmic_text::Buffer::new(raw, metrics);
    buffer.set_size(Some(width.max(1.0)), None);
    buffer.set_wrap(cosmic_text::Wrap::WordOrGlyph);
    let attrs = to_attributes(Font::new(fonts::PROSE))
        .weight(weight)
        .color(to_color(color));
    buffer.set_text(text, &attrs, cosmic_text::Shaping::Advanced, None);
    buffer.shape_until_scroll(raw, false);
    let runs = || buffer.layout_runs();
    let size = Size::new(
        runs().map(|run| run.line_w).fold(0.0, f32::max),
        runs()
            .map(|run| run.line_top + run.line_height)
            .fold(metrics.line_height, f32::max),
    );
    Label {
        buffer: Arc::new(buffer),
        at: Point::ORIGIN,
        size,
    }
}

impl Block {
    /// The properties of `text`'s front matter laid out to fit `width`.
    pub fn new(text: &str, styled: &Styled, (colors, zoom): (Colors, f32), width: f32) -> Self {
        let Some(properties) = styled.properties() else {
            return Self {
                labels: Vec::new(),
                chips: Vec::new(),
                add: Rectangle::default(),
                height: 0.0,
            };
        };
        let (bold, semibold, normal) = (
            cosmic_text::Weight::BOLD,
            cosmic_text::Weight::SEMIBOLD,
            cosmic_text::Weight::NORMAL,
        );
        let small = 13.0 * zoom;
        let mut labels = Vec::new();
        let mut y = 0.0;
        if let Some(title) = properties
            .title
            .clone()
            .filter(|_| !opens_with_title(text, styled))
        {
            let title = label(
                &unquoted(&text[title]),
                TEXT_SIZE * zoom * 1.4,
                bold,
                colors.text,
                width,
            );
            y = title.size.height + 2.0 * zoom;
            labels.push(title);
        }
        // The date, the chips and "+ tag" in rows, wrapping.
        let (chip, gap) = (CHIP * zoom, GAP * zoom);
        let mut x = 0.0;
        let mut place = |w: f32| {
            if x > 0.0 && x + w > width {
                x = 0.0;
                y += chip + gap;
            }
            let at = Point::new(x, y);
            x += w + gap;
            at
        };
        if let Some(created) = properties.created.clone() {
            let mut date = label(
                &long_date(&unquoted(&text[created])),
                small,
                normal,
                colors.marker,
                width,
            );
            let at = place(date.size.width + gap);
            date.at = at + Vector::new(0.0, (chip - date.size.height) / 2.0);
            labels.push(date);
        }
        let listed: Vec<(Chip, String, bool)> = properties
            .tags
            .iter()
            .flat_map(|list| list.items.iter().enumerate())
            .filter(|(_, item)| !item.name.is_empty())
            .map(|(i, item)| (Chip::Listed(i), item.name.clone(), false))
            .collect();
        let mut seen: Vec<String> = listed.iter().map(|(_, n, _)| n.to_lowercase()).collect();
        let mut inline = Vec::new();
        for (_, name) in styled.tags() {
            if !seen.contains(&name.to_lowercase()) {
                seen.push(name.to_lowercase());
                inline.push((Chip::Inline(name.clone()), name.clone(), true));
            }
        }
        let mut chips = Vec::new();
        for (which, name, written) in listed.into_iter().chain(inline) {
            let name = label(&format!("#{name}"), small, semibold, colors.link, width);
            let note = written.then(|| label("in text", 11.0 * zoom, normal, colors.marker, width));
            let cross = label("\u{d7}", small, normal, colors.marker, width);
            let pad = 9.0 * zoom;
            let note_w = note.as_ref().map_or(0.0, |n| n.size.width + 4.0 * zoom);
            let cross_box = 20.0 * zoom;
            let w = pad + name.size.width + note_w + cross_box + 2.0 * zoom;
            let at = place(w);
            let mid = |l: &Label| (chip - l.size.height) / 2.0;
            let mut parts = Vec::new();
            let mut name = name;
            name.at = at + Vector::new(pad, mid(&name));
            let mut after = pad + name.size.width + 4.0 * zoom;
            parts.push(name);
            if let Some(mut note) = note {
                note.at = at + Vector::new(after, mid(&note));
                after += note.size.width;
                parts.push(note);
            }
            let cross_at = at + Vector::new(after, (chip - cross_box) / 2.0);
            let mut cross = cross;
            cross.at = cross_at
                + Vector::new(
                    (cross_box - cross.size.width) / 2.0,
                    (cross_box - cross.size.height) / 2.0,
                );
            parts.push(cross);
            chips.push(ChipBox {
                chip: which,
                bounds: Rectangle::new(at, Size::new(w, chip)),
                cross: Rectangle::new(cross_at, Size::new(cross_box, cross_box)),
                labels: parts,
                inline: written,
            });
        }
        let mut plus = label("+ tag", small, normal, colors.marker, width);
        let add_w = plus.size.width + 12.0 * zoom;
        let at = place(add_w);
        plus.at = at + Vector::new(6.0 * zoom, (chip - plus.size.height) / 2.0);
        labels.push(plus);
        Self {
            labels,
            chips,
            add: Rectangle::new(at, Size::new(add_w, chip)),
            height: y + chip + BELOW * zoom,
        }
    }

    /// What is at `at` (in the block's coordinates): a cross or "+ tag".
    pub fn hit(&self, at: Point) -> Option<Hit> {
        self.chips
            .iter()
            .find(|chip| chip.cross.contains(at))
            .map(|chip| Hit::Remove(chip.chip.clone()))
            .or_else(|| self.add.contains(at).then_some(Hit::Add))
    }

    /// Draws the block with its top left at `origin`, clipped to `clip`;
    /// chips filled with `pill`, those written in the text outlined in it.
    pub fn draw(&self, renderer: &mut iced::Renderer, origin: Point, clip: Rectangle, pill: Color) {
        let offset = Vector::new(origin.x, origin.y);
        for chip in &self.chips {
            let bounds = chip.bounds + offset;
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: Border {
                        color: if chip.inline {
                            Color { a: 0.6, ..pill }
                        } else {
                            Color::TRANSPARENT
                        },
                        width: 1.0,
                        radius: (bounds.height / 2.0).into(),
                    },
                    ..renderer::Quad::default()
                },
                if chip.inline {
                    Color::TRANSPARENT
                } else {
                    Color { a: 0.14, ..pill }
                },
            );
        }
        let labels = self
            .labels
            .iter()
            .chain(self.chips.iter().flat_map(|chip| &chip.labels));
        for label in labels {
            renderer.fill_raw(Raw {
                buffer: Arc::downgrade(&label.buffer),
                position: label.at + offset,
                color: Color::BLACK,
                clip_bounds: clip,
            });
        }
    }
}

/// The front matter's block when it is concealed as properties.
pub fn folded(source: &Source) -> Option<Range<usize>> {
    source
        .concealed
        .first()
        .filter(|m| m.kind == MarkKind::Properties)
        .map(|m| m.touch.clone())
}

impl Lines {
    /// The front matter laid out as properties.
    pub fn block(&mut self, source: &Source) -> Arc<Block> {
        let mut hasher = DefaultHasher::new();
        let end = source.styled.properties().map_or(0, |p| p.block.end);
        source.doc.text()[..end].hash(&mut hasher);
        source
            .styled
            .tags()
            .iter()
            .for_each(|(_, name)| name.hash(&mut hasher));
        // Whether the note opens with its own heading: the title or not.
        opens_with_title(source.doc.text(), source.styled).hash(&mut hasher);
        (self.width.to_bits(), self.zoom.to_bits()).hash(&mut hasher);
        for color in [self.colors.text, self.colors.marker, self.colors.link] {
            color.into_rgba8().hash(&mut hasher);
        }
        let key = hasher.finish();
        if let Some((k, block)) = &self.block
            && *k == key
        {
            return block.clone();
        }
        let block = Arc::new(Block::new(
            source.doc.text(),
            source.styled,
            (self.colors, self.zoom),
            self.width,
        ));
        self.block = Some((key, block.clone()));
        block
    }

    /// Over the properties drawn for concealed front matter at `x`, `y` in
    /// the text area: the cross or "+ tag" there, if any, and where the
    /// caret goes to show the YAML (the title's end, or the closing fence).
    pub fn block_hit(&mut self, source: &Source, x: f32, y: f32) -> Option<(Option<Hit>, usize)> {
        folded(source)?;
        let (index, top) = self.line_at_y(source, y);
        if index != 0 {
            return None;
        }
        let properties = source.styled.properties()?;
        let caret = properties
            .title
            .as_ref()
            .map_or(properties.closing, |t| t.end);
        let control = self.block(source).hit(iced::Point::new(x, y - top));
        Some((control, caret))
    }
}

#[cfg(test)]
impl Block {
    /// Where each cross and "+ tag" is: its middle, in the block.
    pub fn controls(&self) -> Vec<(Hit, Point)> {
        self.chips
            .iter()
            .map(|chip| (Hit::Remove(chip.chip.clone()), chip.cross.center()))
            .chain([(Hit::Add, self.add.center())])
            .collect()
    }
}

/// Whether the note's first block after its front matter is a level-one
/// heading: it is the title then, and the front matter's is not drawn too.
pub fn opens_with_title(text: &str, styled: &Styled) -> bool {
    let Some(properties) = styled.properties() else {
        return false;
    };
    let mut at = properties.block.end;
    while at < text.len() {
        let rest = &text[at..];
        let end = at + rest.find(['\n', '\r']).unwrap_or(rest.len());
        if !text[at..end].trim().is_empty() {
            return heading_level(styled, at..end) == 1;
        }
        at = end + 1;
    }
    false
}

/// `YYYY-MM-DD` as "Friday, 2 October 2026"; anything else as written.
fn long_date(value: &str) -> String {
    let parts: Vec<i64> = value
        .split('-')
        .map(|part| part.parse().unwrap_or(-1))
        .collect();
    let [year, month, day] = parts[..] else {
        return value.to_owned();
    };
    if value.len() != 10 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return value.to_owned();
    }
    // Howard Hinnant's days from civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    const WEEKDAYS: [&str; 7] = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    format!(
        "{}, {day} {} {year}",
        WEEKDAYS[(days + 4).rem_euclid(7) as usize],
        MONTHS[month as usize - 1]
    )
}

#[cfg(test)]
mod tests {
    use super::long_date;

    #[test]
    fn dates_read_as_the_calendar_has_them() {
        assert_eq!(long_date("1970-01-01"), "Thursday, 1 January 1970");
        assert_eq!(long_date("2026-10-02"), "Friday, 2 October 2026");
        assert_eq!(long_date("2024-02-29"), "Thursday, 29 February 2024");
        assert_eq!(long_date("someday"), "someday");
        assert_eq!(long_date("2026-13-01"), "2026-13-01");
    }
}
