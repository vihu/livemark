//! The iced editor. [`Editor`] keeps the markdown, its styling and how it
//! is drawn; [`Editor::view`] shows it and [`Editor::update`] takes back the
//! messages the view produces (`input.rs`). Behaviour per REFERENCE-001.
mod divider;
mod draw;
mod find;
mod highlight;
mod icons;
mod input;
mod keys;
mod lines;
mod marks;
mod picture;
mod preview;
mod scrollbar;
mod shape;
mod surface;
mod table;
mod toolbar;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use iced::advanced::widget::Id;
use iced::{Color, Element, Point, Rectangle, Size, Task};

use self::lines::{Lines, Source};
use self::preview::Pane;
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
    /// The markdown as written beside the rendered note: live preview
    /// with every marker hidden, the two scrolled together (PLAN-003).
    Split,
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
    /// Pictures the host supplied, by image destination (`set_image`).
    pictures: HashMap<String, picture::Picture>,
    /// Split mode's rendered pane.
    preview: preview::Preview,
    /// The share of the width the text takes in Split mode.
    split_ratio: f32,
}

/// What live preview hides and draws over, and the document version and
/// selection it is for.
type RevealFor = (u64, Range<usize>, Reveal);

/// Hidden markers and concealed marks.
type Reveal = (Arc<[Range<usize>]>, Arc<[Mark]>);

