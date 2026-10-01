//! The iced editor. [`Editor`] keeps the markdown, its styling and how it
//! is drawn; [`Editor::view`] shows it and [`Editor::update`] takes back the
//! messages the view produces (`input.rs`). Behaviour per REFERENCE-001.
mod draw;
mod highlight;
mod input;
mod lines;
mod surface;

use std::cell::RefCell;
use std::ops::Range;
use std::time::Instant;

use iced::advanced::widget::Id;
use iced::{Color, Element, Point, Rectangle, Size, Task};

use self::lines::{Colors, Lines, Source};
use crate::doc::{Doc, Selection};
use crate::edit::Motion;
use crate::edit::format::Format;
use crate::layout::Affinity;
use crate::style::Styled;

/// The id the editor widget takes, for [`Editor::focus`].
const ID: Id = Id::new("livemark-editor");

/// A markdown document being edited with live preview.
pub struct Editor {
    doc: Doc,
    styled: Styled,
    started: Instant,
    /// The side of a soft wrap the caret is drawn on: the row before it or
    /// the row after it (CodeMirror's `assoc`).
    side: Affinity,
    /// The mouse press being held.
    press: Option<Press>,
    /// The x the caret aims for moving up and down (REFERENCE-001
    /// section 13).
    goal_x: Option<f32>,
    /// The last copy taken from a line with nothing selected, so pasting
    /// it puts back a line (REFERENCE-001 section 18).
    linewise: Option<String>,
    lines: RefCell<Lines>,
}

/// A mouse press being held.
#[derive(Debug, Clone)]
struct Press {
    /// The selection whose reveal state stays drawn until the release, so
    /// text does not move under the pointer (REFERENCE-001 section 2).
    frozen: Selection,
    /// What a drag extends by (REFERENCE-001 section 14).
    unit: Unit,
    /// The word or line the press selected first.
    first: Range<usize>,
}

/// What a drag extends by: characters after a single click, words after a
/// double click, lines after a triple click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Char,
    Word,
    Line,
}

/// A message from the editor's view, for [`Editor::update`].
#[derive(Debug, Clone)]
pub struct Message(Input);

#[derive(Debug, Clone)]
enum Input {
    /// A press at a point of the text area: the click count (1 to 3), and
    /// Shift extending the selection.
    Press {
        at: Point,
        shift: bool,
        clicks: u8,
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
    /// Shift+Enter: a new line without new markup.
    SoftEnter,
    /// Tab, or Shift+Tab with `true`.
    Indent(bool),
    /// Ctrl/Cmd+B, I or E (REFERENCE-001 section 12).
    Format(Format),
    /// Ctrl/Cmd+K.
    Link,
    Delete(Motion),
    Move(Motion, bool),
    Vertical(Vertical, bool),
    SelectAll,
    /// Copy, or cut with `true` (REFERENCE-001 section 18).
    Copy(bool),
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
        lines::load_fonts();
        let doc = Doc::new(text);
        let styled = Styled::new(doc.text());
        Self {
            doc,
            styled,
            started: Instant::now(),
            side: Affinity::After,
            press: None,
            goal_x: None,
            linewise: None,
            lines: RefCell::new(Lines::new(Colors {
                text: Color::BLACK,
                marker: Color::BLACK,
                code: Color::BLACK,
                link: Color::BLACK,
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
        self.side = Affinity::After;
        self.with_lines(|lines, source| lines.reveal(source, selection.head, Affinity::After));
    }

    /// The editor, filling the space it is given.
    pub fn view(&self) -> Element<'_, Message> {
        Element::new(surface::Surface { editor: self })
    }

    /// A task that gives the editor keyboard focus.
    pub fn focus<T: Send + 'static>() -> Task<T> {
        iced::widget::operation::focus(ID)
    }

    /// The markers hidden with the selection drawn now.
    fn hidden(&self) -> Vec<Range<usize>> {
        let selection = self
            .press
            .as_ref()
            .map_or(self.doc.selection(), |press| press.frozen);
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
        let side = self.side;
        self.with_lines(|lines, source| {
            let top = lines.top_of(source, source.doc.line_at(head))?;
            let (x, row_top, height) = lines.caret_in_line(source, head, side);
            Some(Rectangle::new(
                Point::new(x, top + row_top),
                Size::new(1.0, height),
            ))
        })
    }
}

#[cfg(test)]
mod tests;
