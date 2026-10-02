//! Links the host lists under the note's last line (PLAN-005): the app's
//! "Linked from", each note linking here with the line its link sits in.
//! Drawn in live preview and the rendered pane, under a rule; a click on
//! one hands its destination back as [`super::Message::link`].
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use iced::advanced::graphics::text::Renderer as _;
use iced::advanced::graphics::text::{Raw, cosmic_text};
use iced::advanced::renderer::{self, Renderer as _};
use iced::{Color, Point, Rectangle, Size, Vector};

use super::lines::{Lines, Source};
use super::properties::{Label, label};
use super::shape::Colors;
use super::{Editor, Pane};

/// A link the host lists under the note.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FooterLink {
    /// What it shows: another note's title, say.
    pub label: String,
    /// A quieter line beside it: the line the link sits in there.
    pub detail: String,
    /// Handed back as [`super::Message::link`] when it is clicked.
    pub destination: String,
}

/// The footer the host set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(super) struct Footer {
    heading: String,
    links: Vec<FooterLink>,
}

/// The footer laid out to a width.
pub struct Laid {
    labels: Vec<Label>,
    /// Each link's row, with its label's underline and destination.
    rows: Vec<(Rectangle, Rectangle, String)>,
    rule: f32,
    /// How tall it is, the room above it included.
    pub height: f32,
}

/// Room above the rule, under it and between rows, at 100%.
const ABOVE: f32 = 28.0;
const UNDER: f32 = 12.0;
const ROW: f32 = 6.0;

impl Editor {
    /// Links drawn under the note's last line, under `heading` (the app's
    /// "Linked from"); a click on one comes back as [`super::Message::link`]
    /// with its destination. No links, no footer.
    pub fn set_footer(&mut self, heading: &str, links: Vec<FooterLink>) {
        self.footer = (!links.is_empty()).then(|| Footer {
            heading: heading.to_owned(),
            links,
        });
    }

    /// The destination of the footer link under `at` in `pane`.
    pub(super) fn footer_under(&self, pane: Pane, at: Point) -> Option<String> {
        self.with_pane(pane, |lines, source| lines.footer_hit(source, at.x, at.y))
    }
}

impl Laid {
    fn new(footer: &Footer, (colors, zoom): (Colors, f32), width: f32) -> Self {
        let (normal, semibold) = (cosmic_text::Weight::NORMAL, cosmic_text::Weight::SEMIBOLD);
        let rule = ABOVE * zoom;
        let mut y = rule + UNDER * zoom;
        let mut heading = label(&footer.heading, 12.0 * zoom, semibold, colors.marker, width);
        heading.at = Point::new(0.0, y);
        y += heading.size.height + 4.0 * zoom;
        let mut labels = vec![heading];
        let mut rows = Vec::new();
        for link in &footer.links {
            let mut name = label(&link.label, 14.0 * zoom, normal, colors.link, width);
            name.at = Point::new(0.0, y);
            let beside = name.size.width + 12.0 * zoom;
            // Beside it when there is room, else under it.
            let (x, top, room) = if width - beside > 160.0 * zoom {
                (beside, y, width - beside)
            } else {
                (0.0, y + name.size.height, width)
            };
            let mut detail = label(&link.detail, 14.0 * zoom, normal, colors.marker, room);
            detail.at = Point::new(x, top);
            let bottom = (y + name.size.height).max(top + detail.size.height);
            let underline = Rectangle::new(
                Point::new(0.0, y + name.size.height * 0.82),
                Size::new(name.size.width, (zoom).max(1.0)),
            );
            let row = Rectangle::new(Point::new(0.0, y), Size::new(width, bottom - y));
            rows.push((row, underline, link.destination.clone()));
            labels.push(name);
            if !link.detail.is_empty() {
                labels.push(detail);
            }
            y = bottom + ROW * zoom;
        }
        Self {
            labels,
            rows,
            rule,
            height: y + 8.0 * zoom,
        }
    }