/// Colors until the first draw gives the theme's.
const BLACK: Colors = Colors {
    text: Color::BLACK,
    marker: Color::BLACK,
    code: Color::BLACK,
    link: Color::BLACK,
};

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

    /// The name of an inline `#tag` (without its `#`) the user asked to
    /// see (Ctrl/Cmd+click on it, Alt+Enter in it), for the host to show
    /// what has it (PLAN-004); the editor does nothing with it.
    pub fn tag(&self) -> Option<&str> {
        match &self.0 {
            Input::Tag(name) => Some(name),
            _ => None,
        }
    }

    /// A picture the user pasted (the clipboard held no text), as PNG
    /// bytes, for the host to keep somewhere and answer with the markdown to
    /// insert (`Editor::insert_text`); the editor does nothing with it
    /// (PLAN-001 host hook 3).
    pub fn pasted_image(&self) -> Option<Vec<u8>> {
        let Input::PastedImage(image) = &self.0 else {
            return None;
        };
        let (width, height) = (image.size.width, image.size.height);
        let rgba = image::RgbaImage::from_raw(width, height, image.rgba.to_vec())?;
        let mut png = Vec::new();
        rgba.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        Some(png)
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
    /// A `#tag` to show: the host's (PLAN-004).
    Tag(String),
    Drag(Point),
    Release,
    /// A press in side by side's rendered pane, at a point of its text
    /// area; Ctrl, or Cmd on macOS, held.
    PreviewPress {
        at: Point,
        command: bool,
    },
    /// Pixels to scroll a pane, positive further down.
    Scroll(Pane, f32),
    /// A pane's scroll bar thumb dragged or its track pressed: how far
    /// down its travel, from 0 to 1.
    ScrollTo(Pane, f32),
    /// The divider dragged, or double-clicked (0.5): the text's share of
    /// the width.
    SplitRatio(f32),
    /// Ctrl/Cmd with the wheel: notches up (positive) or down, a tenth of
    /// the text size each, as the app's keys step.
    ZoomSteps(f32),
    /// Ctrl/Cmd with a touchpad: the text size times this, smoothly.
    ZoomBy(f32),
    /// Shift went down or up: Enter in the find field steps back while it
    /// is held (the field submits whatever the modifiers).
    Shift(bool),
    Key(Key),
    /// A toolbar button: its key's command, then the focus back to the
    /// text.
    Tool(Key),
    /// Text committed by an input method.
    Commit(String),
    Paste(String),
    /// A picture pasted: the host's to save (`Message::pasted_image`).
    PastedImage(iced::advanced::clipboard::Image),
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
    /// A toolbar button: a line prefix on or off (`edit::blocks`).
    Block(crate::edit::blocks::Block),
    /// A toolbar button: this mode.
    SetMode(Mode),
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
            lines: RefCell::new(Lines::new(BLACK)),
            reveal: RefCell::new(None),
            pictures: HashMap::new(),
            preview: preview::Preview::new(BLACK),
            split_ratio: 0.5,
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
        self.preview.leader.set(Pane::Text);
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
        let split = (self.mode == Mode::Split) != (mode == Mode::Split);
        self.mode = mode;
        self.keep_caret_row(|lines| lines.source = mode != Mode::Live);
        // Into or out of Split the text's width changes at the next layout:
        // the caret is brought into view then.
        if split {
            self.lines.borrow_mut().pending_reveal = Some((self.doc.selection().head, self.side));
        }
        self.preview.leader.set(Pane::Text);
    }

    /// The share of the width the markdown takes beside the rendered note
    /// (`Mode::Split`), 0.5 by default.
    pub fn split_ratio(&self) -> f32 {
        self.split_ratio
    }

    /// Sets the share of the width the markdown takes beside the rendered
    /// note, from 0.2 to 0.8; the divider between them drags it, and a
    /// double click on it puts it back to 0.5. A host keeps it between
    /// starts.
    pub fn set_split_ratio(&mut self, ratio: f32) {
        let min = divider::MIN_SHARE;
        if ratio.is_finite() {
            self.split_ratio = ratio.clamp(min, 1.0 - min);
        }
    }

    /// Supplies the picture for images pointing at `url` (as written in
    /// `![alt](url)`): encoded bytes, PNG, JPEG, GIF or WebP. The picture is
    /// drawn under the image's line while its markdown hides (REFERENCE-001
    /// section 5); bytes that do not decode are ignored and the markdown
    /// stays.
    pub fn set_image(&mut self, url: &str, bytes: &[u8]) {
        if let Some(picture) = picture::decode(bytes) {
            self.pictures.insert(url.to_owned(), picture);
            *self.reveal.borrow_mut() = None;
            *self.preview.reveal.borrow_mut() = None;
        }
    }

    /// Inserts `text` at the caret, over the selection, as one undo step
    /// (as a paste of it): what a host answers a pasted picture with.
    pub fn insert_text(&mut self, text: &str) {
        let _ = self.update(Message(Input::Paste(text.to_owned())));
    }

    /// Where the images in the text point, each once, in order: what a
    /// host loads and hands to [`Editor::set_image`].
    pub fn image_urls(&self) -> Vec<String> {
        let mut urls: Vec<String> = Vec::new();
        for (_, url) in self.styled.images() {
            if !urls.contains(url) {
                urls.push(url.clone());
            }
        }
        urls
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
            let mut preview = self.preview.lines.borrow_mut();
            preview.zoom = zoom;
            if preview.sized {
                preview.fit();
            }
            self.preview.leader.set(Pane::Text);
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
        {
            // The gutter grows with the zoom: the text's width follows now,
            // not at the next layout, or lines above the caret re-wrap
            // after it was put back.
            let mut lines = self.lines.borrow_mut();
            change(&mut lines);
            if lines.sized {
                lines.fit();
            }
        }
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
        let surface = Element::new(surface::Surface {
            editor: self,
            pane: Pane::Text,
        });
        // In Split, the divider and the rendered pane beside it; always a
        // row, so the surface keeps its place.
        let split = self.mode == Mode::Split;
        let divider = split.then(|| Element::from(divider::Divider { editor: self }));
        let preview = split.then(|| {
            Element::new(surface::Surface {
                editor: self,
                pane: Pane::Preview,
            })
        });
        let text = iced::widget::row![surface]
            .push(divider)
            .push(preview)
            .height(iced::Length::Fill);
        iced::widget::column![text].push(self.find_bar()).into()
    }

    /// A task that gives the editor keyboard focus.
    pub fn focus<T: Send + 'static>() -> Task<T> {
        iced::widget::operation::focus(ID)
    }

    /// The markers hidden and the marks drawn over with the selection
    /// drawn now; none in source mode.
    fn reveal(&self) -> Reveal {
        if self.mode != Mode::Live {
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
                let reveal = self.reveal_for(selection.clone());
                *cache = Some((version, selection, reveal.clone()));
                reveal
            }
        }
    }

    /// The markers hidden and the marks drawn over with `selection`.
    fn reveal_for(&self, selection: Range<usize>) -> Reveal {
        // An image with its picture hides all its markdown while untouched
        // (REFERENCE-001 section 5).
        let mut hidden = self.styled.hidden(selection.clone());
        let touches = |r: &Range<usize>| selection.start <= r.end && r.start <= selection.end;
        let images = self.styled.images().iter();
        hidden.extend(
            images
                .filter(|(r, url)| self.pictures.contains_key(url) && !touches(r))
                .map(|(r, _)| r.clone()),
        );
        hidden.sort_by_key(|h| h.start);
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(hidden.len());
        for h in hidden {
            match merged.last_mut() {
                Some(last) if h.start <= last.end => last.end = last.end.max(h.end),
                _ => merged.push(h),
            }
        }
        (merged.into(), self.styled.concealed(selection).into())
    }

    fn with_lines<R>(&self, f: impl FnOnce(&mut Lines, &Source) -> R) -> R {
        self.with_pane(Pane::Text, f)
    }

    /// `f` over a pane's lines, with what that pane hides.
    fn with_pane<R>(&self, pane: Pane, f: impl FnOnce(&mut Lines, &Source) -> R) -> R {
        let ((hidden, concealed), lines) = match pane {
            Pane::Text => (self.reveal(), &self.lines),
            Pane::Preview => (self.preview_reveal(), &self.preview.lines),
        };
        let source = Source {
            doc: &self.doc,
            styled: &self.styled,
            hidden: &hidden,
            concealed: &concealed,
            pictures: &self.pictures,
        };
        let mut lines = lines.borrow_mut();
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
mod split_tests;
#[cfg(test)]
mod tag_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod walkthrough_tests;
