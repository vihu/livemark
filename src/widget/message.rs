//! What the editor's view sends its `update`: the host's [`Message`],
//! wrapping the inputs, keys and motions the widget turns events into.
use iced::Point;

use super::preview::Pane;
use super::{Mode, complete, find};
use crate::edit::Motion;
use crate::edit::format::Format;

/// A message from the editor's view, for [`Editor::update`].
#[derive(Debug, Clone)]
pub struct Message(pub(super) Input);

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
pub(super) enum Input {
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
    /// A key or click on the completion list.
    Complete(complete::CompleteInput),
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
pub(super) enum Key {
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
    /// Ctrl/Cmd+T: the caret where a new tag goes in the front matter's
    /// list (PLAN-005).
    AddTag,
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
pub(super) enum Vertical {
    Up,
    Down,
    PageUp,
    PageDown,
}
