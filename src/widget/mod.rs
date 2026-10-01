//! The iced editor. [`Editor`] keeps the markdown, its styling and how it
//! is drawn; [`Editor::view`] shows it and [`Editor::update`] takes back the
//! messages the view produces. Behaviour per REFERENCE-001.
mod lines;
mod surface;

use std::cell::RefCell;
use std::ops::Range;
use std::time::Instant;

use iced::advanced::graphics::text::Raw;
use iced::advanced::graphics::text::Renderer as _;
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::Id;
use iced::{Color, Element, Point, Rectangle, Size, Task, Theme, Vector};

use self::lines::{Colors, Lines, Source};
use crate::doc::{Doc, Selection};
use crate::edit::{self, Motion};
use crate::style::Styled;

/// The id the editor widget takes, for [`Editor::focus`].
const ID: Id = Id::new("livemark-editor");

/// A markdown document being edited with live preview.
pub struct Editor {
    doc: Doc,
    styled: Styled,
    started: Instant,
    /// The selection whose reveal state is drawn while a mouse button is
    /// held, so text does not move under the pointer (REFERENCE-001
    /// section 2).
    frozen: Option<Selection>,
    /// The x the caret aims for moving up and down (REFERENCE-001
    /// section 13).
    goal_x: Option<f32>,
    lines: RefCell<Lines>,
}

/// A message from the editor's view, for [`Editor::update`].
#[derive(Debug, Clone)]
pub struct Message(Input);

#[derive(Debug, Clone)]
enum Input {
    /// A press at a point of the text area; Shift extends the selection.
    Press {
        at: Point,
        shift: bool,
    },
    Drag(Point),
    Release,
    /// Pixels to scroll, positive further down.
    Scroll(f32),
    Key(Key),
    /// Text committed by an input method.
    Commit(String),
    Paste(String),
}

#[derive(Debug, Clone, Copy)]
enum Key {
    Insert(char),
    Enter,
    Delete(Motion),
    Move(Motion, bool),
    Vertical(Vertical, bool),
    SelectAll,
    Undo,
    Redo,
    /// Escape: collapse the selection (REFERENCE-001 section 13).
    Collapse,
}

#[derive(Debug, Clone, Copy)]
enum Vertical {
    Up,
    Down,
    PageUp,
    PageDown,
}

impl Editor {
    /// An editor holding `text` exactly, caret at the start.
    pub fn new(text: String) -> Self {
        let doc = Doc::new(text);
        let styled = Styled::new(doc.text());
        Self {
            doc,
            styled,
            started: Instant::now(),
            frozen: None,
            goal_x: None,
            lines: RefCell::new(Lines::new(Colors {
                text: Color::BLACK,
                marker: Color::BLACK,
                code: Color::BLACK,
            })),
        }
    }

    /// The markdown, byte for byte.
    pub fn text(&self) -> &str {
        self.doc.text()
    }

    /// Changes on every edit and comes back with undo; compare with the
    /// value at the last save for autosave.
    pub fn version(&self) -> u64 {
        self.doc.version()
    }

    /// The caret and selection as byte offsets into [`Editor::text`].
    pub fn selection(&self) -> Selection {
        self.doc.selection()
    }

    /// Selects from `anchor` to `head` (byte offsets, clamped to the text
    /// and moved back to a character boundary) and scrolls to show `head`.
    pub fn select(&mut self, anchor: usize, head: usize) {
        let text = self.doc.text();
        let fit = |at: usize| {
            let mut at = at.min(text.len());
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            at
        };
        let selection = Selection {
            anchor: fit(anchor),
            head: fit(head),
        };
        self.doc.set_selection(selection);
        self.goal_x = None;
        self.with_lines(|lines, source| lines.reveal(source, selection.head));
    }