    /// Draws the footer with its top left at `origin`, `width` wide.
    pub fn draw(
        &self,
        renderer: &mut iced::Renderer,
        origin: Point,
        width: f32,
        clip: Rectangle,
        (link, line): (Color, Color),
    ) {
        let offset = Vector::new(origin.x, origin.y);
        let quad = |renderer: &mut iced::Renderer, bounds: Rectangle, color: Color| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    ..renderer::Quad::default()
                },
                color,
            );
        };
        quad(
            renderer,
            Rectangle::new(Point::new(0.0, self.rule), Size::new(width, 1.0)) + offset,
            line,
        );
        for (_, underline, _) in &self.rows {
            quad(renderer, *underline + offset, link);
        }
        for label in &self.labels {
            renderer.fill_raw(Raw {
                buffer: Arc::downgrade(&label.buffer),
                position: label.at + offset,
                color: Color::BLACK,
                clip_bounds: clip,
            });
        }
    }
}

impl Lines {
    /// The footer laid out, when the host set one and this pane shows it
    /// (live preview and the rendered pane, not the markdown as written).
    pub fn footer(&mut self, source: &Source) -> Option<Arc<Laid>> {
        let footer = source.footer.filter(|_| !self.source)?;
        let mut hasher = DefaultHasher::new();
        footer.hash(&mut hasher);
        (self.width.to_bits(), self.zoom.to_bits()).hash(&mut hasher);
        for color in [self.colors.marker, self.colors.link] {
            color.into_rgba8().hash(&mut hasher);
        }
        let key = hasher.finish();
        if let Some((k, laid)) = &self.laid_footer
            && *k == key
        {
            return Some(laid.clone());
        }
        let laid = Arc::new(Laid::new(footer, (self.colors, self.zoom), self.width));
        self.laid_footer = Some((key, laid.clone()));
        Some(laid)
    }

    /// The destination of the footer link at `x`, `y` in the text area.
    pub fn footer_hit(&mut self, source: &Source, x: f32, y: f32) -> Option<String> {
        let laid = self.footer(source)?;
        let last = source.doc.line_count() - 1;
        let (index, top) = self.line_at_y(source, y);
        if index != last {
            return None;
        }
        let height = self.shaped(source, last).height;
        let at = Point::new(x, y - (top + height - laid.height));
        laid.rows
            .iter()
            .find(|(row, _, _)| row.contains(at))
            .map(|(_, _, destination)| destination.clone())
    }
}

#[cfg(test)]
mod tests {
    use iced::Point;

    use super::FooterLink;
    use crate::widget::tests::outputs;
    use crate::widget::{Editor, Input, Message, Mode, Pane};

    fn last_height(editor: &Editor, pane: Pane) -> f32 {
        editor.with_pane(pane, |lines, source| {
            lines.shaped(source, source.doc.line_count() - 1).height
        })
    }

    #[test]
    fn the_footer_sits_under_the_last_line_and_its_links_are_followed() {
        let mut editor = Editor::new("One line.".into());
        for lines in [&editor.lines, &editor.preview.lines] {
            let mut lines = lines.borrow_mut();
            lines.outer = iced::Size::new(600.0, 400.0);
            lines.fit();
            lines.sized = true;
        }
        let bare = last_height(&editor, Pane::Text);
        editor.set_footer(
            "Linked from",
            vec![FooterLink {
                label: "Plan".into(),
                detail: "See hotels.".into(),
                destination: "sub/plan.md".into(),
            }],
        );
        let tall = last_height(&editor, Pane::Text);
        assert!(tall > bare + 60.0, "{bare} then {tall}");
        // A click on its link hands the destination back; the caret stays.
        let row = Point::new(10.0, tall - 20.0);
        let task = editor.update(Message(Input::Press {
            at: row,
            shift: false,
            clicks: 1,
            command: false,
            other: false,
        }));
        let followed: Vec<String> = outputs(task)
            .iter()
            .filter_map(|m| m.link().map(str::to_owned))
            .collect();
        assert_eq!(followed, ["sub/plan.md"]);
        assert_eq!(editor.selection().head, 0);
        // In the rendered pane too; not beside it in the markdown as written.
        editor.set_mode(Mode::Split);
        assert_eq!(last_height(&editor, Pane::Text), bare);
        let task = editor.update(Message(Input::PreviewPress {
            at: row,
            command: false,
        }));
        assert_eq!(outputs(task).len(), 1);
        // No links, no footer.
        editor.set_mode(Mode::Live);
        editor.set_footer("Linked from", Vec::new());
        assert_eq!(last_height(&editor, Pane::Text), bare);
    }
}
