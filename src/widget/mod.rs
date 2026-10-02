//! The iced editor. [`Editor`] keeps the markdown, its styling and how it
//! is drawn; [`Editor::view`] shows it and [`Editor::update`] takes back the
//! messages the view produces (`input.rs`). Behaviour per REFERENCE-001.
mod draw;
mod find;
mod highlight;
mod input;
mod keys;
mod lines;
mod marks;
mod scrollbar;
mod shape;
mod surface;
mod table;

use std::cell::RefCell;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use iced::advanced::widget::Id;
use iced::{Color, Element, Point, Rectangle, Size, Task};

use self::lines::{Lines, Source};
use self::shape::Colors;
use crate::doc::{Doc, Selection};
use crate::edit::Motion;
use crate::edit::format::Format;
use crate::layout::Affinity;
use crate::style::{Mark, Styled};

/// The id the editor widget takes, for [`Editor::focus`].
const ID: Id = Id::new("livemark-editor");

/// How the markdown is drawn (REFERENCE-001 section 17).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Live preview: markers hidden until the caret touches them,
    /// headings sized.
    #[default]
    Live,
    /// The markdown as written: one size, the code font throughout,
    /// nothing hidden, still highlighted.
    Source,
}

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
    /// The last press that ended, with the document version it was in: a
    /// second or third click hits the text as the first one saw it.
    last_press: Option<(Press, u64)>,
    /// The x the caret aims for moving up and down (REFERENCE-001
    /// section 13).
    goal_x: Option<f32>,
    /// The last copy taken from a line with nothing selected, so pasting
    /// it puts back a line (REFERENCE-001 section 18).
    linewise: Option<String>,
    /// The find bar, when it is open.
    find: Option<find::Find>,
    mode: Mode,
    /// Shift is held (`Input::Shift`).
    shift: bool,
    /// Whether the caret has been placed since the document was loaded:
    /// until then nothing is revealed (REFERENCE-001 section 2).
    placed: bool,
    lines: RefCell<Lines>,
    /// The last `reveal` worked out, by document version and selection:
    /// several calls per frame ask for it (backlog 12).
    reveal: RefCell<Option<RevealFor>>,
}

/// What live preview hides and draws over, and the document version and
/// selection it is for.
type RevealFor = (u64, Range<usize>, Reveal);

/// Hidden markers and concealed marks.
type Reveal = (Arc<[Range<usize>]>, Arc<[Mark]>);

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
    /// The first line drawn and its offset when the press began.
    view: (usize, f32),
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