    /// The editor, filling the space it is given.
    pub fn view(&self) -> Element<'_, Message> {
        Element::new(surface::Surface { editor: self })
    }

    /// A task that gives the editor keyboard focus.
    pub fn focus<T: Send + 'static>() -> Task<T> {
        iced::widget::operation::focus(ID)
    }

    /// Applies a message from [`Editor::view`].
    pub fn update(&mut self, Message(input): Message) {
        let now = self.started.elapsed();
        let version = self.doc.version();
        let mut vertical = false;
        match input {
            Input::Press { at, shift } => {
                let offset = self.with_lines(|lines, source| lines.hit(source, at.x, at.y));
                self.frozen = Some(self.doc.selection());
                let anchor = if shift {
                    self.doc.selection().anchor
                } else {
                    offset
                };
                self.doc.set_selection(Selection {
                    anchor,
                    head: offset,
                });
            }
            Input::Drag(at) => {
                let head = self.with_lines(|lines, source| lines.hit(source, at.x, at.y));
                let anchor = self.doc.selection().anchor;
                self.doc.set_selection(Selection { anchor, head });
            }
            Input::Release => self.frozen = None,
            Input::Scroll(dy) => {
                self.with_lines(|lines, source| lines.scroll_by(source, dy));
                return;
            }
            Input::Commit(text) => edit::type_text(&mut self.doc, &text, now),
            Input::Paste(text) => edit::paste(&mut self.doc, &text, now),
            Input::Key(key) => match key {
                Key::Insert(c) => edit::type_text(&mut self.doc, c.encode_utf8(&mut [0; 4]), now),
                Key::Enter => edit::enter(&mut self.doc, now),
                Key::Delete(motion) => edit::delete(&mut self.doc, motion, now),
                Key::Move(motion, extend) => edit::go(&mut self.doc, motion, extend),
                Key::Vertical(direction, extend) => {
                    self.vertical(direction, extend);
                    vertical = true;
                }
                Key::SelectAll => self.doc.set_selection(Selection {
                    anchor: 0,
                    head: self.doc.text().len(),
                }),
                Key::Undo => {
                    self.doc.undo();
                }
                Key::Redo => {
                    self.doc.redo();
                }
                Key::Collapse => {
                    let head = self.doc.selection().head;
                    self.doc.set_selection(Selection::caret(head));
                }
            },
        }
        if self.doc.version() != version {
            self.styled = Styled::new(self.doc.text());
        }
        if !vertical {
            self.goal_x = None;
        }
        let head = self.doc.selection().head;
        self.with_lines(|lines, source| lines.reveal(source, head));
    }

    /// Moves the caret a row or a page up or down, aiming for the same x.
    fn vertical(&mut self, direction: Vertical, extend: bool) {
        let selection = self.doc.selection();
        let goal = self.goal_x;
        let (x, head) = self.with_lines(|lines, source| {
            let index = source.doc.line_at(selection.head);
            let top = lines.top_of(source, index).unwrap_or(0.0);
            let (x, row_top, row_height) = lines.caret_in_line(source, selection.head);
            let x = goal.unwrap_or(x);
            let row = top + row_top;
            let y = match direction {
                Vertical::Up => row - 1.0,
                Vertical::Down => row + row_height + 1.0,
                Vertical::PageUp => row - lines.height,
                Vertical::PageDown => row + lines.height,
            };
            (x, lines.hit(source, x, y))
        });
        self.goal_x = Some(x);
        let anchor = if extend { selection.anchor } else { head };
        self.doc.set_selection(Selection { anchor, head });
    }

    /// The markers hidden with the selection drawn now.
    fn hidden(&self) -> Vec<Range<usize>> {
        let selection = self.frozen.unwrap_or(self.doc.selection());
        self.styled.hidden(selection.range())
    }

    fn with_lines<R>(&self, f: impl FnOnce(&mut Lines, &Source) -> R) -> R {
        let hidden = self.hidden();
        let source = Source {
            doc: &self.doc,
            styled: &self.styled,
            hidden: &hidden,
        };
        let mut lines = self.lines.borrow_mut();
        lines.anchor = lines.anchor.min(self.doc.line_count() - 1);
        f(&mut lines, &source)
    }

    /// The caret as a rectangle in the text area, if it is on screen.
    fn caret(&self) -> Option<Rectangle> {
        let head = self.doc.selection().head;
        self.with_lines(|lines, source| {
            let top = lines.top_of(source, source.doc.line_at(head))?;
            let (x, row_top, height) = lines.caret_in_line(source, head);
            Some(Rectangle::new(
                Point::new(x, top + row_top),
                Size::new(1.0, height),
            ))
        })
    }

    /// Draws the visible lines into `area` with the selection, and the
    /// caret when `caret` is set.
    fn draw(&self, renderer: &mut iced::Renderer, theme: &Theme, area: Rectangle, caret: bool) {
        let palette = theme.palette();
        let text = palette.background.base.text;
        let colors = Colors {
            text,
            marker: Color { a: 0.4, ..text },
            code: palette.primary.base.color,
        };
        let selection_color = palette.primary.weak.color;
        let selection = self.doc.selection().range();
        let caret_rect = caret.then(|| self.caret()).flatten();
        self.with_lines(|lines, source| {
            lines.colors = colors;
            let mut drawn = Vec::new();
            let mut index = lines.anchor;
            let mut top = -lines.offset;
            while top < lines.height && index < source.doc.line_count() {
                let shaped = lines.shaped(source, index);
                let origin = area.position() + Vector::new(0.0, top);
                let range = source.doc.line_range(index);
                if !selection.is_empty()
                    && selection.start <= range.end
                    && range.start <= selection.end
                {
                    let from = shaped.line.to_display(selection.start.max(range.start));
                    let to = shaped.line.to_display(selection.end.min(range.end));
                    let (start, end) = (lines::cursor(from), lines::cursor(to));
                    for run in shaped.buffer.layout_runs() {
                        for (x, width) in run.highlight(start, end) {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle::new(
                                        origin + Vector::new(x, run.line_top),
                                        Size::new(width, run.line_height),
                                    ),
                                    ..renderer::Quad::default()
                                },
                                selection_color,
                            );
                        }
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
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        ..renderer::Quad::default()
                    },
                    text,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