impl Message {
    /// The destination of a link the user asked to follow (Ctrl/Cmd+click
    /// on it, or Alt+Enter with the caret in it), for the host to open;
    /// the editor does nothing with it.
    pub fn link(&self) -> Option<&str> {
        match &self.0 {
            Input::Follow(dest) => Some(dest),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
enum Input {
    /// A press at a point of the text area: the click count (1 to 3), and
    /// Shift extending the selection.
    Press {
        at: Point,
        shift: bool,
        clicks: u8,
        /// Ctrl, or Cmd on macOS: a click on a link follows it.
        command: bool,
        /// Any other modifier (Alt, the logo key, Control on macOS): with
        /// any modifier a checkbox is not toggled.
        other: bool,
    },
    /// A link to follow: the host's to open (REFERENCE-001 section 5).
    Follow(String),
    Drag(Point),
    Release,
    /// Pixels to scroll, positive further down.
    Scroll(f32),
    /// The scroll bar's thumb dragged or the track pressed: how far down
    /// its travel, from 0 to 1.
    ScrollTo(f32),
    /// Shift went down or up: Enter in the find field steps back while it
    /// is held (the field submits whatever the modifiers).
    Shift(bool),
    Key(Key),
    /// Text committed by an input method.
    Commit(String),
    Paste(String),
    Find(find::FindInput),
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
    /// Ctrl/Cmd+Shift+E: live preview or source mode (the user's pick).
    ToggleMode,
    /// Alt+Up, or Alt+Down with `true`: move the selected lines.
    MoveLines(bool),
    /// Shift+Alt+Up, or Shift+Alt+Down with `true`: copy them.
    CopyLines(bool),
    /// Ctrl/Cmd+Shift+K.
    DeleteLines,
    /// Ctrl/Cmd+Enter: a blank line below.
    BlankLine,
    /// Alt+Enter: follow the link at the caret.
    Follow,
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
        shape::load_fonts();
        let doc = Doc::new(text);
        let styled = Styled::new(doc.text());
        Self {
            doc,
            styled,
            started: Instant::now(),
            side: Affinity::After,
            press: None,
            last_press: None,
            goal_x: None,
            linewise: None,
            find: None,
            mode: Mode::Live,
            shift: false,
            placed: false,
            lines: RefCell::new(Lines::new(Colors {
                text: Color::BLACK,
                marker: Color::BLACK,
                code: Color::BLACK,
                link: Color::BLACK,
            })),
            reveal: RefCell::new(None),
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
        self.placed = true;
        self.goal_x = None;
        self.side = Affinity::After;
        self.with_lines(|lines, source| {
            // Before the first layout the view's size is not known yet.
            if lines.sized {
                lines.reveal(source, selection.head, Affinity::After);
            } else {
                lines.pending_reveal = Some((selection.head, Affinity::After));
            }
        });
    }

    /// How the markdown is drawn.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Switches between live preview and source mode, keeping the
    /// selection and the caret's row where it is on screen (REFERENCE-001
    /// section 17).
    pub fn set_mode(&mut self, mode: Mode) {
        if mode == self.mode {
            return;
        }
        self.mode = mode;
        self.keep_caret_row(|lines| lines.source = mode == Mode::Source);
    }

    /// The text size as a share of the default, 1.0 for 100%.
    pub fn zoom(&self) -> f32 {
        self.lines.borrow().zoom
    }

    /// Sets the text size as a share of the default, from 0.5 to 3.0
    /// (prose, headings, code and tables together), keeping the caret's
    /// row where it was on screen. Ctrl+=, Ctrl+- and Ctrl+0 in the app.
    pub fn set_zoom(&mut self, zoom: f32) {
        let zoom = zoom.clamp(0.5, 3.0);
        if zoom != self.zoom() {
            self.keep_caret_row(|lines| lines.zoom = zoom);
        }
    }

    /// Applies `change` to the layout, keeping the caret's row where it
    /// was on screen when it was in view.
    fn keep_caret_row(&mut self, change: impl FnOnce(&mut Lines)) {
        let (head, side) = (self.doc.selection().head, self.side);
        let caret = |lines: &mut Lines, source: &Source| {
            let row = lines.caret_in_line(source, head, side).1;
            (source.doc.line_at(head), row)
        };
        let before = self.with_lines(|lines, source| {
            let (index, row) = caret(lines, source);
            lines.top_of(source, index).map(|top| top + row)
        });
        change(&mut self.lines.borrow_mut());
        if let Some(y) = before {
            self.with_lines(|lines, source| {
                let (index, row) = caret(lines, source);
                lines.anchor = index;
                lines.offset = row - y;
                lines.scroll_by(source, 0.0);
            });
        }
    }

    /// The editor, filling the space it is given, with the find bar under
    /// the text when it is open.
    pub fn view(&self) -> Element<'_, Message> {
        // Always a column, so the surface keeps its state (focus, held
        // modifiers, a preedit) when the find bar opens or closes.
        let surface = Element::new(surface::Surface { editor: self });
        iced::widget::column![surface].push(self.find_bar()).into()
    }

    /// A task that gives the editor keyboard focus.
    pub fn focus<T: Send + 'static>() -> Task<T> {
        iced::widget::operation::focus(ID)
    }

    /// The markers hidden and the marks drawn over with the selection
    /// drawn now; none in source mode.
    fn reveal(&self) -> Reveal {
        if self.mode == Mode::Source {
            return (Arc::new([]), Arc::new([]));
        }
        let selection = if self.placed {
            self.press
                .as_ref()
                .map_or(self.doc.selection(), |press| press.frozen)
                .range()
        } else {
            // Touches nothing.
            usize::MAX..usize::MAX
        };
        let version = self.doc.version();
        let mut cache = self.reveal.borrow_mut();
        match &*cache {
            Some((v, s, reveal)) if *v == version && *s == selection => reveal.clone(),
            _ => {
                let reveal: Reveal = (
                    self.styled.hidden(selection.clone()).into(),
                    self.styled.concealed(selection.clone()).into(),
                );
                *cache = Some((version, selection, reveal.clone()));
                reveal
            }
        }
    }

    fn with_lines<R>(&self, f: impl FnOnce(&mut Lines, &Source) -> R) -> R {
        let (hidden, concealed) = self.reveal();
        let source = Source {
            doc: &self.doc,
            styled: &self.styled,
            hidden: &hidden,
            concealed: &concealed,
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
#[cfg(test)]
mod walkthrough_tests;
